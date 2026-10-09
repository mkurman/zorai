use super::normalize_provider_auth_source_falls_back_for_invalid_values_to_apply::*;
use crate::test_support::{env_var_lock, EnvVarGuard, ZORAI_DATA_DIR_ENV};
use rusqlite::Connection;
use tempfile::tempdir;
use zorai_shared::providers::*;
#[test]
fn build_config_patch_value_round_trips_daemon_backed_settings() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.base_url = "https://example.invalid/v1".to_string();
    model.config.model = "gpt-5.4-mini".to_string();
    model.config.reasoning_effort = "high".to_string();
    model.config.tool_bash = false;
    model.config.tool_file_ops = false;
    model.config.tool_web_search = false;
    model.config.tool_web_browse = true;
    model.config.tool_vision = true;
    model.config.tool_system_info = false;
    model.config.tool_gateway = false;
    model.config.search_provider = "exa".to_string();
    model.config.duckduckgo_region = "pl-pl".to_string();
    model.config.duckduckgo_safe_search = "off".to_string();
    model.config.firecrawl_api_key = "fc-secret".to_string();
    model.config.exa_api_key = "exa-secret".to_string();
    model.config.tavily_api_key = "tavily-secret".to_string();
    model.config.search_max_results = 12;
    model.config.search_timeout_secs = 45;
    model.config.browse_provider = "chrome".to_string();
    model.config.enable_streaming = false;
    model.config.enable_conversation_memory = false;
    model.config.enable_honcho_memory = true;
    model.config.honcho_api_key = "honcho-secret".to_string();
    model.config.honcho_base_url = "https://honcho.example".to_string();
    model.config.honcho_workspace_id = "workspace-123".to_string();
    model.config.anticipatory_enabled = true;
    model.config.anticipatory_morning_brief = true;
    model.config.anticipatory_predictive_hydration = true;
    model.config.anticipatory_stuck_detection = true;
    model.config.operator_model_enabled = true;
    model.config.operator_model_allow_message_statistics = true;
    model.config.operator_model_allow_approval_learning = true;
    model.config.operator_model_allow_attention_tracking = true;
    model.config.operator_model_allow_implicit_feedback = true;
    model.config.collaboration_enabled = true;
    model.config.compliance_mode = "soc2".to_string();
    model.config.compliance_retention_days = 365;
    model.config.compliance_sign_all_events = true;
    model.config.tool_synthesis_enabled = true;
    model.config.tool_synthesis_require_activation = false;
    model.config.tool_synthesis_max_generated_tools = 77;
    model.config.managed_sandbox_enabled = true;
    model.config.managed_security_level = "moderate".to_string();
    model.config.gateway_enabled = true;
    model.config.gateway_prefix = "/zorai".to_string();
    model.config.slack_token = "slack-secret".to_string();
    model.config.slack_channel_filter = "ops,alerts".to_string();
    model.config.telegram_token = "telegram-secret".to_string();
    model.config.telegram_allowed_chats = "1,2".to_string();
    model.config.discord_token = "discord-secret".to_string();
    model.config.discord_channel_filter = "1477397308600619194".to_string();
    model.config.discord_allowed_users = "alice,bob".to_string();
    model.config.whatsapp_allowed_contacts = "15551234567".to_string();
    model.config.whatsapp_token = "whatsapp-secret".to_string();
    model.config.whatsapp_phone_id = "phone-1".to_string();
    model.config.auto_compact_context = false;
    model.config.max_context_messages = 123;
    model.config.tui_chat_history_page_size = 222;
    model.config.max_tool_loops = 44;
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
    model.config.snapshot_max_size_mb = 2_048;
    model.config.snapshot_auto_cleanup = false;

    let json = model.build_config_patch_value();

    let mut reloaded = make_model();
    reloaded.apply_config_json(&json);

    assert_eq!(reloaded.config.provider, PROVIDER_ID_OPENAI);
    assert_eq!(reloaded.config.base_url, "https://example.invalid/v1");
    assert_eq!(reloaded.config.model, "gpt-5.4-mini");
    assert_eq!(reloaded.config.reasoning_effort, "high");
    assert_eq!(reloaded.config.tool_bash, false);
    assert_eq!(reloaded.config.tool_file_ops, false);
    assert_eq!(reloaded.config.tool_web_search, false);
    assert_eq!(reloaded.config.tool_web_browse, true);
    assert_eq!(reloaded.config.tool_vision, true);
    assert_eq!(reloaded.config.tool_system_info, false);
    assert_eq!(reloaded.config.tool_gateway, false);
    assert_eq!(reloaded.config.search_provider, "exa");
    assert_eq!(reloaded.config.duckduckgo_region, "pl-pl");
    assert_eq!(reloaded.config.duckduckgo_safe_search, "off");
    assert_eq!(reloaded.config.firecrawl_api_key, "fc-secret");
    assert_eq!(reloaded.config.exa_api_key, "exa-secret");
    assert_eq!(reloaded.config.tavily_api_key, "tavily-secret");
    assert_eq!(reloaded.config.search_max_results, 12);
    assert_eq!(reloaded.config.search_timeout_secs, 45);
    assert_eq!(reloaded.config.browse_provider, "chrome");
    assert_eq!(reloaded.config.enable_streaming, false);
    assert_eq!(reloaded.config.enable_conversation_memory, false);
    assert_eq!(reloaded.config.enable_honcho_memory, true);
    assert_eq!(reloaded.config.honcho_api_key, "honcho-secret");
    assert_eq!(reloaded.config.honcho_base_url, "https://honcho.example");
    assert_eq!(reloaded.config.honcho_workspace_id, "workspace-123");
    assert_eq!(reloaded.config.anticipatory_enabled, true);
    assert_eq!(reloaded.config.anticipatory_morning_brief, true);
    assert_eq!(reloaded.config.anticipatory_predictive_hydration, true);
    assert_eq!(reloaded.config.anticipatory_stuck_detection, true);
    assert_eq!(reloaded.config.operator_model_enabled, true);
    assert_eq!(
        reloaded.config.operator_model_allow_message_statistics,
        true
    );
    assert_eq!(reloaded.config.operator_model_allow_approval_learning, true);
    assert_eq!(
        reloaded.config.operator_model_allow_attention_tracking,
        true
    );
    assert_eq!(reloaded.config.operator_model_allow_implicit_feedback, true);
    assert_eq!(reloaded.config.collaboration_enabled, true);
    assert_eq!(reloaded.config.compliance_mode, "soc2");
    assert_eq!(reloaded.config.compliance_retention_days, 365);
    assert_eq!(reloaded.config.compliance_sign_all_events, true);
    assert_eq!(reloaded.config.tool_synthesis_enabled, true);
    assert_eq!(reloaded.config.tool_synthesis_require_activation, false);
    assert_eq!(reloaded.config.tool_synthesis_max_generated_tools, 77);
    assert_eq!(reloaded.config.managed_sandbox_enabled, true);
    assert_eq!(reloaded.config.managed_security_level, "moderate");
    assert_eq!(reloaded.config.gateway_enabled, true);
    assert_eq!(reloaded.config.gateway_prefix, "/zorai");
    assert_eq!(reloaded.config.slack_token, "slack-secret");
    assert_eq!(reloaded.config.slack_channel_filter, "ops,alerts");
    assert_eq!(reloaded.config.telegram_token, "telegram-secret");
    assert_eq!(reloaded.config.telegram_allowed_chats, "1,2");
    assert_eq!(reloaded.config.discord_token, "discord-secret");
    assert_eq!(
        reloaded.config.discord_channel_filter,
        "1477397308600619194"
    );
    assert_eq!(reloaded.config.discord_allowed_users, "alice,bob");
    assert_eq!(reloaded.config.whatsapp_allowed_contacts, "15551234567");
    assert_eq!(reloaded.config.whatsapp_token, "whatsapp-secret");
    assert_eq!(reloaded.config.whatsapp_phone_id, "phone-1");
    assert_eq!(reloaded.config.auto_compact_context, false);
    assert_eq!(reloaded.config.max_context_messages, 123);
    assert_eq!(reloaded.config.tui_chat_history_page_size, 222);
    assert_eq!(reloaded.config.max_tool_loops, 44);
    assert_eq!(reloaded.config.max_retries, 7);
    assert_eq!(reloaded.config.retry_delay_ms, 9_000);
    assert_eq!(reloaded.config.message_loop_delay_ms, 250);
    assert_eq!(reloaded.config.tool_call_delay_ms, 750);
    assert_eq!(reloaded.config.llm_stream_chunk_timeout_secs, 420);
    assert_eq!(reloaded.config.auto_refresh_interval_secs, 120);
    assert_eq!(reloaded.config.auto_retry, false);
    assert_eq!(reloaded.config.compact_threshold_pct, 91);
    assert_eq!(reloaded.config.keep_recent_on_compact, 17);
    assert_eq!(reloaded.config.bash_timeout_secs, 77);
    assert_eq!(reloaded.config.weles_max_concurrent_reviews, 4);
    assert_eq!(reloaded.config.compaction_strategy, "custom_model");
    assert_eq!(
        reloaded.config.compaction_weles_provider,
        PROVIDER_ID_OPENAI
    );
    assert_eq!(reloaded.config.compaction_weles_model, "gpt-5.4-mini");
    assert_eq!(reloaded.config.compaction_weles_reasoning_effort, "medium");
    assert_eq!(
        reloaded.config.compaction_custom_provider,
        PROVIDER_ID_OPENROUTER
    );
    assert_eq!(
        reloaded.config.compaction_custom_base_url,
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        reloaded.config.compaction_custom_model,
        "arcee-ai/trinity-large-thinking"
    );
    assert_eq!(reloaded.config.compaction_custom_api_key, "sk-compaction");
    assert_eq!(
        reloaded.config.compaction_custom_assistant_id,
        "assist-compaction"
    );
    assert_eq!(reloaded.config.compaction_custom_auth_source, "api_key");
    assert_eq!(
        reloaded.config.compaction_custom_api_transport,
        "chat_completions"
    );
    assert_eq!(reloaded.config.compaction_custom_reasoning_effort, "high");
    assert_eq!(
        reloaded.config.compaction_custom_context_window_tokens,
        333_000
    );
    assert_eq!(reloaded.config.snapshot_max_count, 15);
    assert_eq!(reloaded.config.snapshot_max_size_mb, 2_048);
    assert_eq!(reloaded.config.snapshot_auto_cleanup, false);
}

