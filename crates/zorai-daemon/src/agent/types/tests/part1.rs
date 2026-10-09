use super::super::*;
use zorai_shared::providers::{
    PROVIDER_ID_ANTHROPIC, PROVIDER_ID_OPENAI, PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
    PROVIDER_ID_Z_AI_CODING_PLAN,
};

/// FOUN-05: Channel capacity is configurable via AgentConfig.
#[test]
fn configurable_channel_capacity() {
    let json_minimal = r#"{}"#;
    let parsed: AgentConfig = serde_json::from_str(json_minimal).unwrap();
    assert_eq!(parsed.pty_channel_capacity, 1024);
    assert_eq!(parsed.agent_event_channel_capacity, 512);
    assert!(parsed.auto_retry);
    assert_eq!(
        parsed.concierge.detail_level,
        ConciergeDetailLevel::ContextSummary
    );
    assert_eq!(parsed.retry_delay_ms, 5_000);
    assert_eq!(parsed.message_loop_delay_ms, 100);
    assert_eq!(parsed.tool_call_delay_ms, 100);
    assert_eq!(parsed.llm_stream_chunk_timeout_secs, 300);

    let json = r#"{"pty_channel_capacity": 2048, "agent_event_channel_capacity": 1024}"#;
    let parsed: AgentConfig = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.pty_channel_capacity, 2048);
    assert_eq!(parsed.agent_event_channel_capacity, 1024);
}

#[test]
fn alibaba_coding_plan_uses_openai_by_default() {
    assert_eq!(
        get_provider_api_type(
            "alibaba-coding-plan",
            "qwen3.6-plus",
            "https://coding-intl.dashscope.aliyuncs.com/v1"
        ),
        ApiType::OpenAI
    );
}

#[test]
fn alibaba_coding_plan_switches_to_anthropic_for_anthropic_base_url() {
    assert_eq!(
        get_provider_api_type(
            "alibaba-coding-plan",
            "qwen3.6-plus",
            "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic"
        ),
        ApiType::Anthropic
    );
}

#[test]
fn alibaba_token_plan_uses_openai_by_default() {
    assert_eq!(
        get_provider_api_type(
            "alibaba-token-plan",
            "qwen3.7-max",
            "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"
        ),
        ApiType::OpenAI
    );
}

#[test]
fn alibaba_token_plan_switches_to_anthropic_for_anthropic_base_url() {
    assert_eq!(
        get_provider_api_type(
            "alibaba-token-plan",
            "qwen3.7-max",
            "https://token-plan.ap-southeast-1.maas.aliyuncs.com/apps/anthropic"
        ),
        ApiType::Anthropic
    );
}

#[test]
fn github_copilot_claude_models_stay_openai_without_explicit_transport_override() {
    assert_eq!(
        get_provider_api_type(
            "github-copilot",
            "claude-sonnet-4.6",
            "https://api.githubcopilot.com"
        ),
        ApiType::OpenAI
    );
}

#[test]
fn default_retry_delay_is_five_seconds() {
    let parsed: AgentConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(parsed.retry_delay_ms, 5_000);
}

#[test]
fn default_sleep_delays_are_one_hundred_ms() {
    let parsed: AgentConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(parsed.message_loop_delay_ms, 100);
    assert_eq!(parsed.tool_call_delay_ms, 100);
}

#[test]
fn stream_chunk_timeout_defaults_to_five_minutes() {
    let parsed: AgentConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(parsed.llm_stream_chunk_timeout_secs, 300);
}

/// FOUN-04: Circuit breaker AgentEvent variants serialize and deserialize correctly.

