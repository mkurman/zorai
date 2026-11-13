#![allow(dead_code)]

//! Plugin command registry: namespaced slash commands declared in plugin manifests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::loader::LoadedPlugin;
use super::manifest::{PythonCommandDef, PythonDefaults, PythonEnvDef};

/// A registered plugin command entry.
#[derive(Debug, Clone)]
pub(crate) struct PluginCommandEntry {
    pub plugin_name: String,
    pub command_key: String,
    pub description: String,
    pub api_endpoint: Option<String>,
    pub python: Option<ResolvedPythonCommand>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedPythonCommand {
    pub command: String,
    pub run_path: PathBuf,
    pub source: Option<String>,
    pub env: Option<PythonEnvDef>,
    pub dependencies: Vec<String>,
    pub shell: String,
}

/// Registry of all plugin slash commands. Rebuilt when plugins change.
pub(crate) struct PluginCommandRegistry {
    commands: HashMap<String, PluginCommandEntry>,
}

impl PluginCommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Clear and repopulate from all loaded plugins.
    /// For each plugin with commands, creates entries with key `/pluginname.commandname` per PSKL-05.
    pub fn rebuild_from_plugins(
        &mut self,
        plugins: &HashMap<String, LoadedPlugin>,
        plugins_dir: &Path,
    ) {
        self.commands.clear();
        for (plugin_name, loaded) in plugins {
            let Some(cmds) = &loaded.manifest.commands else {
                continue;
            };
            let plugin_dir = plugins_dir.join(&loaded.dir_name);
            for (cmd_name, cmd_def) in cmds {
                let key = format!("/{}.{}", plugin_name, cmd_name);
                self.commands.insert(
                    key.clone(),
                    PluginCommandEntry {
                        plugin_name: plugin_name.clone(),
                        command_key: key,
                        description: cmd_def.description.clone(),
                        api_endpoint: cmd_def.action.clone(),
                        python: cmd_def.python.as_ref().map(|python| {
                            resolve_python_command(
                                &plugin_dir,
                                cmd_name,
                                loaded.manifest.python.as_ref(),
                                python,
                            )
                        }),
                    },
                );
            }
        }
    }

    /// Resolve a user input string to a command entry.
    /// Checks if input starts with a registered command key (exact match or followed by whitespace).
    pub fn resolve(&self, input: &str) -> Option<&PluginCommandEntry> {
        for (key, entry) in &self.commands {
            if input == key || input.starts_with(&format!("{} ", key)) {
                return Some(entry);
            }
        }
        None
    }