#[test]
fn build_config_patch_value_round_trips_disabled_snapshot_retention() {
    let mut model = make_model();
    model.config.snapshot_max_count = 0;
    model.config.snapshot_max_size_mb = 10_240;
    model.config.snapshot_auto_cleanup = false;

    let json = model.build_config_patch_value();

    assert_eq!(json["snapshot_retention"]["max_snapshots"], 0);
    assert_eq!(json["snapshot_retention"]["auto_cleanup"], false);

    let mut reloaded = make_model();
    reloaded.apply_config_json(&json);

    assert_eq!(reloaded.config.snapshot_max_count, 0);
    assert_eq!(reloaded.config.snapshot_max_size_mb, 10_240);
    assert!(!reloaded.config.snapshot_auto_cleanup);
}

#[test]
fn refresh_snapshot_stats_reads_daemon_snapshot_index_db() {
    let _lock = env_var_lock();
    let home = tempdir().expect("tempdir");
    let zorai_dir = home.path().join("zorai-data");
    let history_dir = zorai_dir.join("history");
    std::fs::create_dir_all(&history_dir).expect("create history dir");

    let snap_a = zorai_dir.join("snap-a.tar.gz");
    let snap_b = zorai_dir.join("snap-b.tar.gz");
    std::fs::write(&snap_a, vec![b'a'; 128]).expect("write snapshot a");
    std::fs::write(&snap_b, vec![b'b'; 256]).expect("write snapshot b");

    let db_path = history_dir.join("command-history.db");
    let conn = Connection::open(&db_path).expect("open sqlite db");
    conn.execute(
        "CREATE TABLE snapshot_index (snapshot_id TEXT PRIMARY KEY, path TEXT NOT NULL)",
        [],
    )
    .expect("create snapshot index table");
    conn.execute(
        "INSERT INTO snapshot_index (snapshot_id, path) VALUES (?1, ?2)",
        rusqlite::params!["snap-a", snap_a.to_string_lossy().to_string()],
    )
    .expect("insert snapshot a");
    conn.execute(
        "INSERT INTO snapshot_index (snapshot_id, path) VALUES (?1, ?2)",
        rusqlite::params!["snap-b", snap_b.to_string_lossy().to_string()],
    )
    .expect("insert snapshot b");

    let _data_dir = EnvVarGuard::set(ZORAI_DATA_DIR_ENV, &zorai_dir);

    let mut model = make_model();
    model.load_saved_settings();

    assert_eq!(model.config.snapshot_count, 2);
    assert_eq!(model.config.snapshot_total_size_bytes, 384);
}

