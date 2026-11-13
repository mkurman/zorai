use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin manifest (plugin.json) -- v1 schema. Per PLUG-01/D-01/D-02/D-03.
/// Only `name`, `version`, `schema_version` are required (D-02).
/// Unknown fields captured in `extra` (D-01 permissive validation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub schema_version: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zorai_version: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<HashMap<String, SettingField>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<ApiSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<HashMap<String, CommandDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector: Option<ConnectorSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<PythonDefaults>,

    /// D-01: Capture unknown fields silently for forward compatibility.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A single setting field declared by a plugin. Per PLUG-04.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingField {
    /// Field type: "string", "number", "boolean", "select".
    #[serde(rename = "type")]
    pub field_type: String,
    pub label: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// API section declaring base_url, endpoints, and rate limits. Per PLUG-05.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub endpoints: HashMap<String, EndpointDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitDef>,
}

/// A single API endpoint definition. Per PLUG-05.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointDef {
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<EndpointRunDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointRunDef {
    #[serde(flatten)]
    pub python: PythonCommandDef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

/// Rate limit configuration for a plugin's API. Per PLUG-05.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests_per_minute: Option<u32>,
}

/// A slash command declared by a plugin. Per PLUG-06.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDef {
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<PythonCommandDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PythonEnvDef {
    Path(String),
    Managed(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PythonDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<PythonEnvDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PythonCommandDef {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<PythonEnvDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

/// Optional connector metadata for productized daily-work integrations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectorSection {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflow_primitives: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_actions: Vec<String>,
}

/// Auth section for plugins requiring OAuth2, API key, or bearer auth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSection {
    #[serde(rename = "type")]
    pub auth_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    pub pkce: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_manifest_deserializes() {
        let json = r#"{"name":"test","version":"1.0.0","schema_version":1}"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "test");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.schema_version, 1);
        assert!(manifest.description.is_none());
        assert!(manifest.settings.is_none());
        assert!(manifest.api.is_none());
        assert!(manifest.commands.is_none());
        assert!(manifest.skills.is_none());
        assert!(manifest.auth.is_none());
        assert!(manifest.connector.is_none());
        assert!(manifest.python.is_none());
        assert!(manifest.extra.is_empty());
    }

    #[test]
    fn full_manifest_deserializes() {
        let json = r#"{
            "name": "gmail",
            "version": "1.0.0",
            "schema_version": 1,
            "description": "Gmail integration",
            "author": "Test Author",
            "license": "MIT",
            "zorai_version": ">=2.0.0",
            "settings": {
                "max_results": {
                    "type": "number",
                    "label": "Max Results",
                    "required": false,
                    "secret": false,
                    "default": 10,
                    "description": "Maximum number of results"
                }
            },
            "api": {
                "base_url": "https://gmail.googleapis.com",
                "endpoints": {
                    "list_messages": {
                        "method": "GET",
                        "path": "/gmail/v1/users/me/messages"
                    }
                },
                "rate_limit": {
                    "requests_per_minute": 60
                }
            },
            "commands": {
                "/gmail-inbox": {
                    "description": "Show recent inbox messages",
                    "action": "list_messages"
                }
            },
            "skills": ["gmail-search", "gmail-send"],
            "auth": {
                "type": "oauth2",
                "authorization_url": "https://accounts.google.com/o/oauth2/auth",
                "token_url": "https://oauth2.googleapis.com/token",
                "scopes": ["https://www.googleapis.com/auth/gmail.readonly"],
                "pkce": true
            }
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "gmail");
        assert_eq!(manifest.description.as_deref(), Some("Gmail integration"));
        assert_eq!(manifest.author.as_deref(), Some("Test Author"));
        assert_eq!(manifest.license.as_deref(), Some("MIT"));
        assert_eq!(manifest.zorai_version.as_deref(), Some(">=2.0.0"));

        let settings = manifest.settings.as_ref().unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings["max_results"].field_type, "number");

        let api = manifest.api.as_ref().unwrap();
        assert_eq!(
            api.base_url.as_deref(),
            Some("https://gmail.googleapis.com")
        );
        assert_eq!(api.endpoints.len(), 1);
        assert_eq!(
            api.rate_limit.as_ref().unwrap().requests_per_minute,
            Some(60)
        );

        let commands = manifest.commands.as_ref().unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands["/gmail-inbox"].description,
            "Show recent inbox messages"
        );

        let skills = manifest.skills.as_ref().unwrap();
        assert_eq!(skills, &["gmail-search", "gmail-send"]);

        let auth = manifest.auth.as_ref().unwrap();
        assert_eq!(auth.auth_type, "oauth2");
        assert!(auth.pkce);
        assert_eq!(auth.scopes.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn unknown_fields_captured_in_extra() {
        let json = r#"{
            "name": "test",
            "version": "1.0.0",
            "schema_version": 1,
            "custom_field": "hello",
            "another_unknown": 42
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.extra.len(), 2);
        assert_eq!(manifest.extra["custom_field"], "hello");
        assert_eq!(manifest.extra["another_unknown"], 42);
    }

    #[test]
    fn setting_field_with_select_type() {
        let json = r#"{
            "type": "select",
            "label": "Format",
            "options": ["json", "csv", "xml"]
        }"#;
        let field: SettingField = serde_json::from_str(json).unwrap();
        assert_eq!(field.field_type, "select");
        assert_eq!(field.options.as_ref().unwrap(), &["json", "csv", "xml"]);
    }

    #[test]
    fn endpoint_def_deserializes() {
        let json = r#"{
            "method": "GET",
            "path": "/messages"
        }"#;
        let endpoint: EndpointDef = serde_json::from_str(json).unwrap();
        assert_eq!(endpoint.method, "GET");
        assert_eq!(endpoint.path, "/messages");
        assert!(endpoint.params.is_none());
        assert!(endpoint.headers.is_none());
    }

    #[test]
    fn auth_section_oauth2_deserializes() {
        let json = r#"{
            "type": "oauth2",
            "authorization_url": "https://example.com/auth",
            "token_url": "https://example.com/token",
            "scopes": ["read", "write"],
            "pkce": true
        }"#;
        let auth: AuthSection = serde_json::from_str(json).unwrap();
        assert_eq!(auth.auth_type, "oauth2");
        assert_eq!(
            auth.authorization_url.as_deref(),
            Some("https://example.com/auth")
        );
        assert_eq!(auth.token_url.as_deref(), Some("https://example.com/token"));
        assert_eq!(auth.scopes.as_ref().unwrap(), &["read", "write"]);
        assert!(auth.pkce);
    }

    #[test]
    fn python_command_and_defaults_deserialize() {
        let json = r#"{
            "name": "python-plugin",
            "version": "1.0.0",
            "schema_version": 1,
            "python": {
                "run_path": "workspace",
                "source": "https://example.com/tool.py",
                "env": true,
                "dependencies": ["requests>=2.32", "pydantic"]
            },
            "commands": {
                "train": {
                    "description": "Run trainer",
                    "python": {
                        "command": "python train.py --epochs 3",
                        "run_path": "jobs/train"
                    }
                }
            }
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let defaults = manifest.python.as_ref().expect("python defaults");
        assert_eq!(defaults.run_path.as_deref(), Some("workspace"));
        assert_eq!(
            defaults.source.as_deref(),
            Some("https://example.com/tool.py")
        );
        assert_eq!(defaults.env, Some(PythonEnvDef::Managed(true)));
        assert_eq!(defaults.dependencies, vec!["requests>=2.32", "pydantic"]);

        let command = manifest
            .commands
            .as_ref()
            .and_then(|commands| commands.get("train"))
            .and_then(|entry| entry.python.as_ref())
            .expect("python command");
        assert_eq!(command.command, "python train.py --epochs 3");
        assert_eq!(command.run_path.as_deref(), Some("jobs/train"));
        assert!(command.source.is_none());
    }

    #[test]
    fn python_env_path_deserializes() {
        let json = r#"{
            "command": "python main.py",
            "env": "/opt/venvs/app/bin/activate"
        }"#;
        let command: PythonCommandDef = serde_json::from_str(json).unwrap();
        assert_eq!(
            command.env,
            Some(PythonEnvDef::Path(
                "/opt/venvs/app/bin/activate".to_string()
            ))
        );
    }
}
