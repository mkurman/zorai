use crate::app::config_io::helpers::{
    normalize_compliance_mode, normalize_provider_auth_source, normalize_provider_transport,
};
use crate::app::TuiModel;
use crate::state::DaemonCommand;
use std::sync::mpsc;
use tokio::sync::mpsc::unbounded_channel;
use zorai_shared::providers::{
    PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_MINIMAX_CODING_PLAN, PROVIDER_ID_OPENAI,
    PROVIDER_ID_OPENROUTER,
};

pub(super) fn make_model() -> TuiModel {
    let (_client_tx, client_rx) = mpsc::channel();
    let (daemon_tx, _daemon_rx) = unbounded_channel();
    TuiModel::new(client_rx, daemon_tx)
}

pub(super) fn make_model_with_daemon_rx() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_client_tx, client_rx) = mpsc::channel();
    let (daemon_tx, daemon_rx) = unbounded_channel();
    (TuiModel::new(client_rx, daemon_tx), daemon_rx)
}

#[test]
fn normalize_provider_auth_source_falls_back_for_invalid_values() {
    assert_eq!(
        normalize_provider_auth_source(PROVIDER_ID_OPENAI, "bogus"),
        "api_key".to_string()
    );
    assert_eq!(
        normalize_provider_auth_source(PROVIDER_ID_OPENAI, "chatgpt_subscription"),
        "chatgpt_subscription".to_string()
    );
}

#[test]
fn normalize_provider_transport_falls_back_for_invalid_values() {
    assert_eq!(
        normalize_provider_transport(PROVIDER_ID_MINIMAX_CODING_PLAN, "bogus"),
        "chat_completions".to_string()
    );
    assert_eq!(
        normalize_provider_transport(PROVIDER_ID_OPENAI, "responses"),
        "responses".to_string()
    );
}

#[test]
fn normalize_compliance_mode_falls_back_to_standard() {
    assert_eq!(normalize_compliance_mode("soc2"), "soc2".to_string());
    assert_eq!(normalize_compliance_mode("bogus"), "standard".to_string());
}

#[test]
fn openrouter_provider_routing_round_trips_through_tui_config() {
    let mut model = make_model();
    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_OPENROUTER,
        "providers": {
            PROVIDER_ID_OPENROUTER: {
                "base_url": "https://openrouter.ai/api/v1",
                "model": "deepseek/deepseek-r1",
                "api_transport": "chat_completions",
                "auth_source": "api_key",
                "openrouter_provider_order": ["novita/fp8", "azure"],
                "openrouter_provider_ignore": ["deepinfra"],
                "openrouter_allow_fallbacks": false,
                "openrouter_response_cache_enabled": true
            }
        }
    }));

    assert_eq!(model.config.openrouter_provider_order, "novita/fp8, azure");
    assert_eq!(model.config.openrouter_provider_ignore, "deepinfra");
    assert!(!model.config.openrouter_allow_fallbacks);
    assert!(model.config.openrouter_response_cache_enabled);

    let json = model.build_config_patch_value();
    assert_eq!(
        json["providers"][PROVIDER_ID_OPENROUTER]["openrouter_provider_order"],
        serde_json::json!(["novita/fp8", "azure"])
    );
    assert_eq!(
        json["providers"][PROVIDER_ID_OPENROUTER]["openrouter_provider_ignore"],
        serde_json::json!(["deepinfra"])
    );
    assert_eq!(
        json["providers"][PROVIDER_ID_OPENROUTER]["openrouter_allow_fallbacks"],
        false
    );
    assert_eq!(
        json["providers"][PROVIDER_ID_OPENROUTER]["openrouter_response_cache_enabled"],
        true
    );
}