#[test]
fn refresh_snapshot_stats_ignores_stale_snapshot_index_rows() {
    let _lock = env_var_lock();
    let home = tempdir().expect("tempdir");
    let zorai_dir = home.path().join("zorai-data");
    let history_dir = zorai_dir.join("history");
    std::fs::create_dir_all(&history_dir).expect("create history dir");

    let snap_a = zorai_dir.join("snap-a.tar.gz");
    std::fs::write(&snap_a, vec![b'a'; 128]).expect("write snapshot a");
    let missing = zorai_dir.join("missing.tar.gz");

    let db_path = history_dir.join("command-history.db");
    let conn = Connection::open(&db_path).expect("open sqlite db");
    conn.execute(
        "CREATE TABLE snapshot_index (snapshot_id TEXT PRIMARY KEY, path TEXT NOT NULL)",
        [],
    )
    .expect("create snapshot index table");
    conn.execute(
        "INSERT INTO snapshot_index (snapshot_id, path) VALUES (?1, ?2)",
        rusqlite::params!["snap-a", snap_a.to_string_lossy().to_string()],
    )
    .expect("insert snapshot a");
    conn.execute(
        "INSERT INTO snapshot_index (snapshot_id, path) VALUES (?1, ?2)",
        rusqlite::params!["missing", missing.to_string_lossy().to_string()],
    )
    .expect("insert missing snapshot row");

    let _data_dir = EnvVarGuard::set(ZORAI_DATA_DIR_ENV, &zorai_dir);

    let mut model = make_model();
    model.load_saved_settings();

    assert_eq!(model.config.snapshot_count, 1);
    assert_eq!(model.config.snapshot_total_size_bytes, 128);
}

