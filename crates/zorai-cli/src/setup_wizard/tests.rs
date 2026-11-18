use super::flow::{
    parse_fetch_models_terminal_response, parse_gh_cli_token_output,
    parse_set_config_item_response, provider_uses_browser_auth, setup_probe_from_config_json,
    should_validate_provider_after_setup, AuthSetupResult, SetupProbe,
};
use super::*;

#[test]
fn test_select_list_wraps_index() {
    let len = 4usize;
    let mut idx = 0usize;
    if idx == 0 {
        idx = len.saturating_sub(1);
    } else {
        idx -= 1;
    }
    assert_eq!(idx, 3);

    idx = 3;
    idx += 1;
    if idx >= len {
        idx = 0;
    }
    assert_eq!(idx, 0);

    idx = 1;
    idx += 1;
    if idx >= len {
        idx = 0;
    }
    assert_eq!(idx, 2);
}

#[test]
fn setup_select_window_keeps_selected_item_visible() {
    assert_eq!(select_visible_window_start(0, 30, 8), 0);
    assert_eq!(select_visible_window_start(7, 30, 8), 0);
    assert_eq!(select_visible_window_start(8, 30, 8), 1);
    assert_eq!(select_visible_window_start(29, 30, 8), 22);
    assert_eq!(select_visible_window_start(29, 30, 80), 0);
}

#[test]
fn setup_select_navigation_supports_large_steps_and_edges() {
    assert_eq!(move_select_index(0, 30, SelectMove::Previous), 29);
    assert_eq!(move_select_index(29, 30, SelectMove::Next), 0);
    assert_eq!(move_select_index(2, 30, SelectMove::PageDown(8)), 10);
    assert_eq!(move_select_index(28, 30, SelectMove::PageDown(8)), 29);
    assert_eq!(move_select_index(6, 30, SelectMove::PageUp(8)), 0);
    assert_eq!(move_select_index(12, 30, SelectMove::First), 0);
    assert_eq!(move_select_index(12, 30, SelectMove::Last), 29);
}

#[test]
fn test_is_local_provider() {
    assert!(is_local_provider("ollama"));
    assert!(is_local_provider("lmstudio"));
    assert!(!is_local_provider("anthropic"));
    assert!(!is_local_provider("openai"));
}

#[test]
fn test_security_default_for_tier() {
    assert_eq!(default_security_index("newcomer"), 0);
    assert_eq!(default_security_index("familiar"), 1);
    assert_eq!(default_security_index("power_user"), 2);
    assert_eq!(default_security_index("expert"), 2);
    assert_eq!(default_security_index("unknown"), 1);
}

#[test]
fn test_tier_shows_optional_steps() {
    assert!(!tier_shows_step("newcomer", "model"));
    assert!(!tier_shows_step("newcomer", "data_dir"));
    assert!(!tier_shows_step("newcomer", "advanced_agents"));
    assert!(tier_shows_step("familiar", "model"));
    assert!(tier_shows_step("familiar", "data_dir"));
    assert!(tier_shows_step("familiar", "advanced_agents"));
    assert!(tier_shows_step("power_user", "model"));
    assert!(tier_shows_step("power_user", "advanced_agents"));
    assert!(tier_shows_step("expert", "model"));
    assert!(tier_shows_step("expert", "advanced_agents"));
}

#[test]
fn test_security_level_from_index() {
    assert_eq!(
        security_level_from_index(0),
        ("highest", "Approve risky actions")
    );
    assert_eq!(
        security_level_from_index(1),
        ("moderate", "Approve risky actions")
    );
    assert_eq!(
        security_level_from_index(2),
        ("lowest", "Approve destructive only")
    );
    assert_eq!(
        security_level_from_index(3),
        ("yolo", "Minimize interruptions")
    );
    assert_eq!(
        security_level_from_index(99),
        ("moderate", "Approve risky actions")
    );
}

#[test]
fn test_post_setup_action_from_index() {
    assert_eq!(post_setup_action_from_index(0), PostSetupAction::LaunchTui);
    assert_eq!(
        post_setup_action_from_index(1),
        PostSetupAction::LaunchElectron
    );
    assert_eq!(post_setup_action_from_index(2), PostSetupAction::NotNow);
}

