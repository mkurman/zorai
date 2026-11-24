use super::*;
use zorai_shared::providers::PROVIDER_ID_OPENAI;

#[tokio::test]
async fn merge_config_patch_preserves_existing_provider_state() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let mut config = engine.get_config().await;
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = "https://api.openai.com/v1".to_string();
    config.model = "gpt-5.4".to_string();
    config.api_key = "root-key".to_string();
    config.providers.insert(
        PROVIDER_ID_OPENAI.to_string(),
        ProviderConfig {
            presence_penalty: None,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: "openai-key".to_string(),
            assistant_id: "asst_openai".to_string(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::Responses,
            context_window_tokens: 128_000,
            reasoning_effort: "high".to_string(),
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        },
    );
    config.providers.insert(
        "groq".to_string(),
        ProviderConfig {
            presence_penalty: None,
            base_url: "https://api.groq.com/openai/v1".to_string(),
            model: "llama-3.3-70b-versatile".to_string(),
            api_key: "groq-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::Responses,
            context_window_tokens: 128_000,
            reasoning_effort: "high".to_string(),
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        },
    );
    engine.set_config(config).await;

    engine
        .merge_config_patch_json(r#"{"model":"gpt-5.4-mini"}"#)
        .await
        .unwrap();

    let updated = engine.get_config().await;
    assert_eq!(updated.model, "gpt-5.4-mini");
    assert_eq!(
        updated
            .providers
            .get(PROVIDER_ID_OPENAI)
            .map(|provider| provider.api_key.as_str()),
        Some("openai-key")
    );
    assert_eq!(
        updated
            .providers
            .get("groq")
            .map(|provider| provider.api_key.as_str()),
        Some("groq-key")
    );
}

#[tokio::test]
async fn merge_config_patch_sanitizes_stale_enum_strings() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .merge_config_patch_json(
            r#"{
                "agent_backend":"OpenClaw",
                "auth_source":"API-KEY",
                "api_transport":"chat completions",
                "concierge":{"detail_level":"daily briefing"},
                "compliance":{"mode":"SOC2"},
                "providers":{
                    "openai":{"auth_source":"chatgpt-subscription","api_transport":"native assistant"}
                }
            }"#,
        )
        .await
        .unwrap();

    let updated = engine.get_config().await;
    assert_eq!(updated.agent_backend, AgentBackend::Openclaw);
    assert_eq!(updated.auth_source, AuthSource::ApiKey);
    assert_eq!(updated.api_transport, ApiTransport::ChatCompletions);
    assert_eq!(
        updated.concierge.detail_level,
        ConciergeDetailLevel::DailyBriefing
    );
    assert_eq!(updated.compliance.mode, ComplianceMode::Soc2);
    let provider = updated.providers.get(PROVIDER_ID_OPENAI).unwrap();
    assert_eq!(provider.auth_source, AuthSource::ChatgptSubscription);
    assert_eq!(provider.api_transport, ApiTransport::NativeAssistant);
}

#[tokio::test]
async fn merge_config_patch_preserves_extended_gateway_fields() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .merge_config_patch_json(
            r#"{
                "gateway": {
                    "enabled": true,
                    "command_prefix": "!zorai",
                    "slack_token": "xoxb-test",
                    "slack_channel_filter": "ops,alerts",
                    "telegram_token": "tg-token",
                    "telegram_allowed_chats": "1,2",
                    "discord_token": "discord-token",
                    "discord_channel_filter": "deployments",
                    "discord_allowed_users": "alice,bob",
                    "whatsapp_allowed_contacts": "+48123456789",
                    "whatsapp_token": "wa-token",
                    "whatsapp_phone_id": "phone-id"
                }
            }"#,
        )
        .await
        .unwrap();

    let updated = engine.get_config().await;
    assert!(updated.gateway.enabled);
    assert_eq!(updated.gateway.command_prefix, "!zorai");
    assert_eq!(updated.gateway.slack_channel_filter, "ops,alerts");
    assert_eq!(updated.gateway.telegram_allowed_chats, "1,2");
    assert_eq!(updated.gateway.discord_channel_filter, "deployments");
    assert_eq!(updated.gateway.discord_allowed_users, "alice,bob");
    assert_eq!(updated.gateway.whatsapp_allowed_contacts, "+48123456789");
    assert_eq!(updated.gateway.whatsapp_token, "wa-token");
    assert_eq!(updated.gateway.whatsapp_phone_id, "phone-id");
}

#[tokio::test]
async fn merge_config_patch_preserves_whatsapp_link_fallback_flag() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let baseline = engine.get_config().await;
    assert!(!baseline.gateway.whatsapp_link_fallback_electron);

    engine
        .merge_config_patch_json(
            r#"{
                "gateway": {
                    "whatsapp_link_fallback_electron": true
                }
            }"#,
        )
        .await
        .unwrap();

    let updated = engine.get_config().await;
    assert!(updated.gateway.whatsapp_link_fallback_electron);
}

#[tokio::test]
async fn whatsapp_link_fallback_flag_persists_across_sqlite_roundtrip() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let baseline_items = engine
        .history
        .list_agent_config_items()
        .await
        .expect("baseline config should be readable");
    let baseline_loaded =
        load_config_from_items(baseline_items).expect("baseline config should deserialize");
    assert!(
        !baseline_loaded.gateway.whatsapp_link_fallback_electron,
        "default should remain false when unset"
    );

    engine
        .merge_config_patch_json(
            r#"{
                "gateway": {
                    "whatsapp_link_fallback_electron": true
                }
            }"#,
        )
        .await
        .expect("patch should persist fallback flag");

    let persisted_items = engine
        .history
        .list_agent_config_items()
        .await
        .expect("persisted config should be readable");
    let rehydrated =
        load_config_from_items(persisted_items).expect("persisted config should deserialize");
    assert!(
        rehydrated.gateway.whatsapp_link_fallback_electron,
        "override should survive sqlite write/read roundtrip"
    );
}
