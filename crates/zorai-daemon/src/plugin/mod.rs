pub mod api_proxy;
pub mod bundled;
pub mod commands;
pub mod crypto;
pub mod loader;
pub mod local_run;
mod manager_extras;
pub mod manifest;
pub mod oauth2;
pub mod persistence;
pub mod rate_limiter;
pub mod schema;
pub mod skills;
pub mod ssrf;
pub mod template;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::RwLock;

use manager_extras::{extract_settings_schema, to_plugin_info_from_record};

pub use api_proxy::PluginApiError;
pub use loader::LoadedPlugin;
pub(crate) use manager_extras::{PluginAuthHealthIssue, PluginAuthStatus};
pub use persistence::{PluginPersistence, PluginRecord};

/// Manages plugin lifecycle: loading, validation, persistence, queries, and API proxy.
/// Initialized once in server.rs, shared via Arc.
pub struct PluginManager {
    plugins: RwLock<HashMap<String, LoadedPlugin>>,
    persistence: PluginPersistence,
    plugins_dir: PathBuf,
    /// Root directory for skill files (`~/.zorai/skills/`).
    skills_root: PathBuf,
    schema_validator: jsonschema::Validator,
    /// Shared HTTP client for plugin API proxy requests.
    http_client: reqwest::Client,
    /// Per-plugin token bucket rate limiters (lazy-initialized).
    rate_limiters: tokio::sync::Mutex<rate_limiter::RateLimiterMap>,
    /// Handlebars template registry with custom helpers for request/response rendering.
    template_registry: handlebars::Handlebars<'static>,
    /// Registry of plugin slash commands, rebuilt on plugin changes.
    command_registry: RwLock<commands::PluginCommandRegistry>,
    /// Per-plugin Mutex for serializing concurrent token refresh attempts (Pitfall 7).
    oauth_refresh_locks: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    #[cfg(test)]
    test_api_call_delay: tokio::sync::Mutex<Option<Duration>>,
}

impl PluginManager {
    /// Create a new PluginManager. Does NOT load plugins yet -- call load_all_from_disk().
    pub fn new(history: Arc<crate::history::HistoryStore>, plugins_dir: PathBuf) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let skills_root = plugins_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join("skills");