#[test]
fn test_post_setup_choices_include_not_now() {
    let choices = post_setup_choices();
    assert_eq!(choices.len(), 3);
    assert_eq!(choices[0].0, "TUI");
    assert_eq!(choices[1].0, "Electron");
    assert_eq!(choices[2].0, "Not now");
}

#[test]
fn submit_key_recognizes_control_m_as_enter() {
    assert!(terminal_ui::is_submit_key(
        KeyCode::Char('m'),
        KeyModifiers::CONTROL
    ));
}

#[test]
fn submit_key_recognizes_standard_enter() {
    assert!(terminal_ui::is_submit_key(
        KeyCode::Enter,
        KeyModifiers::NONE
    ));
}

#[test]
fn submit_key_does_not_treat_plain_m_as_enter() {
    assert!(!terminal_ui::is_submit_key(
        KeyCode::Char('m'),
        KeyModifiers::NONE
    ));
}

#[test]
fn raw_mode_guard_requests_only_disambiguate_enhancement_flag() {
    let flags = wizard_keyboard_enhancement_flags();

    assert!(flags.contains(crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES));
    assert!(!flags.contains(crossterm::event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES));
    assert!(!flags.contains(crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS));
    assert!(!flags
        .contains(crossterm::event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES));
}

#[test]
fn selection_screen_enter_output_uses_alternate_screen() {
    let mut output = Vec::new();

    terminal_ui::write_selection_screen_enter(&mut output).expect("write enter sequence");

    let output = String::from_utf8(output).expect("ansi output should be utf8");
    assert!(output.contains("\x1b[?1049h"));
    assert!(output.contains("\x1b[2J"));
    assert!(output.contains("\x1b[1;1H"));
}

#[test]
fn actionable_key_event_kind_accepts_press() {
    assert!(terminal_ui::is_actionable_key_event_kind(
        crossterm::event::KeyEventKind::Press
    ));
}

#[test]
fn actionable_key_event_kind_accepts_repeat() {
    assert!(terminal_ui::is_actionable_key_event_kind(
        crossterm::event::KeyEventKind::Repeat
    ));
}

#[test]
fn actionable_key_event_kind_rejects_release() {
    assert!(!terminal_ui::is_actionable_key_event_kind(
        crossterm::event::KeyEventKind::Release
    ));
}

#[test]
fn submit_key_recognizes_carriage_return_char() {
    assert!(terminal_ui::is_submit_key(
        KeyCode::Char('\r'),
        KeyModifiers::NONE
    ));
}

#[test]
fn submit_key_recognizes_newline_char() {
    assert!(terminal_ui::is_submit_key(
        KeyCode::Char('\n'),
        KeyModifiers::NONE
    ));
}

#[test]
fn submit_key_recognizes_control_j_as_enter() {
    assert!(terminal_ui::is_submit_key(
        KeyCode::Char('j'),
        KeyModifiers::CONTROL
    ));
}

#[test]
fn submit_key_does_not_treat_plain_j_as_enter() {
    assert!(!terminal_ui::is_submit_key(
        KeyCode::Char('j'),
        KeyModifiers::NONE
    ));
}

#[test]
fn test_gateway_choice_items_include_whatsapp_and_skip() {
    let items = gateway_choice_items();
    assert_eq!(items.len(), 5);
    assert_eq!(items[3], ("WhatsApp", "whatsapp"));
    assert_eq!(items[4], ("Skip", ""));
}

#[test]
fn web_search_setup_maps_api_key_to_search_provider() {
    assert_eq!(
        web_search_provider_for_key("firecrawl_api_key"),
        Some("firecrawl")
    );
    assert_eq!(web_search_provider_for_key("exa_api_key"), Some("exa"));
    assert_eq!(
        web_search_provider_for_key("tavily_api_key"),
        Some("tavily")
    );
    assert_eq!(web_search_provider_for_key(""), None);
}

#[test]
fn web_search_setup_offers_duckduckgo_without_api_key() {
    let items = web_search_choice_items();
    assert!(items.contains(&("DuckDuckGo", "duckduckgo")));
    assert_eq!(
        web_search_provider_for_choice("duckduckgo"),
        Some("duckduckgo")
    );
    assert_eq!(web_search_api_key_prompt_for_choice("duckduckgo"), None);
}