#[test]
fn heartbeat_checks_config_deserializes_from_empty_json() {
    let cfg: HeartbeatChecksConfig = serde_json::from_str("{}").unwrap();
    assert!(cfg.stale_todos_enabled);
    assert_eq!(cfg.stale_todo_threshold_hours, 24);
    assert!(cfg.stuck_goals_enabled);
    assert_eq!(cfg.stuck_goal_threshold_hours, 2);
    assert!(cfg.unreplied_messages_enabled);
    assert_eq!(cfg.unreplied_message_threshold_hours, 1);
    assert!(cfg.repo_changes_enabled);
    assert!(cfg.plugin_auth_enabled);
    assert!(cfg.stale_todos_cron.is_none());
    assert!(cfg.stuck_goals_cron.is_none());
    assert!(cfg.unreplied_messages_cron.is_none());
    assert!(cfg.repo_changes_cron.is_none());
    assert!(cfg.plugin_auth_cron.is_none());
    assert!((cfg.stale_todos_priority_weight - 1.0).abs() < f64::EPSILON);
    assert!((cfg.stuck_goals_priority_weight - 1.0).abs() < f64::EPSILON);
    assert!((cfg.unreplied_messages_priority_weight - 1.0).abs() < f64::EPSILON);
    assert!((cfg.repo_changes_priority_weight - 1.0).abs() < f64::EPSILON);
    assert!((cfg.plugin_auth_priority_weight - 1.0).abs() < f64::EPSILON);
}

#[test]
fn heartbeat_checks_config_priority_overrides_default_none() {
    let cfg: HeartbeatChecksConfig = serde_json::from_str("{}").unwrap();
    assert!(cfg.stale_todos_priority_override.is_none());
    assert!(cfg.stuck_goals_priority_override.is_none());
    assert!(cfg.unreplied_messages_priority_override.is_none());
    assert!(cfg.repo_changes_priority_override.is_none());
    assert!(cfg.plugin_auth_priority_override.is_none());
    assert!(!cfg.reset_learned_priorities);
}

#[test]
fn agent_config_adaptive_heartbeat_defaults() {
    let cfg: AgentConfig = serde_json::from_str("{}").unwrap();
    assert!((cfg.ema_alpha - 0.3).abs() < f64::EPSILON);
    assert_eq!(cfg.low_activity_frequency_factor, 4);
    assert!((cfg.ema_activity_threshold - 2.0).abs() < f64::EPSILON);
}

#[test]
fn heartbeat_check_type_serializes_to_snake_case() {
    assert_eq!(
        serde_json::to_string(&HeartbeatCheckType::StaleTodos).unwrap(),
        "\"stale_todos\""
    );
    assert_eq!(
        serde_json::to_string(&HeartbeatCheckType::StuckGoalRuns).unwrap(),
        "\"stuck_goal_runs\""
    );
    assert_eq!(
        serde_json::to_string(&HeartbeatCheckType::UnrepliedGatewayMessages).unwrap(),
        "\"unreplied_gateway_messages\""
    );
    assert_eq!(
        serde_json::to_string(&HeartbeatCheckType::RepoChanges).unwrap(),
        "\"repo_changes\""
    );
    assert_eq!(
        serde_json::to_string(&HeartbeatCheckType::PluginAuth).unwrap(),
        "\"plugin_auth\""
    );
}

#[test]
fn check_severity_serializes_to_snake_case() {
    assert_eq!(
        serde_json::to_string(&CheckSeverity::Low).unwrap(),
        "\"low\""
    );
    assert_eq!(
        serde_json::to_string(&CheckSeverity::Medium).unwrap(),
        "\"medium\""
    );
    assert_eq!(
        serde_json::to_string(&CheckSeverity::High).unwrap(),
        "\"high\""
    );
    assert_eq!(
        serde_json::to_string(&CheckSeverity::Critical).unwrap(),
        "\"critical\""
    );
}

#[test]
fn heartbeat_check_result_roundtrips_through_serde() {
    let result = HeartbeatCheckResult {
        check_type: HeartbeatCheckType::StaleTodos,
        items_found: 2,
        summary: "2 stale TODO(s)".to_string(),
        details: vec![CheckDetail {
            id: "todo-1".to_string(),
            label: "Fix the bug".to_string(),
            age_hours: 48.5,
            severity: CheckSeverity::High,
            context: "TODO pending for 48.5h".to_string(),
        }],
    };
    let json = serde_json::to_string(&result).unwrap();
    let parsed: HeartbeatCheckResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.check_type, HeartbeatCheckType::StaleTodos);
    assert_eq!(parsed.items_found, 2);
    assert_eq!(parsed.details.len(), 1);
    assert_eq!(parsed.details[0].severity, CheckSeverity::High);
}