        Self {
            plugins: RwLock::new(HashMap::new()),
            persistence: PluginPersistence::new(history),
            plugins_dir,
            skills_root,
            schema_validator: schema::compile_schema_v1(),
            http_client,
            rate_limiters: tokio::sync::Mutex::new(rate_limiter::RateLimiterMap::new()),
            template_registry: template::create_registry(),
            command_registry: RwLock::new(commands::PluginCommandRegistry::new()),
            oauth_refresh_locks: tokio::sync::Mutex::new(HashMap::new()),
            #[cfg(test)]
            test_api_call_delay: tokio::sync::Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub async fn set_test_api_call_delay(&self, delay: Duration) {
        *self.test_api_call_delay.lock().await = Some(delay);
    }

    #[cfg(test)]
    pub async fn test_api_call_delay(&self) -> Option<Duration> {
        *self.test_api_call_delay.lock().await
    }

    /// Load all plugins from disk, validate, persist to SQLite, reconcile stale records.
    /// Per D-09: skip and warn on failures.
    /// Returns (loaded_count, skipped_count).
    pub async fn load_all_from_disk(&self) -> (usize, usize) {
        if let Err(e) = std::fs::create_dir_all(&self.plugins_dir) {
            tracing::warn!(
                path = %self.plugins_dir.display(),
                error = %e,
                "failed to create plugins directory"
            );
            return (0, 0);
        }

        let scan = loader::scan_plugins_dir(&self.plugins_dir, &self.schema_validator);
        let loaded_count = scan.loaded.len();
        let skipped_count = scan.skipped.len();

        let now = chrono::Utc::now().to_rfc3339();
        let mut active_names = Vec::with_capacity(loaded_count);
        let mut plugins_map = HashMap::with_capacity(loaded_count);

        for plugin in scan.loaded {
            let record = PluginRecord {
                name: plugin.manifest.name.clone(),
                version: plugin.manifest.version.clone(),
                description: plugin.manifest.description.clone(),
                author: plugin.manifest.author.clone(),
                manifest_json: plugin.manifest_json.clone(),
                install_source: "local".to_string(),
                enabled: true,
                installed_at: now.clone(),
                updated_at: now.clone(),
            };

            if let Err(e) = self.persistence.upsert_plugin(&record).await {
                tracing::warn!(
                    plugin = %record.name,
                    error = %e,
                    "failed to persist plugin record"
                );
                continue;
            }

            active_names.push(plugin.manifest.name.clone());
            plugins_map.insert(plugin.manifest.name.clone(), plugin);
        }

        if let Err(e) = self.persistence.remove_stale_plugins(&active_names).await {
            tracing::warn!(error = %e, "failed to reconcile stale plugin records");
        }

        let enabled = self.enabled_names().await;
        for (name, plugin) in &plugins_map {
            let result = if enabled.contains(name) {
                skills::install_bundled_skills(
                    &self.plugins_dir,
                    name,
                    &plugin.manifest,
                    &self.skills_root,
                )
                .map(|_| ())
            } else {
                skills::remove_bundled_skills(name, &self.skills_root)
            };
            if let Err(e) = result {
                tracing::warn!(plugin = %name, error = %e, "failed to sync bundled skills");
            }
        }

        *self.plugins.write().await = plugins_map;

        self.rebuild_command_registry().await;

        tracing::info!(
            loaded = loaded_count,
            skipped = skipped_count,
            "plugin loader: loaded {} plugins ({} skipped)",
            loaded_count,
            skipped_count
        );

        (loaded_count, skipped_count)
    }

    /// List all plugins (from SQLite for accurate enabled state).
    pub async fn list_plugins(&self) -> Vec<zorai_protocol::PluginInfo> {
        let plugins = self.plugins.read().await;
        let records = match self.persistence.list_plugins().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "failed to list plugins from database");
                return Vec::new();
            }
        };

        let mut result = Vec::with_capacity(records.len());
        for rec in &records {
            let loaded = plugins.get(&rec.name);
            let auth_status = self
                .persistence
                .get_auth_status(&rec.name)
                .await
                .unwrap_or_else(|_| "not_configured".to_string());
            let settings = self
                .persistence
                .get_settings(&rec.name)
                .await
                .unwrap_or_default();
            result.push(to_plugin_info_from_record(
                rec,
                loaded,
                auth_status,
                &settings,
            ));
        }
        result
    }

    /// Get a single plugin by name.
    /// Returns (PluginInfo, settings_schema_json).
    pub async fn get_plugin(
        &self,
        name: &str,
    ) -> Option<(zorai_protocol::PluginInfo, Option<String>)> {
        let record = match self.persistence.get_plugin(name).await {
            Ok(Some(r)) => r,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(plugin = %name, error = %e, "failed to get plugin from database");
                return None;
            }
        };

        let plugins = self.plugins.read().await;
        let loaded = plugins.get(name);
        let auth_status = self
            .persistence
            .get_auth_status(name)
            .await
            .unwrap_or_else(|_| "not_configured".to_string());
        let settings = self
            .persistence
            .get_settings(name)
            .await
            .unwrap_or_default();
        let info = to_plugin_info_from_record(&record, loaded, auth_status, &settings);

        let settings_schema = extract_settings_schema(&record.manifest_json);

        Some((info, settings_schema))
    }

    /// Enable or disable a plugin.
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        self.persistence.set_enabled(name, enabled).await?;
        let manifest = self
            .plugins
            .read()
            .await
            .get(name)
            .map(|plugin| plugin.manifest.clone());
        if let Some(manifest) = manifest {
            let result = if enabled {
                skills::install_bundled_skills(
                    &self.plugins_dir,
                    name,
                    &manifest,
                    &self.skills_root,
                )
                .map(|_| ())
            } else {
                skills::remove_bundled_skills(name, &self.skills_root)
            };
            if let Err(e) = result {
                tracing::warn!(plugin = %name, error = %e, "failed to sync bundled skills");
            }
        }
        self.rebuild_command_registry().await;
        Ok(())
    }

    pub(crate) async fn enabled_names(&self) -> std::collections::HashSet<String> {
        self.persistence
            .list_plugins()
            .await
            .map(|records| {
                records
                    .into_iter()
                    .filter(|record| record.enabled)
                    .map(|record| record.name)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Install bundled plugins that are missing (disabled) and update the ones
    /// that came from the bundle; plugins installed any other way are left alone.
    pub async fn sync_bundled_plugins(&self, bundle_root: &Path) -> bundled::BundleSyncReport {
        let mut report = bundled::BundleSyncReport::default();
        if std::fs::create_dir_all(&self.plugins_dir).is_err() {
            return report;
        }
        for source in bundled::find_bundled_plugins(bundle_root) {
            let Ok(raw) = std::fs::read(source.join("plugin.json")) else {
                continue;
            };
            let (manifest, manifest_json) = match loader::validate_manifest(
                &raw,
                &self.schema_validator,
            ) {
                Ok(validated) => validated,
                Err(e) => {
                    tracing::warn!(path = %source.display(), error = %e, "skipping invalid bundled plugin");
                    continue;
                }
            };
            let target = self.plugins_dir.join(&manifest.name);
            let fresh = match self.persistence.plugin_origin(&manifest.name).await {
                Ok(None) if !target.exists() => true,
                Ok(Some((origin, version, false))) if origin == bundled::BUNDLED_SOURCE => {
                    if version == manifest.version && target.join("plugin.json").is_file() {
                        continue;
                    }
                    false
                }
                _ => continue,
            };
            if let Err(e) = bundled::replace_plugin_files(&source, &target) {
                tracing::warn!(plugin = %manifest.name, error = %e, "failed to copy bundled plugin");
                continue;
            }
            let now = chrono::Utc::now().to_rfc3339();
            let record = PluginRecord {
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                description: manifest.description.clone(),
                author: manifest.author.clone(),
                manifest_json,
                install_source: bundled::BUNDLED_SOURCE.to_string(),
                enabled: false,
                installed_at: now.clone(),
                updated_at: now,
            };
            if let Err(e) = self.persistence.upsert_plugin(&record).await {
                tracing::warn!(plugin = %manifest.name, error = %e, "failed to record bundled plugin");
                continue;
            }
            if fresh {
                report.installed.push(manifest.name);
            } else {
                report.updated.push(manifest.name);
            }
        }
        report
    }

    /// Check if a manifest's commands or skills conflict with existing plugins.
    /// Returns Ok(()) if no conflicts, Err with conflict details if any found.
    /// Namespace convention: commands are namespaced as /pluginname.command per PSKL-05.
    /// Conflicts happen when two different plugins declare the same command key or skill path.
    /// Register a plugin from its directory. Validates manifest,
    /// persists to SQLite, and adds to in-memory map.
    /// Called by the PluginInstall IPC handler after CLI has copied files to disk.
    pub async fn register_plugin(
        &self,
        dir_name: &str,
        install_source: &str,
    ) -> Result<zorai_protocol::PluginInfo> {
        let manifest_path = self.plugins_dir.join(dir_name).join("plugin.json");
        let raw_bytes = std::fs::read(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;

        let (manifest, manifest_json) =
            loader::validate_manifest(&raw_bytes, &self.schema_validator)?;

        let now = chrono::Utc::now().to_rfc3339();
        let record = PluginRecord {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            author: manifest.author.clone(),
            manifest_json: manifest_json.clone(),
            install_source: install_source.to_string(),
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
        };

        self.persistence.upsert_plugin(&record).await?;

        let auth_status = self
            .persistence
            .get_auth_status(&manifest.name)
            .await
            .unwrap_or_else(|_| "not_configured".to_string());
        let info = to_plugin_info_from_record(
            &record,
            Some(&loader::LoadedPlugin {
                manifest: manifest.clone(),
                manifest_json,
                dir_name: dir_name.to_string(),
            }),
            auth_status,
            &[],
        );

        if let Err(e) = skills::install_bundled_skills(
            &self.plugins_dir,
            &manifest.name,
            &manifest,
            &self.skills_root,
        ) {
            tracing::warn!(plugin = %manifest.name, error = %e, "failed to install bundled skills");
        }

        self.plugins.write().await.insert(
            manifest.name.clone(),
            loader::LoadedPlugin {
                manifest,
                manifest_json: record.manifest_json,
                dir_name: dir_name.to_string(),
            },
        );

        self.rebuild_command_registry().await;

        tracing::info!(plugin = %record.name, source = %install_source, "plugin registered");
        Ok(info)
    }

    /// Unregister a plugin: remove from SQLite (plugins + settings + credentials)
    /// and from in-memory map. Does NOT delete files from disk (CLI handles that).
    pub async fn unregister_plugin(&self, name: &str) -> Result<()> {
        if let Err(e) = skills::remove_bundled_skills(name, &self.skills_root) {
            tracing::warn!(plugin = %name, error = %e, "failed to remove bundled skills");
        }

        let existed = self.persistence.remove_plugin(name).await?;
        if !existed {
            return Err(anyhow::anyhow!("plugin '{}' not found", name));
        }
        self.plugins.write().await.remove(name);

        self.rebuild_command_registry().await;

        tracing::info!(plugin = %name, "plugin unregistered");
        Ok(())
    }

    /// Get settings for a plugin. Masks secret values for display. Per PSET-04/PSET-06.
    pub async fn get_settings(&self, name: &str) -> Vec<(String, String, bool)> {
        match self.persistence.get_settings(name).await {
            Ok(settings) => settings
                .into_iter()
                .map(|(k, v, secret)| {
                    if secret {
                        (k, "********".to_string(), true)
                    } else {
                        (k, v, false)
                    }
                })
                .collect(),
            Err(e) => {
                tracing::warn!(plugin = %name, error = %e, "failed to get plugin settings");
                Vec::new()
            }
        }
    }

    /// Update a single setting value. Per PSET-06/D-06.
    pub async fn update_setting(
        &self,
        plugin_name: &str,
        key: &str,
        value: &str,
        is_secret: bool,
    ) -> Result<()> {
        self.persistence
            .upsert_setting(plugin_name, key, value, is_secret)
            .await
    }

    /// Test connectivity using a connector readiness endpoint when available,
    /// otherwise fall back to a lightweight HEAD probe.
    /// Per PSET-05/D-10. Returns (success, message).
    pub async fn test_connection(&self, name: &str) -> (bool, String) {
        let plugins = self.plugins.read().await;
        let Some(plugin) = plugins.get(name) else {
            return (false, format!("Plugin '{}' not found", name));
        };
        let readiness_endpoint = plugin
            .manifest
            .connector
            .as_ref()
            .and_then(|connector| connector.readiness_endpoint.clone())
            .or_else(|| {
                plugin.manifest.api.as_ref().and_then(|api| {
                    api.endpoints
                        .contains_key("check_health")
                        .then(|| "check_health".to_string())
                })
            });
        let fallback_probe = plugin.manifest.api.clone();
        drop(plugins);

        if let Some(endpoint_name) = readiness_endpoint {
            return match self
                .api_call(name, &endpoint_name, serde_json::json!({}))
                .await
            {
                Ok(message) => (true, message),
                Err(error) => (false, error.to_string()),
            };
        }

        let Some(api) = fallback_probe else {
            return (false, "Plugin has no API section".to_string());
        };
        let base_url = match &api.base_url {
            Some(url) => url.clone(),
            None => return (false, "Plugin has no base_url".to_string()),
        };
        let test_url = if let Some((_name, endpoint)) = api.endpoints.iter().next() {
            format!("{}{}", base_url.trim_end_matches('/'), endpoint.path)
        } else {
            base_url
        };
        match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
        {
            Ok(client) => match client.head(&test_url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() || status == 401 || status == 403 {
                        (true, "Connection successful".to_string())
                    } else {
                        (false, format!("Server returned {}", status))
                    }
                }
                Err(e) => (false, format!("Connection failed: {}", e)),
            },
            Err(e) => (false, format!("HTTP client error: {}", e)),
        }
    }

    /// Execute a plugin API call through the full proxy flow.
    ///
    /// Orchestrates: plugin lookup -> enabled check -> rate limit -> settings fetch ->
    /// template render -> SSRF check -> HTTP request -> response template -> return text.
    ///
    /// Per D-11/APRX-01/APRX-03.
    pub async fn api_call(
        &self,
        plugin_name: &str,
        endpoint_name: &str,
        params: serde_json::Value,
    ) -> Result<String, api_proxy::PluginApiError> {
        let plugins = self.plugins.read().await;
        let plugin =
            plugins
                .get(plugin_name)
                .ok_or_else(|| api_proxy::PluginApiError::PluginNotFound {
                    name: plugin_name.to_string(),
                })?;

        if !self.check_plugin_enabled(plugin_name).await? {
            return Err(api_proxy::PluginApiError::PluginDisabled {
                name: plugin_name.to_string(),
            });
        }

        let api = plugin.manifest.api.as_ref().ok_or_else(|| {
            api_proxy::PluginApiError::EndpointNotFound {
                plugin: plugin_name.to_string(),
                endpoint: endpoint_name.to_string(),
            }
        })?;
        let endpoint = api.endpoints.get(endpoint_name).ok_or_else(|| {
            api_proxy::PluginApiError::EndpointNotFound {
                plugin: plugin_name.to_string(),
                endpoint: endpoint_name.to_string(),
            }
        })?;

        if let Some(run) = endpoint.run.clone() {
            let plugin_dir = self.plugins_dir.join(&plugin.dir_name);
            let data_dir = local_run::plugin_data_dir(&self.plugins_dir, plugin_name);
            let version = plugin.manifest.version.clone();
            let setup = commands::resolve_python_command(
                &plugin_dir,
                endpoint_name,
                plugin.manifest.python.as_ref(),
                &manifest::PythonCommandDef {
                    command: ":".to_string(),
                    ..run.python.clone()
                },
            )
            .shell;
            drop(plugins);
            if !local_run::local_run_approved(&data_dir, &version) {
                return Err(api_proxy::PluginApiError::LocalRunNotApproved {
                    plugin: plugin_name.to_string(),
                    version,
                });
            }
            let shell = local_run::local_run_shell(&setup, &run.python.command);
            let settings = self
                .persistence
                .get_settings(plugin_name)
                .await
                .unwrap_or_default();
            return local_run::run_local_endpoint(local_run::LocalRunRequest {
                plugin_name,
                plugin_dir: &plugin_dir,
                data_dir: &data_dir,
                shell: &shell,
                input: template::build_context(params, settings, None),
                timeout_secs: local_run::local_run_timeout_secs(&run),
            })
            .await;
        }

        let api_clone = api.clone();
        let endpoint_clone = endpoint.clone();
        let manifest_clone = plugin.manifest.clone();
        let rpm = api
            .rate_limit
            .as_ref()
            .and_then(|rl| rl.requests_per_minute)
            .unwrap_or(rate_limiter::DEFAULT_REQUESTS_PER_MINUTE);

        let plugin_has_oauth = plugin
            .manifest
            .auth
            .as_ref()
            .map(|a| a.auth_type == "oauth2")
            .unwrap_or(false);
        let manifest_auth = plugin.manifest.auth.clone();

        drop(plugins);

        {
            let mut limiters = self.rate_limiters.lock().await;
            if !limiters.check(plugin_name, rpm) {
                return Err(api_proxy::PluginApiError::RateLimited {
                    plugin: plugin_name.to_string(),
                    retry_after_secs: 60,
                });
            }
        }

        let settings = self
            .persistence
            .get_settings(plugin_name)
            .await
            .unwrap_or_default();

        let auth_context = if plugin_has_oauth {
            match self
                .get_oauth_context_with_refresh(plugin_name, &manifest_auth, &settings)
                .await
            {
                Ok(ctx) => ctx,
                Err(_) => {
                    return Err(api_proxy::PluginApiError::AuthExpired {
                        plugin: plugin_name.to_string(),
                    });
                }
            }
        } else {
            None
        };

        let context = template::build_context(params, settings, auth_context);

        let rendered = template::render_request(
            &self.template_registry,
            &api_clone,
            &endpoint_clone,
            &context,
        )
        .await?;

        ssrf::validate_url(&rendered.url, false).await?;

        let response_json = match api_proxy::execute_request(&self.http_client, &rendered).await {
            Ok(json) => json,
            Err(api_proxy::PluginApiError::RateLimited {
                retry_after_secs, ..
            }) => {
                return Err(api_proxy::PluginApiError::RateLimited {
                    plugin: plugin_name.to_string(),
                    retry_after_secs,
                });
            }
            Err(e) => {
                return Err(manager_extras::enrich_plugin_api_error(
                    plugin_name,
                    endpoint_name,
                    &manifest_clone,
                    e,
                ))
            }
        };

        let rendered_text =
            template::render_response(&self.template_registry, &endpoint_clone, &response_json)?;

        Ok(rendered_text)
    }
}