    /// Return all entries sorted by command_key.
    pub fn list_all(&self) -> Vec<&PluginCommandEntry> {
        let mut entries: Vec<&PluginCommandEntry> = self.commands.values().collect();
        entries.sort_by(|a, b| a.command_key.cmp(&b.command_key));
        entries
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

pub(super) fn resolve_python_command(
    plugin_dir: &Path,
    command_name: &str,
    defaults: Option<&PythonDefaults>,
    command: &PythonCommandDef,
) -> ResolvedPythonCommand {
    let command_source = command
        .source
        .clone()
        .or_else(|| defaults.and_then(|value| value.source.clone()));
    let env = command
        .env
        .clone()
        .or_else(|| defaults.and_then(|value| value.env.clone()));

    let mut dependencies = defaults
        .map(|value| value.dependencies.clone())
        .unwrap_or_default();
    for dependency in &command.dependencies {
        if !dependencies.contains(dependency) {
            dependencies.push(dependency.clone());
        }
    }

    let run_path = resolve_run_path(
        plugin_dir,
        command_name,
        command_source.as_deref(),
        command
            .run_path
            .as_deref()
            .or_else(|| defaults.and_then(|value| value.run_path.as_deref())),
    );

    let shell = build_python_shell(
        plugin_dir,
        command_name,
        &run_path,
        command_source.as_deref(),
        env.as_ref(),
        &dependencies,
        &command.command,
    );

    ResolvedPythonCommand {
        command: command.command.clone(),
        run_path,
        source: command_source,
        env,
        dependencies,
        shell,
    }
}

fn resolve_run_path(
    plugin_dir: &Path,
    command_name: &str,
    source: Option<&str>,
    run_path: Option<&str>,
) -> PathBuf {
    let source_root = source_root(plugin_dir, command_name, source);
    match run_path {
        Some(path) => {
            let candidate = PathBuf::from(path);
            if candidate.is_absolute() {
                candidate
            } else {
                source_root
                    .unwrap_or_else(|| plugin_dir.to_path_buf())
                    .join(candidate)
            }
        }
        None => source_root.unwrap_or_else(|| plugin_dir.to_path_buf()),
    }
}

fn source_root(plugin_dir: &Path, command_name: &str, source: Option<&str>) -> Option<PathBuf> {
    let source = source?;
    if looks_like_url(source) {
        Some(plugin_dir.join(".python").join(command_name).join("source"))
    } else {
        let path = PathBuf::from(source);
        Some(if path.is_dir() {
            path
        } else {
            path.parent().map(Path::to_path_buf).unwrap_or(path)
        })
    }
}

fn build_python_shell(
    plugin_dir: &Path,
    command_name: &str,
    run_path: &Path,
    source: Option<&str>,
    env: Option<&PythonEnvDef>,
    dependencies: &[String],
    command: &str,
) -> String {
    let mut lines = vec!["set -euo pipefail".to_string()];

    if let Some(source) = source {
        if looks_like_url(source) {
            let download_dir = plugin_dir.join(".python").join(command_name).join("source");
            let download_file = download_dir.join(infer_download_name(source));
            lines.push(format!(
                "mkdir -p {}",
                shell_quote(&download_dir.display().to_string())
            ));
            lines.push(format!(
                "if [ ! -f {0} ]; then curl -fsSL {1} -o {0}; fi",
                shell_quote(&download_file.display().to_string()),
                shell_quote(source)
            ));
        } else {
            lines.push(format!("test -e {}", shell_quote(source)));
        }
    }

    match env {
        Some(PythonEnvDef::Path(path)) => {
            lines.push(format!("source {}", shell_quote(path)));
            lines.push(format!(
                "mkdir -p {}",
                shell_quote(&run_path.display().to_string())
            ));
            lines.push(format!(
                "cd {}",
                shell_quote(&run_path.display().to_string())
            ));
            if !dependencies.is_empty() {
                lines.push(format!(
                    "python -m pip install {}",
                    dependencies
                        .iter()
                        .map(|value| shell_quote(value))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
        Some(PythonEnvDef::Managed(true)) => {
            lines.push(format!(
                "mkdir -p {}",
                shell_quote(&run_path.display().to_string())
            ));
            lines.push(format!(
                "cd {}",
                shell_quote(&run_path.display().to_string())
            ));
            lines.push("if [ ! -d .venv ]; then".to_string());
            lines.push("  if command -v uv >/dev/null 2>&1; then uv venv .venv; else python3 -m venv .venv; fi".to_string());
            lines.push("fi".to_string());
            lines.push("source .venv/bin/activate".to_string());
            if !dependencies.is_empty() {
                let joined = dependencies
                    .iter()
                    .map(|value| shell_quote(value))
                    .collect::<Vec<_>>()
                    .join(" ");
                lines.push(format!(
                    "if command -v uv >/dev/null 2>&1; then uv pip install {joined}; else python -m pip install {joined}; fi"
                ));
            }
        }
        _ => {
            lines.push(format!(
                "mkdir -p {}",
                shell_quote(&run_path.display().to_string())
            ));
            lines.push(format!(
                "cd {}",
                shell_quote(&run_path.display().to_string())
            ));
            if !dependencies.is_empty() {
                lines.push(format!(
                    "python -m pip install {}",
                    dependencies
                        .iter()
                        .map(|value| shell_quote(value))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
    }

    lines.push(command.to_string());
    lines.join("\n")
}

fn infer_download_name(source: &str) -> String {
    let candidate = source
        .split('/')
        .next_back()
        .unwrap_or("source_artifact")
        .split('?')
        .next()
        .unwrap_or("source_artifact")
        .trim();
    if candidate.is_empty() {
        "source_artifact".to_string()
    } else {
        candidate.to_string()
    }
}

fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manifest::{
        CommandDef, PluginManifest, PythonCommandDef, PythonDefaults, PythonEnvDef,
    };

    fn make_plugin_with_commands(
        name: &str,
        commands: Vec<(&str, &str, Option<&str>)>,
    ) -> LoadedPlugin {
        let mut cmd_map = HashMap::new();
        for (cmd_name, desc, action) in commands {
            cmd_map.insert(
                cmd_name.to_string(),
                CommandDef {
                    description: desc.to_string(),
                    action: action.map(|a| a.to_string()),
                    python: None,
                },
            );
        }
        LoadedPlugin {
            manifest: PluginManifest {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                schema_version: 1,
                description: None,
                author: None,
                license: None,
                zorai_version: None,
                settings: None,
                api: None,
                commands: Some(cmd_map),
                skills: None,
                auth: None,
                connector: None,
                python: None,
                extra: HashMap::new(),
            },
            manifest_json: String::new(),
            dir_name: name.to_string(),
        }
    }

    #[test]
    fn rebuild_populates_namespaced_commands() {
        let mut registry = PluginCommandRegistry::new();
        let mut plugins = HashMap::new();
        plugins.insert(
            "gmail-calendar".to_string(),
            make_plugin_with_commands(
                "gmail-calendar",
                vec![
                    ("inbox", "Show inbox", Some("list_messages")),
                    ("send", "Send email", Some("send_message")),
                ],
            ),
        );

        registry.rebuild_from_plugins(&plugins, Path::new("/tmp/plugins"));

        assert!(!registry.is_empty());
        let all = registry.list_all();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|e| e.command_key == "/gmail-calendar.inbox"));
        assert!(all.iter().any(|e| e.command_key == "/gmail-calendar.send"));
    }

    #[test]
    fn resolve_finds_registered_command() {
        let mut registry = PluginCommandRegistry::new();
        let mut plugins = HashMap::new();
        plugins.insert(
            "gmail-calendar".to_string(),
            make_plugin_with_commands(
                "gmail-calendar",
                vec![("inbox", "Show inbox", Some("list_messages"))],
            ),
        );
        registry.rebuild_from_plugins(&plugins, Path::new("/tmp/plugins"));

        let entry = registry.resolve("/gmail-calendar.inbox").unwrap();
        assert_eq!(entry.plugin_name, "gmail-calendar");
        assert_eq!(entry.api_endpoint.as_deref(), Some("list_messages"));
    }

    #[test]
    fn resolve_returns_none_for_unregistered() {
        let mut registry = PluginCommandRegistry::new();
        let plugins: HashMap<String, LoadedPlugin> = HashMap::new();
        registry.rebuild_from_plugins(&plugins, Path::new("/tmp/plugins"));

        assert!(registry.resolve("/unknown.command").is_none());
    }

    #[test]
    fn list_all_returns_sorted_entries() {
        let mut registry = PluginCommandRegistry::new();
        let mut plugins = HashMap::new();
        plugins.insert(
            "weather".to_string(),
            make_plugin_with_commands("weather", vec![("forecast", "Get forecast", None)]),
        );
        plugins.insert(
            "gmail".to_string(),
            make_plugin_with_commands("gmail", vec![("inbox", "Show inbox", Some("list"))]),
        );
        registry.rebuild_from_plugins(&plugins, Path::new("/tmp/plugins"));

        let all = registry.list_all();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].command_key, "/gmail.inbox");
        assert_eq!(all[1].command_key, "/weather.forecast");
    }

    #[test]
    fn rebuild_resolves_python_commands_with_top_level_defaults() {
        let mut registry = PluginCommandRegistry::new();
        let mut plugins = HashMap::new();

        let mut commands = HashMap::new();
        commands.insert(
            "sync".to_string(),
            CommandDef {
                description: "Run sync".to_string(),
                action: None,
                python: Some(PythonCommandDef {
                    command: "python sync.py --full".to_string(),
                    run_path: None,
                    source: None,
                    env: None,
                    dependencies: vec!["rich".to_string()],
                }),
            },
        );

        plugins.insert(
            "python-tools".to_string(),
            LoadedPlugin {
                manifest: PluginManifest {
                    name: "python-tools".to_string(),
                    version: "1.0.0".to_string(),
                    schema_version: 1,
                    description: None,
                    author: None,
                    license: None,
                    zorai_version: None,
                    settings: None,
                    api: None,
                    commands: Some(commands),
                    skills: None,
                    auth: None,
                    connector: None,
                    python: Some(PythonDefaults {
                        run_path: Some("workspace".to_string()),
                        source: Some("https://example.com/tool.py".to_string()),
                        env: Some(PythonEnvDef::Managed(true)),
                        dependencies: vec!["requests>=2.32".to_string()],
                    }),
                    extra: HashMap::new(),
                },
                manifest_json: String::new(),
                dir_name: "python-tools".to_string(),
            },
        );

        registry.rebuild_from_plugins(&plugins, Path::new("/tmp/plugins"));

        let entry = registry
            .resolve("/python-tools.sync")
            .expect("python command");
        let plan = entry.python.as_ref().expect("python plan");
        assert_eq!(plan.command, "python sync.py --full");
        assert_eq!(
            plan.run_path.display().to_string(),
            "/tmp/plugins/python-tools/.python/sync/source/workspace"
        );
        assert_eq!(plan.source.as_deref(), Some("https://example.com/tool.py"));
        assert_eq!(plan.dependencies, vec!["requests>=2.32", "rich"]);
        assert!(plan.shell.contains("uv venv .venv"));
        assert!(plan.shell.contains("python sync.py --full"));
    }

    #[test]
    fn rebuild_resolves_python_commands_with_explicit_env_path() {
        let mut registry = PluginCommandRegistry::new();
        let mut plugins = HashMap::new();

        let mut commands = HashMap::new();
        commands.insert(
            "lint".to_string(),
            CommandDef {
                description: "Run lint".to_string(),
                action: None,
                python: Some(PythonCommandDef {
                    command: "python -m ruff check .".to_string(),
                    run_path: Some("/workspace/project".to_string()),
                    source: Some("/opt/plugin-src".to_string()),
                    env: Some(PythonEnvDef::Path(
                        "/opt/venvs/lint/bin/activate".to_string(),
                    )),
                    dependencies: vec!["ruff".to_string()],
                }),
            },
        );

        plugins.insert(
            "python-tools".to_string(),
            LoadedPlugin {
                manifest: PluginManifest {
                    name: "python-tools".to_string(),
                    version: "1.0.0".to_string(),
                    schema_version: 1,
                    description: None,
                    author: None,
                    license: None,
                    zorai_version: None,
                    settings: None,
                    api: None,
                    commands: Some(commands),
                    skills: None,
                    auth: None,
                    connector: None,
                    python: None,
                    extra: HashMap::new(),
                },
                manifest_json: String::new(),
                dir_name: "python-tools".to_string(),
            },
        );

        registry.rebuild_from_plugins(&plugins, Path::new("/tmp/plugins"));

        let entry = registry
            .resolve("/python-tools.lint")
            .expect("python command");
        let plan = entry.python.as_ref().expect("python plan");
        assert!(plan.shell.contains("source '/opt/venvs/lint/bin/activate'"));
        assert!(plan.shell.contains("python -m pip install 'ruff'"));
        assert!(plan.shell.contains("cd '/workspace/project'"));
    }
}