#[test]
fn heartbeat_digest_item_roundtrips_through_serde() {
    let item = HeartbeatDigestItem {
        priority: 1,
        check_type: HeartbeatCheckType::StuckGoalRuns,
        title: "Stuck goal run".to_string(),
        suggestion: "Consider cancelling".to_string(),
    };
    let json = serde_json::to_string(&item).unwrap();
    let parsed: HeartbeatDigestItem = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.priority, 1);
    assert_eq!(parsed.check_type, HeartbeatCheckType::StuckGoalRuns);
    assert_eq!(parsed.title, "Stuck goal run");
    assert_eq!(parsed.suggestion, "Consider cancelling");
}

#[test]
fn agent_config_backward_compat_new_heartbeat_fields() {
    let json = r#"{}"#;
    let parsed: AgentConfig = serde_json::from_str(json).unwrap();
    assert!(parsed.heartbeat_cron.is_none());
    assert!(parsed.heartbeat_checks.stale_todos_enabled);
    assert_eq!(parsed.heartbeat_checks.stale_todo_threshold_hours, 24);
    assert!(parsed.quiet_hours_start.is_none());
    assert!(parsed.quiet_hours_end.is_none());
    assert!(!parsed.dnd_enabled);
}

#[test]
fn agent_event_heartbeat_digest_serde_roundtrip() {
    let event = AgentEvent::HeartbeatDigest {
        cycle_id: "cycle-1".to_string(),
        actionable: true,
        digest: "2 items need attention".to_string(),
        items: vec![HeartbeatDigestItem {
            priority: 1,
            check_type: HeartbeatCheckType::StaleTodos,
            title: "Stale todos".to_string(),
            suggestion: "Review pending items".to_string(),
        }],
        checked_at: 1234567890,
        explanation: Some("Heartbeat found stale items".to_string()),
        confidence: Some(0.85),
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::HeartbeatDigest {
            cycle_id,
            actionable,
            digest,
            items,
            checked_at,
            explanation,
            confidence,
        } => {
            assert_eq!(cycle_id, "cycle-1");
            assert!(actionable);
            assert_eq!(digest, "2 items need attention");
            assert_eq!(items.len(), 1);
            assert_eq!(checked_at, 1234567890);
            assert_eq!(explanation.as_deref(), Some("Heartbeat found stale items"));
            assert!((confidence.unwrap() - 0.85).abs() < f64::EPSILON);
        }
        _ => panic!("wrong variant after deserialize"),
    }
}

#[test]
fn agent_event_weles_health_update_serde_roundtrip() {
    let event = AgentEvent::WelesHealthUpdate {
        state: WelesHealthState::Degraded,
        reason: Some("WELES review unavailable for guarded actions".to_string()),
        checked_at: 1234567890,
    };

    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::WelesHealthUpdate {
            state,
            reason,
            checked_at,
        } => {
            assert_eq!(state, WelesHealthState::Degraded);
            assert_eq!(checked_at, 1234567890);
            assert_eq!(
                reason.as_deref(),
                Some("WELES review unavailable for guarded actions")
            );
        }
        _ => panic!("wrong variant after deserialize"),
    }
}

#[test]
fn agent_event_notification_inbox_upsert_serde_roundtrip() {
    let event = AgentEvent::NotificationInboxUpsert {
        notification: zorai_protocol::InboxNotification {
            id: "plugin-auth:gmail".to_string(),
            source: "plugin_auth".to_string(),
            kind: "plugin_needs_reconnect".to_string(),
            title: "Gmail needs reconnect".to_string(),
            body: "Reconnect Gmail to keep using the plugin.".to_string(),
            subtitle: Some("plugin auth".to_string()),
            severity: "warning".to_string(),
            created_at: 100,
            updated_at: 200,
            read_at: None,
            archived_at: None,
            deleted_at: None,
            actions: vec![zorai_protocol::InboxNotificationAction {
                id: "open_plugin_settings".to_string(),
                label: "Open plugin settings".to_string(),
                action_type: "open_plugin_settings".to_string(),
                target: Some("gmail".to_string()),
                payload_json: None,
            }],
            metadata_json: None,
        },
    };

    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::NotificationInboxUpsert { notification } => {
            assert_eq!(notification.id, "plugin-auth:gmail");
            assert_eq!(notification.kind, "plugin_needs_reconnect");
            assert_eq!(notification.actions.len(), 1);
            assert_eq!(notification.actions[0].target.as_deref(), Some("gmail"));
        }
        _ => panic!("wrong variant after deserialize"),
    }
}