#[test]
fn apply_config_json_prefers_active_provider_transport_over_stale_root_transport() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
        "api_transport": "chat_completions",
        "providers": {
            "github-copilot": {
                "base_url": "https://api.githubcopilot.com",
                "model": "gpt-5.4",
                "api_transport": "responses",
                "auth_source": "github_copilot"
            }
        }
    }));

    assert_eq!(model.config.provider, PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(model.config.api_transport, "responses");
}

#[test]
fn apply_config_json_prefers_active_provider_auth_source_over_stale_root_auth_source() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
        "auth_source": "api_key",
        "providers": {
            "github-copilot": {
                "base_url": "https://api.githubcopilot.com",
                "model": "gpt-5.4",
                "api_transport": "responses",
                "auth_source": "github_copilot"
            }
        }
    }));

    assert_eq!(model.config.provider, PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(model.config.auth_source, "github_copilot");
}

#[test]
fn apply_config_json_uses_daemon_retry_delay_default_when_missing() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5.4",
                "auth_source": "api_key",
                "api_transport": "responses"
            }
        }
    }));

    assert_eq!(model.config.retry_delay_ms, 5_000);
    assert_eq!(model.config.tui_chat_history_page_size, 100);
    assert_eq!(model.config.message_loop_delay_ms, 100);
    assert_eq!(model.config.tool_call_delay_ms, 100);
}

#[test]
fn apply_provider_selection_reads_nested_provider_config_values() {
    let mut model = make_model();

    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "providers": {
            "alibaba-coding-plan": {
                "base_url": "https://coding-intl.dashscope.aliyuncs.com/v1",
                "model": "qwen3-coder-next",
                "api_key": "dashscope-key",
                "assistant_id": "",
                "auth_source": "api_key",
                "api_transport": "chat_completions",
                "context_window_tokens": 204800
            }
        }
    }));

    model.apply_provider_selection(PROVIDER_ID_ALIBABA_CODING_PLAN);

    assert_eq!(model.config.provider, PROVIDER_ID_ALIBABA_CODING_PLAN);
    assert_eq!(model.config.api_key, "dashscope-key");
    assert_eq!(model.config.model, "qwen3-coder-next");
    assert_eq!(
        model.config.base_url,
        "https://coding-intl.dashscope.aliyuncs.com/v1"
    );
}

#[test]
fn build_config_patch_value_preserves_nested_inactive_provider_auth_modes() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.base_url = "https://models.github.ai/inference".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.api_transport = "responses".to_string();
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5.4",
                "auth_source": "chatgpt_subscription",
                "api_transport": "responses"
            },
            "github-copilot": {
                "base_url": "https://models.github.ai/inference",
                "model": "gpt-5.4",
                "auth_source": "github_copilot",
                "api_transport": "responses"
            }
        }
    }));

    let json = model.build_config_patch_value();

    assert_eq!(
        json["providers"][PROVIDER_ID_OPENAI]["auth_source"],
        "chatgpt_subscription"
    );
    assert_eq!(
        json["providers"][PROVIDER_ID_OPENAI]["api_transport"],
        "responses"
    );
    assert_eq!(
        json["providers"][PROVIDER_ID_GITHUB_COPILOT]["auth_source"],
        "github_copilot"
    );
}
