use crate::plugin::manager_extras::{connector_readiness, enrich_plugin_api_error, to_plugin_info};
use crate::plugin::manifest::{
    ApiSection, CommandDef, ConnectorSection, EndpointDef, PluginManifest, RateLimitDef,
    SettingField,
};
use crate::plugin::{LoadedPlugin, PluginApiError};
use std::collections::HashMap;

fn sample_loaded_plugin() -> LoadedPlugin {
    let mut settings = HashMap::new();
    settings.insert(
        "token".to_string(),
        SettingField {
            field_type: "string".to_string(),
            label: "Token".to_string(),
            required: true,
            secret: true,
            default: None,
            options: None,
            description: Some("API token".to_string()),
        },
    );

    let mut endpoints = HashMap::new();
    endpoints.insert(
        "check_health".to_string(),
        EndpointDef {
            method: "GET".to_string(),
            path: "/health".to_string(),
            run: None,
            params: None,
            headers: None,
            body: None,
            response_template: Some("ok".to_string()),
        },
    );

    let mut commands = HashMap::new();
    commands.insert(
        "comment".to_string(),
        CommandDef {
            description: "Comment".to_string(),
            action: Some("comment_on_work_item".to_string()),
            python: None,
        },
    );

    LoadedPlugin {
        manifest: PluginManifest {
            name: "github".to_string(),
            version: "1.1.0".to_string(),
            schema_version: 1,
            description: Some("GitHub connector".to_string()),
            author: Some("zorai".to_string()),
            license: Some("MIT".to_string()),
            zorai_version: Some(">=2.0.0".to_string()),
            settings: Some(settings),
            api: Some(ApiSection {
                base_url: Some("https://api.github.com".to_string()),
                endpoints,
                rate_limit: Some(RateLimitDef {
                    requests_per_minute: Some(60),
                }),
            }),
            commands: Some(commands),
            skills: Some(vec!["skills/github.md".to_string()]),
            auth: None,
            connector: Some(ConnectorSection {
                kind: "github".to_string(),
                category: Some("repo".to_string()),
                setup_hint: Some("Add a PAT with repo access.".to_string()),
                docs_path: Some("plugins/zorai-plugin-github/README.md".to_string()),
                readiness_endpoint: Some("check_health".to_string()),
                workflow_primitives: vec!["list_work_items".to_string()],
                read_actions: vec!["list_issues".to_string()],
                write_actions: vec!["comment_on_work_item".to_string()],
            }),
            python: None,
            extra: HashMap::new(),
        },
        manifest_json: "{}".to_string(),
        dir_name: "github".to_string(),
    }
}

#[test]
fn connector_readiness_detects_missing_required_settings() {
    let plugin = sample_loaded_plugin();
    let readiness = connector_readiness(&plugin, true, "not_configured", &[]);
    assert_eq!(readiness.state, "needs_setup");
    assert!(readiness
        .message
        .as_deref()
        .unwrap_or_default()
        .contains("token"));
    assert!(readiness.recovery_hint.is_some());
}

#[test]
fn connector_readiness_reports_ready_for_non_oauth_connector_with_settings() {
    let plugin = sample_loaded_plugin();
    let readiness = connector_readiness(
        &plugin,
        true,
        "not_configured",
        &[("token".to_string(), "ghp_test".to_string(), true)],
    );
    assert_eq!(readiness.state, "ready");
}

#[test]
fn to_plugin_info_includes_connector_metadata_and_actions() {
    let plugin = sample_loaded_plugin();
    let info = to_plugin_info(
        &plugin,
        true,
        "local",
        "2026-04-30T00:00:00Z",
        "2026-04-30T00:00:00Z",
        "connected".to_string(),
        &[("token".to_string(), "ghp_test".to_string(), true)],
    );
    assert_eq!(info.connector_kind.as_deref(), Some("github"));
    assert_eq!(info.connector_category.as_deref(), Some("repo"));
    assert_eq!(info.readiness_state, "ready");
    assert_eq!(
        info.workflow_primitives,
        vec!["list_work_items".to_string()]
    );
    assert_eq!(info.read_actions, vec!["list_issues".to_string()]);
    assert_eq!(info.write_actions, vec!["comment_on_work_item".to_string()]);
}

#[test]
fn enrich_plugin_api_error_adds_scope_recovery_hint() {
    let plugin = sample_loaded_plugin();
    let error = enrich_plugin_api_error(
        "github",
        "comment_on_work_item",
        &plugin.manifest,
        PluginApiError::HttpError {
            status: 403,
            body: "insufficient permissions".to_string(),
        },
    );
    match error {
        PluginApiError::HttpError { body, .. } => {
            assert!(body.contains("Recovery:"));
        }
        other => panic!("expected HttpError, got {other:?}"),
    }
}