#[test]
fn interval_mins_to_cron_converts_correctly() {
    assert_eq!(interval_mins_to_cron(1), "* * * * *");
    assert_eq!(interval_mins_to_cron(15), "*/15 * * * *");
    assert_eq!(interval_mins_to_cron(60), "0 * * * *");
    assert_eq!(interval_mins_to_cron(120), "0 */2 * * *");
    assert_eq!(interval_mins_to_cron(0), "* * * * *");
}

#[test]
fn openai_curated_media_modalities_match_expectations() {
    let provider = get_provider_definition(PROVIDER_ID_OPENAI).expect("openai provider");

    assert_eq!(provider.default_model, "gpt-5.5");
    assert_eq!(
        provider.models.first().map(|model| model.id),
        Some("gpt-5.5")
    );
    assert_eq!(
        model_modalities(PROVIDER_ID_OPENAI, "gpt-6-astra"),
        TEXT_IMAGE
    );
    assert_eq!(model_modalities(PROVIDER_ID_OPENAI, "gpt-5.5"), MULTIMODAL);
    assert_eq!(
        model_modalities(PROVIDER_ID_OPENAI, "gpt-5.6-sol"),
        TEXT_IMAGE
    );
    assert_eq!(
        model_modalities(PROVIDER_ID_OPENAI, "gpt-5.6-terra"),
        TEXT_IMAGE
    );
    assert_eq!(
        model_modalities(PROVIDER_ID_OPENAI, "gpt-5.6-luna"),
        TEXT_IMAGE
    );
    assert_eq!(model_modalities(PROVIDER_ID_OPENAI, "gpt-5.4"), MULTIMODAL);
    assert_eq!(
        model_modalities(PROVIDER_ID_OPENAI, "gpt-5.4-mini"),
        TEXT_IMAGE
    );
    assert_eq!(
        model_modalities(PROVIDER_ID_OPENAI, "gpt-5.3-codex-spark"),
        TEXT_ONLY
    );
    assert!(model_supports(
        PROVIDER_ID_OPENAI,
        "gpt-5.5",
        Modality::Audio
    ));
    assert!(model_supports(
        PROVIDER_ID_OPENAI,
        "gpt-5.5",
        Modality::Video
    ));
    assert!(model_supports(
        PROVIDER_ID_OPENAI,
        "gpt-5.4",
        Modality::Audio
    ));
    assert!(model_supports(
        PROVIDER_ID_OPENAI,
        "gpt-5.4",
        Modality::Video
    ));
    assert!(!model_supports(
        PROVIDER_ID_OPENAI,
        "gpt-5.4-mini",
        Modality::Audio
    ));
    assert!(!model_supports(
        PROVIDER_ID_OPENAI,
        "gpt-5.4-mini",
        Modality::Video
    ));
    assert!(!model_supports(
        PROVIDER_ID_OPENAI,
        "gpt-5.3-codex-spark",
        Modality::Image
    ));
}

#[test]
fn z_ai_coding_plan_catalog_uses_current_supported_models() {
    let provider = get_provider_definition(PROVIDER_ID_Z_AI_CODING_PLAN).expect("z.ai provider");

    assert_eq!(provider.default_model, "glm-5.3");
    assert_eq!(
        provider.models.first().map(|model| model.id),
        Some("glm-5.3")
    );
    assert_eq!(
        provider.models.get(1).map(|model| model.id),
        Some("glm-5.3-flash")
    );
    assert_eq!(
        provider.models.get(1).map(|model| model.context_window),
        Some(1_000_000)
    );
    assert_eq!(
        provider.models.get(2).map(|model| model.id),
        Some("glm-5-turbo")
    );
    assert_eq!(
        provider.models.get(2).map(|model| model.context_window),
        Some(200_000)
    );
    assert_eq!(
        provider.models.get(3).map(|model| model.id),
        Some("glm-4.7")
    );
}