#[test]
fn web_search_setup_offers_keenable_with_optional_api_key() {
    let items = web_search_choice_items();
    assert!(items.contains(&("Keenable", "keenable_api_key")));
    assert_eq!(
        web_search_provider_for_choice("keenable_api_key"),
        Some("keenable")
    );
    let prompt = web_search_api_key_prompt_for_choice("keenable_api_key").expect("prompt");
    assert!(prompt.contains("optional"));
    assert_eq!(items.last(), Some(&("Skip", "")));
}

#[test]
fn web_search_setup_offers_youcom_with_optional_api_key() {
    let items = web_search_choice_items();
    assert!(items.contains(&("You.com", "youcom_api_key")));
    assert_eq!(
        web_search_provider_for_choice("youcom_api_key"),
        Some("youcom")
    );
    let prompt = web_search_api_key_prompt_for_choice("youcom_api_key").expect("prompt");
    assert!(prompt.contains("optional"));
    assert_eq!(items.last(), Some(&("Skip", "")));
}

#[test]
fn test_whatsapp_timeout_choice_mapping() {
    let choices = whatsapp_timeout_choices();
    assert_eq!(choices.len(), 2);
    assert!(whatsapp_timeout_retry_selected(0));
    assert!(!whatsapp_timeout_retry_selected(1));
}

#[test]
fn whatsapp_setup_accepts_multiline_or_csv_contacts() {
    let parsed = parse_whatsapp_setup_allowlist("+48 123 456 789, 15551230000\n+44 20 7946 0958");

    assert_eq!(
        parsed,
        Some(vec![
            "48123456789".to_string(),
            "15551230000".to_string(),
            "442079460958".to_string(),
        ])
    );
}

#[test]
fn whatsapp_setup_rejects_empty_allowlist() {
    assert_eq!(
        parse_whatsapp_setup_allowlist("\n , invalid , device "),
        None
    );
    assert_eq!(parse_whatsapp_setup_allowlist("   \n  "), None);
}

#[test]
fn whatsapp_setup_cancellation_paths_stop_without_retry() {
    assert_eq!(
        resolve_whatsapp_allowlist_prompt(WhatsAppAllowlistPromptOutcome::Cancelled),
        WhatsAppAllowlistPromptResolution::Cancel
    );
    assert_eq!(
        resolve_whatsapp_allowlist_prompt(WhatsAppAllowlistPromptOutcome::EndOfInput),
        WhatsAppAllowlistPromptResolution::Cancel
    );
}

#[test]
fn whatsapp_setup_invalid_submission_requests_retry() {
    assert_eq!(
        resolve_whatsapp_allowlist_prompt(WhatsAppAllowlistPromptOutcome::Submitted(
            "\n , invalid , device ".to_string()
        )),
        WhatsAppAllowlistPromptResolution::Retry(
            "Enter at least one valid WhatsApp phone number before linking."
        )
    );
}

#[test]
fn raw_mode_line_output_uses_carriage_return_line_feed() {
    assert_eq!(
        format_raw_mode_line_output("██\n██"),
        "██\r\n██\r\n".to_string()
    );
    assert_eq!(
        format_raw_mode_line_output("status line"),
        "status line\r\n".to_string()
    );
}

#[test]
fn whatsapp_setup_persists_allowlist_as_string_before_gateway_enable() {
    let writes =
        whatsapp_gateway_config_writes("+48 123 456 789, 15551230000").expect("valid writes");

    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].key_path, "/gateway/whatsapp_allowed_contacts");
    assert_eq!(writes[0].value_json, "\"48123456789,15551230000\"");
    assert_eq!(writes[1].key_path, "/gateway/enabled");
    assert_eq!(writes[1].value_json, "true");
}

#[test]
fn wizard_declares_async_command_capability_on_connect() {
    let messages = wizard_startup_messages();
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        ClientMessage::AgentDeclareAsyncCommandCapability { capability } => {
            assert_eq!(capability.version, 1);
            assert!(capability.supports_operation_acceptance);
        }
        other => panic!("expected async command capability declaration, got {other:?}"),
    }
}

