use super::*;
use zorai_shared::providers::{PROVIDER_ID_ANTHROPIC, PROVIDER_ID_OPENAI};

#[test]
fn agent_config_serializes_honcho_fields_in_snake_case() {
    let config = AgentConfig {
        enable_honcho_memory: true,
        honcho_api_key: "key".to_string(),
        honcho_base_url: "https://honcho.example".to_string(),
        honcho_workspace_id: "workspace".to_string(),
        ..AgentConfig::default()
    };

    let json = serde_json::to_value(config).unwrap();
    assert_eq!(json["enable_honcho_memory"], true);
    assert_eq!(json["honcho_api_key"], "key");
    assert_eq!(json["honcho_base_url"], "https://honcho.example");
    assert_eq!(json["honcho_workspace_id"], "workspace");
}

#[test]
fn sub_agent_definition_roundtrip_preserves_builtin_metadata_and_reasoning_effort() {
    let definition = SubAgentDefinition {
        generation_settings: None,
        base_url: None,
        claude_permission_mode: None,
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("governance".to_string()),
        system_prompt: Some("Protect tool execution.".to_string()),
        tool_whitelist: Some(vec!["python_execute".to_string()]),
        tool_blacklist: Some(vec!["bash".to_string()]),
        context_budget_tokens: Some(8192),
        context_window_tokens: None,
        max_duration_secs: Some(30),
        supervisor_config: None,
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Daemon-owned WELES registry entry".to_string()),
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 1_712_000_000,
    };

    let value = serde_json::to_value(&definition).unwrap();
    assert_eq!(value["builtin"], true);
    assert_eq!(value["immutable_identity"], true);
    assert_eq!(value["disable_allowed"], false);
    assert_eq!(value["delete_allowed"], false);
    assert_eq!(
        value["protected_reason"],
        "Daemon-owned WELES registry entry"
    );
    assert_eq!(value["reasoning_effort"], "medium");

    let roundtrip: SubAgentDefinition = serde_json::from_value(value).unwrap();
    assert!(roundtrip.builtin);
    assert!(roundtrip.immutable_identity);
    assert!(!roundtrip.disable_allowed);
    assert!(!roundtrip.delete_allowed);
    assert_eq!(
        roundtrip.protected_reason.as_deref(),
        Some("Daemon-owned WELES registry entry")
    );
    assert_eq!(roundtrip.reasoning_effort.as_deref(), Some("medium"));
}

#[test]
fn legacy_sub_agent_definition_defaults_allow_disable_and_delete() {
    let legacy = serde_json::json!({
        "id": "legacy_subagent",
        "name": "Legacy Subagent",
        "provider": PROVIDER_ID_OPENAI,
        "model": "gpt-5.4-mini",
        "enabled": true,
        "created_at": 1_712_000_001u64
    });

    let definition: SubAgentDefinition = serde_json::from_value(legacy).unwrap();

    assert!(definition.disable_allowed);
    assert!(definition.delete_allowed);
}

#[tokio::test]
async fn list_sub_agents_always_includes_weles_builtin_with_main_defaults() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.system_prompt = "Main agent prompt".to_string();
    engine.set_config(config).await;

    let sub_agents = engine.list_sub_agents().await;
    let weles = sub_agents
        .iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("WELES builtin should always be present");

    assert_eq!(weles.name, "WELES");
    assert_eq!(weles.provider, PROVIDER_ID_OPENAI);
    assert_eq!(weles.model, "gpt-5.4-mini");
    assert_eq!(weles.system_prompt.as_deref(), Some("Main agent prompt"));
    assert_eq!(weles.reasoning_effort.as_deref(), Some("medium"));
    assert_eq!(weles.tool_whitelist, None);
    assert_eq!(
        weles.tool_blacklist.as_deref(),
        Some(
            &[
                "list_terminals".to_string(),
                "read_active_terminal_content".to_string(),
                "run_terminal_command".to_string(),
                "allocate_terminal".to_string(),
                "type_in_terminal".to_string(),
            ][..]
        )
    );
    assert!(weles.enabled);
    assert!(weles.builtin);
    assert!(weles.immutable_identity);
    assert!(!weles.disable_allowed);
    assert!(!weles.delete_allowed);
}