#[test]
fn build_config_patch_value_covers_all_daemon_backed_tabs() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.base_url = "https://example.invalid/v1".to_string();
    model.config.model = "gpt-5.4-mini".to_string();
    model.config.api_key = "sk-live".to_string();
    model.config.assistant_id = "asst_123".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_transport = "responses".to_string();
    model.config.reasoning_effort = "high".to_string();
    model.config.tool_bash = false;
    model.config.tool_web_search = true;
    model.config.search_provider = "exa".to_string();
    model.config.search_max_results = 12;
    model.config.search_timeout_secs = 45;
    model.config.enable_conversation_memory = false;
    model.config.operator_model_enabled = true;
    model.config.collaboration_enabled = true;
    model.config.compliance_sign_all_events = true;
    model.config.gateway_enabled = true;
    model.config.gateway_prefix = "/zorai".to_string();
    model.config.slack_token = "slack-secret".to_string();
    model.config.slack_channel_filter = "ops,alerts".to_string();
    model.config.telegram_token = "telegram-secret".to_string();
    model.config.telegram_allowed_chats = "1,2".to_string();
    model.config.discord_token = "discord-secret".to_string();
    model.config.discord_channel_filter = "1477397308600619194".to_string();
    model.config.discord_allowed_users = "alice,bob".to_string();
    model.config.whatsapp_phone_id = "phone-1".to_string();
    model.config.whatsapp_token = "whatsapp-secret".to_string();
    model.config.auto_compact_context = false;
    model.config.max_context_messages = 123;
    model.config.tui_chat_history_page_size = 222;
    model.config.max_tool_loops = 44;
    model.config.max_subagent_budget_extensions = 0;
    model.config.max_retries = 7;
    model.config.retry_delay_ms = 9_000;
    model.config.message_loop_delay_ms = 250;
    model.config.tool_call_delay_ms = 750;
    model.config.llm_stream_chunk_timeout_secs = 420;
    model.config.auto_refresh_interval_secs = 120;
    model.config.auto_retry = false;
    model.config.compact_threshold_pct = 91;
    model.config.keep_recent_on_compact = 17;
    model.config.bash_timeout_secs = 77;
    model.config.weles_max_concurrent_reviews = 4;
    model.config.compaction_strategy = "custom_model".to_string();
    model.config.compaction_weles_provider = PROVIDER_ID_OPENAI.to_string();
    model.config.compaction_weles_model = "gpt-5.4-mini".to_string();
    model.config.compaction_weles_reasoning_effort = "medium".to_string();
    model.config.compaction_custom_provider = PROVIDER_ID_OPENROUTER.to_string();
    model.config.compaction_custom_base_url = "https://openrouter.ai/api/v1".to_string();
    model.config.compaction_custom_model = "arcee-ai/trinity-large-thinking".to_string();
    model.config.compaction_custom_api_key = "sk-compaction".to_string();
    model.config.compaction_custom_assistant_id = "assist-compaction".to_string();
    model.config.compaction_custom_auth_source = "api_key".to_string();
    model.config.compaction_custom_api_transport = "chat_completions".to_string();
    model.config.compaction_custom_reasoning_effort = "high".to_string();
    model.config.compaction_custom_context_window_tokens = 333_000;
    model.config.snapshot_max_count = 15;
    model.config.agent_config_raw = Some(serde_json::json!({
        "agent_name": "Zorai",
        "system_prompt": "be sharp",
        "agent_backend": "daemon"
    }));

    let json = model.build_config_patch_value();

    assert_eq!(json["agent_name"], "Zorai");
    assert_eq!(json["system_prompt"], "be sharp");
    assert_eq!(json["provider"], PROVIDER_ID_OPENAI);
    assert_eq!(
        json["providers"][PROVIDER_ID_OPENAI]["assistant_id"],
        "asst_123"
    );
    assert!(json[PROVIDER_ID_OPENAI].get("api_key").is_none());
    assert!(json["providers"][PROVIDER_ID_OPENAI]
        .get("api_key")
        .is_none());
    assert_eq!(json["tools"]["bash"], false);
    assert_eq!(json["search_provider"], "exa");
    assert_eq!(json["enable_conversation_memory"], false);
    assert_eq!(json["operator_model"]["enabled"], true);
    assert_eq!(json["collaboration"]["enabled"], true);
    assert_eq!(json["compliance"]["sign_all_events"], true);
    assert_eq!(json["gateway"]["command_prefix"], "/zorai");
    assert_eq!(json["gateway"]["slack_token"], "slack-secret");
    assert_eq!(json["gateway"]["slack_channel_filter"], "ops,alerts");
    assert_eq!(json["gateway"]["telegram_token"], "telegram-secret");
    assert_eq!(json["gateway"]["telegram_allowed_chats"], "1,2");
    assert_eq!(json["gateway"]["discord_token"], "discord-secret");
    assert_eq!(
        json["gateway"]["discord_channel_filter"],
        "1477397308600619194"
    );
    assert_eq!(json["gateway"]["discord_allowed_users"], "alice,bob");
    assert_eq!(json["gateway"]["whatsapp_token"], "whatsapp-secret");
    assert_eq!(json["gateway"]["whatsapp_phone_id"], "phone-1");
    assert_eq!(json["auto_compact_context"], false);
    assert_eq!(json["max_context_messages"], 123);
    assert_eq!(json["tui_chat_history_page_size"], 222);
    assert_eq!(json["max_tool_loops"], 44);
    assert_eq!(json["max_subagent_budget_extensions"], 0);
    assert_eq!(json["max_retries"], 7);
    assert_eq!(json["retry_delay_ms"], 9000);
    assert_eq!(json["message_loop_delay_ms"], 250);
    assert_eq!(json["tool_call_delay_ms"], 750);
    assert_eq!(json["llm_stream_chunk_timeout_secs"], 420);
    assert_eq!(json["auto_refresh_interval_secs"], 120);
    assert_eq!(json["auto_retry"], false);
    assert!(
        json.get("context_budget_tokens").is_none(),
        "config patch should not serialize removed context budget settings"
    );
    assert_eq!(json["compact_threshold_pct"], 91);
    assert_eq!(json["keep_recent_on_compact"], 17);
    assert_eq!(json["bash_timeout_seconds"], 77);
    assert_eq!(
        json["builtin_sub_agents"]["weles"]["max_concurrent_reviews"],
        4
    );
    assert_eq!(json["compaction"]["strategy"], "custom_model");
    assert_eq!(json["compaction"]["weles"]["model"], "gpt-5.4-mini");
    assert_eq!(
        json["compaction"]["custom_model"]["provider"],
        PROVIDER_ID_OPENROUTER
    );
    assert_eq!(
        json["compaction"]["custom_model"]["context_window_tokens"],
        333000
    );
    assert_eq!(json["snapshot_retention"]["max_snapshots"], 15);
}