#[test]
fn wizard_ignores_async_command_capability_ack_messages() {
    let ignored = should_ignore_wizard_message(&DaemonMessage::AgentAsyncCommandCapabilityAck {
        capability: zorai_protocol::AsyncCommandCapability {
            version: 1,
            supports_operation_acceptance: true,
        },
    });
    assert!(ignored, "handshake ack should not surface to setup flows");

    let forwarded = should_ignore_wizard_message(&DaemonMessage::AgentProviderAuthStates {
        states_json: "[]".to_string(),
    });
    assert!(
        !forwarded,
        "real daemon payloads must still reach setup flows"
    );
}

#[test]
fn setup_probe_requires_configured_provider_and_model() {
    assert_eq!(
        setup_probe_from_config_json(r#"{"provider":"openai","model":""}"#),
        SetupProbe::NeedsSetup
    );
    assert_eq!(
        setup_probe_from_config_json(r#"{"provider":"","model":"gpt-5.4-mini"}"#),
        SetupProbe::NeedsSetup
    );
    assert_eq!(
        setup_probe_from_config_json(r#"{"provider":"openai","model":"gpt-5.4-mini"}"#),
        SetupProbe::Ready
    );
}

#[test]
fn provider_validation_terminal_response_ignores_operation_acceptance() {
    let response = parse_provider_validation_terminal_response(DaemonMessage::OperationAccepted {
        operation_id: "op-provider-validation-1".to_string(),
        kind: "provider_validation".to_string(),
        dedup: None,
        revision: 1,
    });
    assert!(response.is_none(), "operation acceptance is not terminal");
}

#[test]
fn provider_validation_terminal_response_extracts_result_payload() {
    let response =
        parse_provider_validation_terminal_response(DaemonMessage::AgentProviderValidation {
            operation_id: Some("op-provider-validation-1".to_string()),
            provider_id: "openai".to_string(),
            valid: false,
            error: Some("bad key".to_string()),
            models_json: None,
        })
        .expect("terminal response")
        .expect("successful parse");

    assert_eq!(response, (false, Some("bad key".to_string())));
}

#[test]
fn fetch_models_terminal_response_ignores_operation_acceptance() {
    let response = parse_fetch_models_terminal_response(DaemonMessage::OperationAccepted {
        operation_id: "op-fetch-models-1".to_string(),
        kind: "fetch_models".to_string(),
        dedup: None,
        revision: 1,
    });
    assert!(response.is_none(), "operation acceptance is not terminal");
}

#[test]
fn fetch_models_terminal_response_extracts_models_json() {
    let response = parse_fetch_models_terminal_response(DaemonMessage::AgentModelsResponse {
        operation_id: Some("op-fetch-models-1".to_string()),
        models_json: "[\"gpt-5.4\"]".to_string(),
    })
    .expect("terminal response")
    .expect("successful parse");

    assert_eq!(response, "[\"gpt-5.4\"]".to_string());
}

#[test]
fn setup_model_options_parse_remote_model_payloads_for_all_agents() {
    let models = super::flow::parse_setup_model_options(
        r#"[
            {
                "id": "openai/gpt-5.4",
                "name": "GPT 5.4",
                "pricing": {
                    "prompt": "0.0000025",
                    "completion": "0.00001"
                }
            }
        ]"#,
    );

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "openai/gpt-5.4");

    let items = super::flow::setup_model_select_items(&models);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "openai/gpt-5.4");
    assert_eq!(items[0].detail.as_deref(), Some("GPT 5.4"));
    assert_eq!(
        items[0].subtitle.as_deref(),
        Some("Prompt $2.50/M tok, completion $10.00/M tok")
    );
}

#[test]
fn setup_model_options_parse_legacy_string_payloads() {
    let models = super::flow::parse_setup_model_options(r#"["gpt-5.4-mini"]"#);

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "gpt-5.4-mini");

    let items = super::flow::setup_model_select_items(&models);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "gpt-5.4-mini");
    assert_eq!(items[0].detail, None);
    assert_eq!(items[0].subtitle, None);
}

#[test]
fn config_set_response_completes_on_operation_acceptance() {
    let response = parse_set_config_item_response(DaemonMessage::OperationAccepted {
        operation_id: "op-config-set-1".to_string(),
        kind: "config_set_item".to_string(),
        dedup: None,
        revision: 1,
    });
    assert!(
        response.is_some(),
        "operation acceptance should complete config writes"
    );
    response
        .expect("terminal response")
        .expect("successful config-set acknowledgement");
}