#[tokio::test]
async fn remove_sub_agent_rejects_weles_builtin_as_protected_mutation() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let error = engine
        .remove_sub_agent("weles_builtin")
        .await
        .expect_err("builtin removal should be rejected");

    assert!(
        error.to_string().contains("protected mutation"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn set_sub_agent_rejects_disabling_weles_builtin() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut weles = engine
        .list_sub_agents()
        .await
        .into_iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("missing builtin weles entry");
    weles.enabled = false;

    let error = engine
        .set_sub_agent(weles)
        .await
        .expect_err("builtin disable should be rejected");

    assert!(
        error.to_string().contains("protected mutation"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn set_sub_agent_rejects_reserved_weles_collisions_from_user_entries() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let reserved_id_error = engine
        .set_sub_agent(test_user_sub_agent("weles_builtin", "User Agent"))
        .await
        .expect_err("reserved id should be rejected");
    assert!(
        reserved_id_error
            .to_string()
            .contains("reserved built-in sub-agent"),
        "unexpected error: {reserved_id_error}"
    );

    let reserved_name_error = engine
        .set_sub_agent(test_user_sub_agent("user_agent", "WELES"))
        .await
        .expect_err("reserved name should be rejected");
    assert!(
        reserved_name_error
            .to_string()
            .contains("reserved built-in sub-agent"),
        "unexpected error: {reserved_name_error}"
    );
}

#[tokio::test]
async fn set_sub_agent_rejects_weles_immutable_field_changes() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut weles = engine
        .list_sub_agents()
        .await
        .into_iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("missing builtin weles entry");
    weles.name = "Operator WELES".to_string();
    weles.builtin = false;
    weles.immutable_identity = false;
    weles.disable_allowed = true;
    weles.delete_allowed = true;

    let error = engine
        .set_sub_agent(weles)
        .await
        .expect_err("immutable builtin metadata should be rejected");

    assert!(
        error.to_string().contains("protected mutation"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn set_sub_agent_rejects_weles_protection_flag_changes_as_protected_mutation() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut weles = engine
        .list_sub_agents()
        .await
        .into_iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("missing builtin weles entry");
    weles.builtin = false;
    weles.immutable_identity = false;
    weles.disable_allowed = true;
    weles.delete_allowed = true;

    let error = engine
        .set_sub_agent(weles)
        .await
        .expect_err("protection flag changes should be rejected");

    assert!(
        error.to_string().contains("protected mutation"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn set_sub_agent_preserves_weles_inheritance_when_editing_unrelated_fields() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.system_prompt = "Main prompt A".to_string();
    engine.set_config(config).await;

    let mut weles = engine
        .list_sub_agents()
        .await
        .into_iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("missing builtin weles entry");
    weles.reasoning_effort = Some("high".to_string());
    engine
        .set_sub_agent(weles)
        .await
        .expect("unrelated built-in edit should succeed");

    let stored = engine.get_config().await;
    assert_eq!(stored.builtin_sub_agents.weles.provider, None);
    assert_eq!(stored.builtin_sub_agents.weles.model, None);
    assert_eq!(stored.builtin_sub_agents.weles.system_prompt, None);

    let mut updated = stored;
    updated.provider = PROVIDER_ID_ANTHROPIC.to_string();
    updated.model = "claude-sonnet".to_string();
    updated.system_prompt = "Main prompt B".to_string();
    engine.set_config(updated).await;

    let weles = engine
        .list_sub_agents()
        .await
        .into_iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("missing builtin weles entry");
    assert_eq!(weles.provider, PROVIDER_ID_ANTHROPIC);
    assert_eq!(weles.model, "claude-sonnet");
    assert_eq!(weles.system_prompt.as_deref(), Some("Main prompt B"));
    assert_eq!(weles.reasoning_effort.as_deref(), Some("high"));
}

#[tokio::test]
async fn set_sub_agent_rejects_minimal_weles_override_payload_without_protection_metadata() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.system_prompt = "Main prompt".to_string();
    engine.set_config(config).await;

    let minimal = SubAgentDefinition {
        generation_settings: None,
        base_url: None,
        claude_permission_mode: None,
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: PROVIDER_ID_ANTHROPIC.to_string(),
        model: "claude-sonnet".to_string(),
        role: Some("governance".to_string()),
        system_prompt: Some("Escalated WELES prompt".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("high".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 0,
    };

    let error = engine
        .set_sub_agent(minimal)
        .await
        .expect_err("engine should require canonical WELES protection metadata");

    assert!(
        error
            .to_string()
            .contains("reserved built-in sub-agent id collision"),
        "unexpected error: {error}"
    );
}