#[test]
fn build_config_patch_value_preserves_custom_model_context_override_for_non_custom_provider() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_OPENROUTER.to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.base_url = "https://openrouter.ai/api/v1".to_string();
    model.config.model = "openrouter/custom-preview".to_string();
    model.config.custom_model_name = "Custom Preview".to_string();
    model.config.context_window_tokens = 333_000;
    model.config.custom_context_window_tokens = Some(333_000);
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": "openrouter",
        "providers": {
            "openrouter": {
                "base_url": "https://openrouter.ai/api/v1",
                "model": "openrouter/custom-preview",
                "custom_model_name": "Custom Preview",
                "auth_source": "api_key",
                "api_transport": "chat_completions",
                "context_window_tokens": 333000
            }
        }
    }));

    let json = model.build_config_patch_value();

    assert_eq!(json["context_window_tokens"], serde_json::json!(333000));
    assert_eq!(
        json["providers"]["openrouter"]["context_window_tokens"],
        serde_json::json!(333000)
    );
    assert_eq!(
        json["openrouter"]["context_window_tokens"],
        serde_json::json!(333000)
    );
}

#[test]
fn apply_config_json_preserves_azure_openai_base_url() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_AZURE_OPENAI,
        "base_url": "https://my-resource.openai.azure.com/openai/v1",
        "model": "my-deployment",
        "providers": {
            PROVIDER_ID_AZURE_OPENAI: {
                "base_url": "https://my-resource.openai.azure.com/openai/v1",
                "model": "my-deployment",
                "auth_source": "api_key",
                "api_transport": "responses"
            }
        }
    }));

    assert_eq!(model.config.provider, PROVIDER_ID_AZURE_OPENAI);
    assert_eq!(
        model.config.base_url,
        "https://my-resource.openai.azure.com/openai/v1"
    );
    assert_eq!(model.config.model, "my-deployment");
    assert_eq!(model.config.api_transport, "responses");
}