#[test]
fn anthropic_models_remain_text_image_only() {
    assert_eq!(
        model_modalities(PROVIDER_ID_ANTHROPIC, "claude-opus-4-7"),
        TEXT_IMAGE
    );
    assert!(model_supports(
        PROVIDER_ID_ANTHROPIC,
        "claude-opus-4-7",
        Modality::Image
    ));
    assert!(!model_supports(
        PROVIDER_ID_ANTHROPIC,
        "claude-opus-4-7",
        Modality::Audio
    ));
    assert!(!model_supports(
        PROVIDER_ID_ANTHROPIC,
        "claude-opus-4-7",
        Modality::Video
    ));
}

#[test]
fn xiaomi_mimo_omni_is_multimodal() {
    assert_eq!(
        model_modalities(PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN, "mimo-v2-omni"),
        MULTIMODAL
    );
    assert!(model_supports(
        PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
        "mimo-v2-omni",
        Modality::Audio
    ));
    assert!(model_supports(
        PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
        "mimo-v2-omni",
        Modality::Video
    ));
}

#[test]
fn xiaomi_mimo_v25_series_modalities_match_capabilities() {
    assert_eq!(
        model_modalities(PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN, "mimo-v2.5"),
        MULTIMODAL
    );
    assert!(model_supports(
        PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
        "mimo-v2.5",
        Modality::Audio
    ));
    assert_eq!(
        model_modalities(PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN, "mimo-v2.5-asr"),
        TEXT_AUDIO
    );
    assert_eq!(
        model_modalities(PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN, "mimo-v2.5-tts"),
        TEXT_AUDIO
    );
    assert!(!model_supports(
        PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
        "mimo-v2.5-tts",
        Modality::Video
    ));
}

#[test]
fn arcee_catalog_stays_conservative_text_only() {
    assert_eq!(
        model_modalities(
            zorai_shared::providers::PROVIDER_ID_ARCEE,
            "trinity-large-thinking"
        ),
        TEXT_ONLY
    );
    assert!(model_supports(
        zorai_shared::providers::PROVIDER_ID_ARCEE,
        "trinity-large-thinking",
        Modality::Text
    ));
    assert!(!model_supports(
        zorai_shared::providers::PROVIDER_ID_ARCEE,
        "trinity-large-thinking",
        Modality::Image
    ));
    assert!(!model_supports(
        zorai_shared::providers::PROVIDER_ID_ARCEE,
        "trinity-large-thinking",
        Modality::Audio
    ));
}

#[test]
fn circuit_breaker_event_serde_roundtrip() {
    let event = AgentEvent::ProviderCircuitOpen {
        provider: PROVIDER_ID_OPENAI.to_string(),
        failed_model: None,
        trip_count: 3,
        reason: "circuit breaker open".to_string(),
        suggested_alternatives: vec![ProviderAlternativeSuggestion {
            provider_id: "groq".to_string(),
            model: Some("llama-3.3-70b-versatile".to_string()),
            reason: "healthy and configured".to_string(),
        }],
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::ProviderCircuitOpen {
            provider,
            failed_model,
            trip_count,
            reason,
            suggested_alternatives,
        } => {
            assert_eq!(provider, PROVIDER_ID_OPENAI);
            assert!(failed_model.is_none());
            assert_eq!(trip_count, 3);
            assert_eq!(reason, "circuit breaker open");
            assert_eq!(suggested_alternatives.len(), 1);
            assert_eq!(suggested_alternatives[0].provider_id, "groq");
        }
        _ => panic!("wrong variant after deserialize"),
    }

    let recovery = AgentEvent::ProviderCircuitRecovered {
        provider: PROVIDER_ID_ANTHROPIC.to_string(),
    };
    let json = serde_json::to_string(&recovery).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::ProviderCircuitRecovered { provider } => {
            assert_eq!(provider, PROVIDER_ID_ANTHROPIC);
        }
        _ => panic!("wrong variant after deserialize"),
    }
}