#[test]
fn setup_probe_marks_ready_when_provider_and_model_are_persisted() {
    assert_eq!(
        setup_probe_from_config_json(r#"{"provider":"openai","model":"gpt-5.4-mini"}"#),
        SetupProbe::Ready
    );
}

#[test]
fn setup_probe_requires_setup_without_provider() {
    assert_eq!(
        setup_probe_from_config_json(r#"{"provider":""}"#),
        SetupProbe::NeedsSetup
    );
    assert_eq!(
        setup_probe_from_config_json(r#"{"providers":{"openai":{}}}"#),
        SetupProbe::NeedsSetup
    );
}

#[test]
fn setup_probe_treats_invalid_config_as_needing_setup() {
    assert_eq!(
        setup_probe_from_config_json("{not-json"),
        SetupProbe::NeedsSetup
    );
}

#[test]
fn github_copilot_setup_prefers_browser_auth_over_api_key_prompt() {
    let provider = ProviderSelection {
        provider_id: "github-copilot".to_string(),
        provider_name: "GitHub Copilot".to_string(),
        base_url: "https://api.githubcopilot.com".to_string(),
        default_model: "gpt-4.1".to_string(),
        auth_source: "github_copilot".to_string(),
    };

    assert!(provider_uses_browser_auth(&provider));
}

#[test]
fn github_copilot_browser_auth_setup_still_runs_provider_validation() {
    let provider = ProviderSelection {
        provider_id: "github-copilot".to_string(),
        provider_name: "GitHub Copilot".to_string(),
        base_url: "https://api.githubcopilot.com".to_string(),
        default_model: "gpt-4.1".to_string(),
        auth_source: "github_copilot".to_string(),
    };

    assert!(should_validate_provider_after_setup(
        &provider,
        AuthSetupResult {
            auth_source: "github_copilot".to_string(),
            api_key_for_requests: String::new(),
            authenticated: true,
        }
    ));
}

#[test]
fn anthropic_setup_uses_api_key_prompt_and_validation() {
    let provider = ProviderSelection {
        provider_id: "anthropic".to_string(),
        provider_name: "Anthropic".to_string(),
        base_url: "https://api.anthropic.com".to_string(),
        default_model: "claude-opus-4-7".to_string(),
        auth_source: "api_key".to_string(),
    };

    assert!(!provider_uses_browser_auth(&provider));
    assert!(should_validate_provider_after_setup(
        &provider,
        AuthSetupResult {
            auth_source: "api_key".to_string(),
            api_key_for_requests: "sk-ant-api03-test".to_string(),
            authenticated: false,
        }
    ));
}

#[test]
fn setup_agent_model_hints_match_agent_roles() {
    assert_eq!(
        super::agents::setup_agent_model_hint("Svarog"),
        "Svarog is the main working fire. Prefer your strongest model."
    );
    assert_eq!(
        super::agents::setup_agent_model_hint("Rarog"),
        "Rarog should stay light, cheap, and responsive."
    );
    assert_eq!(
        super::agents::setup_agent_model_hint("WELES"),
        "Weles should stay strong enough for review and governance."
    );
}

#[test]
fn setup_agent_reasoning_hints_match_agent_roles() {
    assert_eq!(
        super::agents::setup_agent_reasoning_hint("Svarog"),
        "Svarog handles primary execution and longer reasoning chains."
    );
    assert_eq!(
        super::agents::setup_agent_reasoning_hint("Rarog"),
        "Non-reasoning is fine for Rarog; add more only if it clearly helps."
    );
    assert_eq!(
        super::agents::setup_agent_reasoning_hint("WELES"),
        "Weles does not need to be your top model, but avoid weak review setups."
    );
}

#[test]
fn github_cli_token_output_trims_trailing_newlines() {
    assert_eq!(
        parse_gh_cli_token_output(b"copilot-token\n"),
        Some("copilot-token".to_string())
    );
}

#[test]
fn github_cli_token_output_rejects_empty_stdout() {
    assert_eq!(parse_gh_cli_token_output(b"\n"), None);
}

#[test]
fn remote_model_pricing_subtitle_formats_prompt_and_completion_costs() {
    let model: super::types::RemoteModelOption = serde_json::from_value(serde_json::json!({
        "id": "openai/gpt-5.4",
        "pricing": {
            "prompt": "0.0000025",
            "completion": "0.00001"
        }
    }))
    .expect("parse remote model option");

    assert_eq!(
        super::flow::format_remote_model_pricing_subtitle(&model),
        Some("Prompt $2.50/M tok, completion $10.00/M tok".to_string())
    );
}

#[test]
fn remote_model_pricing_subtitle_returns_free_for_zero_prompt_and_completion() {
    let model: super::types::RemoteModelOption = serde_json::from_value(serde_json::json!({
        "id": "meta-llama/free",
        "pricing": {
            "prompt": "0",
            "completion": "0"
        }
    }))
    .expect("parse remote model option");

    assert_eq!(
        super::flow::format_remote_model_pricing_subtitle(&model),
        Some("free".to_string())
    );
}

#[test]
fn setup_compaction_choices_include_heuristic_weles_and_custom_llm() {
    let choices = super::agents::compaction_choice_items();

    assert_eq!(
        choices.iter().map(|(label, _)| *label).collect::<Vec<_>>(),
        vec!["Heuristic", "WELES", "Custom LLM"]
    );
}

#[test]
fn custom_llm_compaction_writes_provider_model_and_reasoning_effort() {
    let writes = super::agents::custom_llm_compaction_writes(
        "anthropic",
        "https://api.anthropic.com",
        "claude-opus-4-7",
        Some("sk-ant-test"),
        "api_key",
        Some("high"),
    );

    assert_eq!(
        writes,
        vec![
            super::types::ConfigWrite {
                key_path: "/auto_compact_context".to_string(),
                value_json: "true".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/strategy".to_string(),
                value_json: "\"custom_model\"".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/custom_model/provider".to_string(),
                value_json: "\"anthropic\"".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/custom_model/base_url".to_string(),
                value_json: "\"https://api.anthropic.com\"".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/custom_model/model".to_string(),
                value_json: "\"claude-opus-4-7\"".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/custom_model/api_key".to_string(),
                value_json: "\"sk-ant-test\"".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/custom_model/auth_source".to_string(),
                value_json: "\"api_key\"".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/custom_model/reasoning_effort".to_string(),
                value_json: "\"high\"".to_string(),
            },
        ]
    );
}

#[test]
fn weles_compaction_choice_yes_reuses_weles_provider_model_and_effort() {
    let writes = super::agents::weles_compaction_writes(
        super::agents::WelesCompactionChoice::Yes,
        "anthropic",
        "claude-opus-4-7",
        Some("high"),
    );

    assert_eq!(
        writes,
        vec![
            super::types::ConfigWrite {
                key_path: "/auto_compact_context".to_string(),
                value_json: "true".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/strategy".to_string(),
                value_json: "\"weles\"".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/weles/provider".to_string(),
                value_json: "\"anthropic\"".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/weles/model".to_string(),
                value_json: "\"claude-opus-4-7\"".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/weles/reasoning_effort".to_string(),
                value_json: "\"high\"".to_string(),
            },
        ]
    );
}

#[test]
fn weles_compaction_choice_default_heuristic_enables_auto_compaction_without_overriding_weles() {
    let writes = super::agents::weles_compaction_writes(
        super::agents::WelesCompactionChoice::NoUseDefault,
        "openai",
        "gpt-5.4-mini",
        Some("medium"),
    );

    assert_eq!(
        writes,
        vec![
            super::types::ConfigWrite {
                key_path: "/auto_compact_context".to_string(),
                value_json: "true".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/strategy".to_string(),
                value_json: "\"heuristic\"".to_string(),
            },
        ]
    );
}

#[test]
fn weles_compaction_choice_no_uses_default_heuristic() {
    let writes = super::agents::weles_compaction_writes(
        super::agents::WelesCompactionChoice::NoUseDefault,
        "openai",
        "gpt-5.4-mini",
        Some("none"),
    );

    assert_eq!(
        writes,
        vec![
            super::types::ConfigWrite {
                key_path: "/auto_compact_context".to_string(),
                value_json: "true".to_string(),
            },
            super::types::ConfigWrite {
                key_path: "/compaction/strategy".to_string(),
                value_json: "\"heuristic\"".to_string(),
            },
        ]
    );
}
