use crate::history::db;
use anyhow::Result;
use std::sync::Arc;

/// Internal plugin record (not exposed to protocol crate).
#[derive(Debug, Clone)]
pub struct PluginRecord {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub manifest_json: String,
    pub install_source: String,
    pub enabled: bool,
    pub installed_at: String,
    pub updated_at: String,
}

fn row_to_plugin_record(row: &db::Row) -> Result<PluginRecord> {
    Ok(PluginRecord {
        name: row.get(0)?,
        version: row.get(1)?,
        description: row.get(2)?,
        author: row.get(3)?,
        manifest_json: row.get(4)?,
        install_source: row.get(5)?,
        enabled: row.get::<i64>(6)? != 0,
        installed_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

/// SQLite persistence layer for plugin metadata.
pub struct PluginPersistence {
    history: Arc<crate::history::HistoryStore>,
}

impl PluginPersistence {
    pub fn new(history: Arc<crate::history::HistoryStore>) -> Self {
        Self { history }
    }

    /// List all plugins. Per PLUG-09.
    pub async fn list_plugins(&self) -> Result<Vec<PluginRecord>> {
        let rows = self
            .history
            .read_db
            .query(
                "SELECT name, version, description, author, manifest_json, install_source, enabled, installed_at, updated_at FROM plugins WHERE deleted_at IS NULL ORDER BY name ASC",
                db::Params::None,
            )
            .await?;
        rows.iter().map(row_to_plugin_record).collect()
    }

    /// Get a single plugin by name. Per PLUG-09.
    pub async fn get_plugin(&self, name: &str) -> Result<Option<PluginRecord>> {
        let name = name.to_string();
        self.history
            .read_db
            .query_opt(
                "SELECT name, version, description, author, manifest_json, install_source, enabled, installed_at, updated_at FROM plugins WHERE name = ?1 AND deleted_at IS NULL",
                db::db_params![name],
            )
            .await?
            .map(|row| row_to_plugin_record(&row))
            .transpose()
    }

    /// Insert or update a plugin record. Per PLUG-09.
    pub async fn upsert_plugin(&self, record: &PluginRecord) -> Result<()> {
        let record = record.clone();
        self.history
            .conn_db
            .execute(
                "INSERT INTO plugins (name, version, description, author, manifest_json, install_source, enabled, installed_at, updated_at, deleted_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL) \
                 ON CONFLICT(name) DO UPDATE SET \
                   version = excluded.version, \
                   description = excluded.description, \
                   author = excluded.author, \
                   manifest_json = excluded.manifest_json, \
                   install_source = CASE WHEN excluded.install_source = 'local' THEN plugins.install_source ELSE excluded.install_source END, \
                   updated_at = excluded.updated_at, \
                   deleted_at = NULL",
                db::db_params![
                    record.name,
                    record.version,
                    record.description,
                    record.author,
                    record.manifest_json,
                    record.install_source,
                    record.enabled as i64,
                    record.installed_at,
                    record.updated_at,
                ],
            )
            .await?;
        Ok(())
    }

    /// Set enabled/disabled status. Per PLUG-09.
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        let name = name.to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let rows = self
            .history
            .conn_db
            .execute(
                "UPDATE plugins SET enabled = ?1, updated_at = ?2 WHERE name = ?3 AND deleted_at IS NULL",
                db::db_params![enabled as i64, now, name.clone()],
            )
            .await?;
        if rows == 0 {
            return Err(anyhow::anyhow!("plugin not found: {name}"));
        }
        Ok(())
    }

    /// Remove a single plugin by name (and its settings/credentials). Per INST-04/D-06.
    pub async fn remove_plugin(&self, name: &str) -> Result<bool> {
        let name = name.to_string();
        let deleted_at = crate::history::now_ts() as i64;
        self.history
            .conn_db
            .execute(
                "UPDATE plugin_settings SET deleted_at = ?2 WHERE plugin_name = ?1 AND deleted_at IS NULL",
                db::db_params![name.clone(), deleted_at],
            )
            .await?;
        self.history
            .conn_db
            .execute(
                "UPDATE plugin_credentials SET deleted_at = ?2 WHERE plugin_name = ?1 AND deleted_at IS NULL",
                db::db_params![name.clone(), deleted_at],
            )
            .await?;
        let rows = self
            .history
            .conn_db
            .execute(
                "UPDATE plugins SET deleted_at = ?2 WHERE name = ?1 AND deleted_at IS NULL",
                db::db_params![name, deleted_at],
            )
            .await?;
        Ok(rows > 0)
    }

    /// Get all settings for a plugin. Returns (key, value, is_secret) tuples.
    /// Secret values are base64-decoded before returning. Per PSET-06.
    pub async fn get_settings(&self, plugin_name: &str) -> Result<Vec<(String, String, bool)>> {
        let plugin_name = plugin_name.to_string();
        let rows = self
            .history
            .read_db
            .query(
                "SELECT key, value, is_secret FROM plugin_settings WHERE plugin_name = ?1 AND deleted_at IS NULL ORDER BY key ASC",
                db::db_params![plugin_name],
            )
            .await?;
        let mut settings = Vec::new();
        for row in &rows {
            let key = row.get::<String>(0)?;
            let raw_value = row.get::<String>(1)?;
            let is_secret = row.get::<i64>(2)? != 0;
            let value = if is_secret {
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(&raw_value) {
                    Ok(bytes) => String::from_utf8(bytes).unwrap_or(raw_value),
                    Err(_) => raw_value,
                }
            } else {
                raw_value
            };
            settings.push((key, value, is_secret));
        }
        Ok(settings)
    }

    /// Upsert a single plugin setting. Per PSET-06/D-06.
    /// Secret values are base64-encoded before storage (placeholder for Phase 18 AES encryption).
    pub async fn upsert_setting(
        &self,
        plugin_name: &str,
        key: &str,
        value: &str,
        is_secret: bool,
    ) -> Result<()> {
        let plugin_name = plugin_name.to_string();
        let key = key.to_string();
        let stored_value = if is_secret {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
        } else {
            value.to_string()
        };
        let is_secret_i64 = is_secret as i64;
        self.history
            .conn_db
            .execute(
                "INSERT OR REPLACE INTO plugin_settings (plugin_name, key, value, is_secret, deleted_at) VALUES (?1, ?2, ?3, ?4, NULL)",
                db::db_params![plugin_name, key, stored_value, is_secret_i64],
            )
            .await?;
        Ok(())
    }

    /// Remove plugins not in the provided set (stale record reconciliation per Pitfall 6).
    /// Returns the number of rows deleted.
    pub async fn remove_stale_plugins(&self, active_names: &[String]) -> Result<u64> {
        let active_names = active_names.to_vec();
        if active_names.is_empty() {
            return self
                .history
                .conn_db
                .execute(
                    "UPDATE plugins SET deleted_at = ?1 WHERE deleted_at IS NULL",
                    db::db_params![crate::history::now_ts() as i64],
                )
                .await;
        }
        let placeholders: Vec<&str> = active_names.iter().map(|_| "?").collect();
        let sql = format!(
            "UPDATE plugins SET deleted_at = ? WHERE deleted_at IS NULL AND name NOT IN ({})",
            placeholders.join(",")
        );
        let deleted_at = crate::history::now_ts() as i64;
        let mut values: Vec<db::Value> = vec![db::Value::Integer(deleted_at)];
        values.extend(active_names.into_iter().map(db::Value::Text));
        self.history
            .conn_db
            .execute(&sql, db::Params::Positional(values))
            .await
    }
}

impl PluginPersistence {
    /// Get a credential from the plugin_credentials table. Returns the raw
    /// encrypted blob and optional expiry timestamp. Decryption is the caller's
    /// responsibility (uses crypto module).
    pub async fn get_credential(
        &self,
        plugin_name: &str,
        credential_type: &str,
    ) -> Result<Option<(Vec<u8>, Option<String>)>> {
        let plugin_name = plugin_name.to_string();
        let credential_type = credential_type.to_string();
        self.history
            .read_db
            .query_opt(
                "SELECT encrypted_value, expires_at FROM plugin_credentials WHERE plugin_name = ?1 AND credential_type = ?2 AND deleted_at IS NULL",
                db::db_params![plugin_name, credential_type],
            )
            .await?
            .map(|row| {
                Ok::<_, anyhow::Error>((row.get::<Vec<u8>>(0)?, row.get::<Option<String>>(1)?))
            })
            .transpose()
    }

    /// Insert or update a credential in the plugin_credentials table.
    /// The value should already be encrypted (via crypto::encrypt).
    pub async fn upsert_credential(
        &self,
        plugin_name: &str,
        credential_type: &str,
        encrypted_value: &[u8],
        expires_at: Option<&str>,
    ) -> Result<()> {
        let plugin_name = plugin_name.to_string();
        let credential_type = credential_type.to_string();
        let encrypted_value = encrypted_value.to_vec();
        let expires_at = expires_at.map(|s| s.to_string());
        let now = chrono::Utc::now().to_rfc3339();
        self.history
            .conn_db
            .execute(
                "INSERT INTO plugin_credentials (plugin_name, credential_type, encrypted_value, expires_at, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, NULL)
                 ON CONFLICT(plugin_name, credential_type) DO UPDATE SET
                     encrypted_value = excluded.encrypted_value,
                     expires_at = excluded.expires_at,
                     updated_at = excluded.updated_at,
                     deleted_at = NULL",
                db::db_params![plugin_name, credential_type, encrypted_value, expires_at, now],
            )
            .await?;
        Ok(())
    }

    /// Compute auth status for a plugin based on credential state.
    /// Returns "connected", "refreshable", "needs_reconnect", or "not_configured".
    /// No decryption needed -- just checks row existence and expiry.
    pub async fn get_auth_status(&self, plugin_name: &str) -> Result<String> {
        let plugin_name = plugin_name.to_string();
        let access_row: Option<Option<String>> = self
            .history
            .read_db
            .query_opt(
                "SELECT expires_at FROM plugin_credentials WHERE plugin_name = ?1 AND credential_type = 'access_token' AND deleted_at IS NULL",
                db::db_params![plugin_name.clone()],
            )
            .await?
            .map(|row| row.get::<Option<String>>(0))
            .transpose()?;
        let has_refresh_token = self
            .history
            .read_db
            .query_opt(
                "SELECT 1 FROM plugin_credentials WHERE plugin_name = ?1 AND credential_type = 'refresh_token' AND deleted_at IS NULL LIMIT 1",
                db::db_params![plugin_name],
            )
            .await?
            .is_some();

        let status = match access_row {
            Some(expires_at) => super::manager_extras::auth_status_from_expiry_and_refresh_token(
                expires_at.as_deref(),
                has_refresh_token,
            ),
            None => super::manager_extras::PluginAuthStatus::NotConfigured,
        };
        Ok(status.as_str().to_string())
    }
}

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
