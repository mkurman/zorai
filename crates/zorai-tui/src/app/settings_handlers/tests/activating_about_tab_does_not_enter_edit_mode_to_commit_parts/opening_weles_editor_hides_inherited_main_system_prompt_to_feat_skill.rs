use super::super::make_model;
use super::*;
use crate::app::TuiModel;
use crate::state::settings::SettingsTab;
use crossterm::event::{KeyCode, KeyModifiers};
#[test]
fn opening_weles_editor_hides_inherited_main_system_prompt() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "system_prompt": "Main Svarog prompt",
        "builtin_sub_agents": {
            "weles": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-5.4-mini"
            }
        }
    }));
    model.subagents.entries = vec![crate::state::SubAgentEntry {
        claude_permission_mode: None,
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("governance".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Daemon-owned WELES registry entry".to_string()),
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: Some(serde_json::json!({
            "id": "weles_builtin",
            "name": "WELES",
            "provider": PROVIDER_ID_OPENAI,
            "model": "gpt-5.4-mini",
            "role": "governance",
            "system_prompt": "Main Svarog prompt",
            "enabled": true,
            "builtin": true,
            "immutable_identity": true,
            "disable_allowed": false,
            "delete_allowed": false,
            "protected_reason": "Daemon-owned WELES registry entry",
            "reasoning_effort": "medium",
            "created_at": 0
        })),
    }];

    model.open_subagent_editor_existing();

    assert_eq!(
        model
            .subagents
            .editor
            .as_ref()
            .map(|editor| editor.system_prompt.as_str()),
        Some("")
    );
}

pub fn focus_settings_field(model: &mut TuiModel, tab: SettingsTab, field_name: &str) {
    model.settings.reduce(SettingsAction::SwitchTab(tab));
    let count = model.settings.field_count_with_config(&model.config);
    for _ in 0..count {
        if model.settings.current_field_name_with_config(&model.config) == field_name {
            return;
        }
        model.settings.reduce(SettingsAction::NavigateField(1));
    }
    panic!("field {field_name} not found in {:?}", tab);
}

fn raw_json_string(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    current.as_str().map(str::to_string)
}

fn raw_json_bool(value: &serde_json::Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    current.as_bool()
}

#[test]
fn feature_toggle_fields_emit_expected_config_updates() {
    let cases = [
        (
            "feat_tier_override",
            "/tier/user_override",
            "\"familiar\"",
            Some((vec!["tier", "user_override"], "familiar")),
            None,
        ),
        (
            "feat_check_stale_todos",
            "/heartbeat_checks/stale_todos_enabled",
            "false",
            None,
            Some((vec!["heartbeat_checks", "stale_todos_enabled"], false)),
        ),
        (
            "feat_check_stuck_goals",
            "/heartbeat_checks/stuck_goals_enabled",
            "false",
            None,
            Some((vec!["heartbeat_checks", "stuck_goals_enabled"], false)),
        ),
        (
            "feat_check_unreplied_messages",
            "/heartbeat_checks/unreplied_messages_enabled",
            "false",
            None,
            Some((
                vec!["heartbeat_checks", "unreplied_messages_enabled"],
                false,
            )),
        ),
        (
            "feat_check_repo_changes",
            "/heartbeat_checks/repo_changes_enabled",
            "false",
            None,
            Some((vec!["heartbeat_checks", "repo_changes_enabled"], false)),
        ),
        (
            "feat_check_plugin_auth",
            "/heartbeat_checks/plugin_auth_enabled",
            "false",
            None,
            Some((vec!["heartbeat_checks", "plugin_auth_enabled"], false)),
        ),
        (
            "feat_heartbeat_enabled",
            "/heartbeat_enabled",
            "true",
            None,
            Some((vec!["heartbeat_enabled"], true)),
        ),
        (
            "feat_consolidation_enabled",
            "/consolidation/enabled",
            "false",
            None,
            Some((vec!["consolidation", "enabled"], false)),
        ),
        (
            "feat_skill_recommendation_enabled",
            "/skill_recommendation/enabled",
            "false",
            None,
            Some((vec!["skill_recommendation", "enabled"], false)),
        ),
        (
            "feat_skill_background_community_search",
            "/skill_recommendation/background_community_search",
            "false",
            None,
            Some((
                vec!["skill_recommendation", "background_community_search"],
                false,
            )),
        ),
        (
            "feat_audio_stt_enabled",
            "/audio/stt/enabled",
            "true",
            None,
            Some((vec!["audio", "stt", "enabled"], true)),
        ),
        (
            "feat_audio_tts_enabled",
            "/audio/tts/enabled",
            "true",
            None,
            Some((vec!["audio", "tts", "enabled"], true)),
        ),
    ];

    for (field, expected_key_path, expected_value_json, expected_string, expected_bool) in cases {
        let (mut model, mut daemon_rx) = make_model();
        model.config.agent_config_raw = Some(serde_json::json!({}));
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        focus_settings_field(
            &mut model,
            crate::state::settings_registry::settings_page_tab_for_field(field)
                .expect("field is on a settings page"),
            field,
        );

        let quit = model.handle_key_modal(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit, "settings modal should remain open for {field}");

        match daemon_rx.try_recv() {
            Ok(DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            }) => {
                assert_eq!(key_path, expected_key_path, "wrong key path for {field}");
                assert_eq!(
                    value_json, expected_value_json,
                    "wrong serialized value for {field}"
                );
            }
            other => panic!("expected SetConfigItem for {field}, got {other:?}"),
        }

        let raw = model
            .config
            .agent_config_raw
            .as_ref()
            .expect("feature toggles should keep raw config");
        if let Some((path, expected)) = expected_string {
            assert_eq!(raw_json_string(raw, &path), Some(expected.to_string()));
        }
        if let Some((path, expected)) = expected_bool {
            assert_eq!(raw_json_bool(raw, &path), Some(expected));
        }
    }
}

#[test]
fn feature_edit_fields_start_with_saved_values_and_submit_expected_updates() {
    let cases = [
        (
            "feat_heartbeat_interval",
            serde_json::json!({"heartbeat_interval_mins": 120}),
            "120",
            "/heartbeat_interval_mins",
            "120",
        ),
        (
            "feat_heartbeat_cron",
            serde_json::json!({"heartbeat_cron": "*/30 * * * *"}),
            "*/30 * * * *",
            "/heartbeat_cron",
            "\"*/30 * * * *\"",
        ),
        (
            "feat_heartbeat_quiet_start",
            serde_json::json!({"quiet_hours_start": 21}),
            "21",
            "/quiet_hours_start",
            "21",
        ),
        (
            "feat_heartbeat_quiet_end",
            serde_json::json!({"quiet_hours_end": 6}),
            "6",
            "/quiet_hours_end",
            "6",
        ),
        (
            "feat_decay_half_life_hours",
            serde_json::json!({"consolidation": {"memory_decay_half_life_hours": 72.0}}),
            "72",
            "/consolidation/memory_decay_half_life_hours",
            "72",
        ),
        (
            "feat_heuristic_promotion_threshold",
            serde_json::json!({"consolidation": {"heuristic_promotion_threshold": 9}}),
            "9",
            "/consolidation/heuristic_promotion_threshold",
            "9",
        ),
        (
            "feat_skill_community_preapprove_timeout_secs",
            serde_json::json!({"skill_recommendation": {"community_preapprove_timeout_secs": 45}}),
            "45",
            "/skill_recommendation/community_preapprove_timeout_secs",
            "45",
        ),
        (
            "feat_skill_suggest_global_enable_after_approvals",
            serde_json::json!({"skill_recommendation": {"suggest_global_enable_after_approvals": 6}}),
            "6",
            "/skill_recommendation/suggest_global_enable_after_approvals",
            "6",
        ),
    ];

    for (field, raw, expected_buffer, expected_key_path, expected_value_json) in cases {
        let (mut model, mut daemon_rx) = make_model();
        model.config.agent_config_raw = Some(raw);
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        focus_settings_field(
            &mut model,
            crate::state::settings_registry::settings_page_tab_for_field(field)
                .expect("field is on a settings page"),
            field,
        );

        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(
            !quit,
            "settings modal should stay open while starting edit for {field}"
        );
        assert_eq!(model.settings.editing_field(), Some(field));
        assert_eq!(model.settings.edit_buffer(), expected_buffer);

        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(
            !quit,
            "settings modal should stay open while committing edit for {field}"
        );
        assert_eq!(model.settings.editing_field(), None);

        match daemon_rx.try_recv() {
            Ok(DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            }) => {
                assert_eq!(key_path, expected_key_path, "wrong key path for {field}");
                assert_eq!(
                    value_json, expected_value_json,
                    "wrong serialized value for {field}"
                );
            }
            other => panic!("expected SetConfigItem for {field}, got {other:?}"),
        }
    }
}

#[test]
fn feat_skill_recommendation_toggles_write_new_daemon_paths() {
    let cases = [
        (
            "feat_skill_recommendation_enabled",
            "/skill_recommendation/enabled",
            "false",
            vec!["skill_recommendation", "enabled"],
        ),
        (
            "feat_skill_background_community_search",
            "/skill_recommendation/background_community_search",
            "false",
            vec!["skill_recommendation", "background_community_search"],
        ),
    ];

    for (field, expected_key_path, expected_value_json, raw_path) in cases {
        let (mut model, mut daemon_rx) = make_model();
        model.config.agent_config_raw = Some(serde_json::json!({}));
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        focus_settings_field(
            &mut model,
            crate::state::settings_registry::settings_page_tab_for_field(field)
                .expect("field is on a settings page"),
            field,
        );

        let quit = model.handle_key_modal(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit, "settings modal should remain open for {field}");

        match daemon_rx.try_recv() {
            Ok(DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            }) => {
                assert_eq!(key_path, expected_key_path, "wrong key path for {field}");
                assert_eq!(
                    value_json, expected_value_json,
                    "wrong serialized value for {field}"
                );
            }
            other => panic!("expected SetConfigItem for {field}, got {other:?}"),
        }

        let raw = model
            .config
            .agent_config_raw
            .as_ref()
            .expect("feature toggles should keep raw config");
        assert_eq!(raw_json_bool(raw, &raw_path), Some(false));
    }
}

#[test]
fn feat_skill_recommendation_numeric_fields_write_new_daemon_paths() {
    let cases = [
        (
            "feat_skill_community_preapprove_timeout_secs",
            serde_json::json!({"skill_recommendation": {"community_preapprove_timeout_secs": 45}}),
            "45",
            "/skill_recommendation/community_preapprove_timeout_secs",
            "45",
        ),
        (
            "feat_skill_suggest_global_enable_after_approvals",
            serde_json::json!({"skill_recommendation": {"suggest_global_enable_after_approvals": 6}}),
            "6",
            "/skill_recommendation/suggest_global_enable_after_approvals",
            "6",
        ),
    ];

    for (field, raw, expected_buffer, expected_key_path, expected_value_json) in cases {
        let (mut model, mut daemon_rx) = make_model();
        model.config.agent_config_raw = Some(raw);
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        focus_settings_field(
            &mut model,
            crate::state::settings_registry::settings_page_tab_for_field(field)
                .expect("field is on a settings page"),
            field,
        );

        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(
            !quit,
            "settings modal should stay open while starting edit for {field}"
        );
        assert_eq!(model.settings.editing_field(), Some(field));
        assert_eq!(model.settings.edit_buffer(), expected_buffer);

        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(
            !quit,
            "settings modal should stay open while committing edit for {field}"
        );
        assert_eq!(model.settings.editing_field(), None);

        match daemon_rx.try_recv() {
            Ok(DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            }) => {
                assert_eq!(key_path, expected_key_path, "wrong key path for {field}");
                assert_eq!(
                    value_json, expected_value_json,
                    "wrong serialized value for {field}"
                );
            }
            other => panic!("expected SetConfigItem for {field}, got {other:?}"),
        }
    }
}

#[test]
fn quiet_hours_reject_values_the_daemon_cannot_store_and_clear_on_empty() {
    for (typed, expected) in [("25", None), ("", Some("null")), ("22:00", Some("22"))] {
        let (mut model, mut daemon_rx) = make_model();
        model.connected = true;
        model.agent_config_loaded = true;
        model.config.agent_config_raw = Some(serde_json::json!({}));
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        focus_settings_field(
            &mut model,
            SettingsTab::Heartbeat,
            "feat_heartbeat_quiet_start",
        );
        model
            .settings
            .start_editing("feat_heartbeat_quiet_start", typed);
        model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );

        let mut quiet_writes = Vec::new();
        while let Ok(command) = daemon_rx.try_recv() {
            if let DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            } = command
            {
                if key_path == "/quiet_hours_start" {
                    quiet_writes.push(value_json);
                }
            }
        }
        match expected {
            Some(value) => assert_eq!(quiet_writes, vec![value.to_string()], "typed {typed:?}"),
            None => {
                assert!(quiet_writes.is_empty(), "typed {typed:?}: {quiet_writes:?}");
                assert!(model.status_line.contains("0-23"), "{}", model.status_line);
            }
        }
    }
}

fn connected_settings_model() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (mut model, daemon_rx) = make_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.agent_config_raw = Some(serde_json::json!({}));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    (model, daemon_rx)
}

fn drain_config_writes(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Vec<(String, String)> {
    let mut writes = Vec::new();
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::SetConfigItem {
            key_path,
            value_json,
        } = command
        {
            writes.push((key_path, value_json));
        }
    }
    writes
}

#[test]
fn saving_the_system_prompt_reaches_the_daemon() {
    let (mut model, mut daemon_rx) = connected_settings_model();
    focus_settings_field(&mut model, SettingsTab::Agent, "system_prompt");
    model.settings.start_editing("system_prompt", "Be terse.");
    model.handle_key_modal(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
        modal::ModalKind::Settings,
    );

    assert!(
        drain_config_writes(&mut daemon_rx)
            .contains(&("/system_prompt".to_string(), "\"Be terse.\"".to_string())),
        "the prompt edit must be sent, not only stored locally"
    );
}

#[test]
fn editing_repo_monitor_include_dirs_saves_them_to_the_workspace() {
    let (mut model, mut daemon_rx) = connected_settings_model();
    model.config.workspace_repo_monitor_enabled = true;
    focus_settings_field(
        &mut model,
        SettingsTab::Storage,
        "workspace_repo_monitor_include_dirs",
    );
    model.settings.start_editing(
        "workspace_repo_monitor_include_dirs",
        "crates\nfrontend/src",
    );
    model.handle_key_modal(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
        modal::ModalKind::Settings,
    );

    let mut saved = None;
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::SetWorkspaceRepoMonitor {
            repo_monitor_enabled,
            repo_monitor_include_dirs,
            ..
        } = command
        {
            saved = Some((repo_monitor_enabled, repo_monitor_include_dirs));
        }
    }
    assert_eq!(
        saved,
        Some((true, vec!["crates".to_string(), "frontend/src".to_string()]))
    );
}

#[test]
fn saving_settings_keeps_each_inactive_providers_own_reasoning_effort() {
    let (mut model, _daemon_rx) = connected_settings_model();
    model.config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    model.config.reasoning_effort = "high".to_string();
    model.config.agent_config_raw = Some(serde_json::json!({
        "providers": { "anthropic": { "model": "claude-x", "reasoning_effort": "low" } }
    }));

    assert_eq!(
        model.provider_wire_config_value("anthropic")["reasoning_effort"],
        "low"
    );
    assert_eq!(
        model.provider_wire_config_value(zorai_shared::providers::PROVIDER_ID_OPENAI)
            ["reasoning_effort"],
        "high"
    );
}

fn compaction_strategy_showing(field: &str) -> &'static str {
    if field.starts_with("compaction_weles_") {
        "weles"
    } else if field.starts_with("compaction_custom_") {
        "custom_model"
    } else {
        "heuristic"
    }
}

#[test]
fn every_field_on_the_new_settings_pages_reacts_to_its_key() {
    use crate::state::settings_registry::{settings_page_spec, SettingsFieldKind};

    for tab in SettingsTab::all()
        .iter()
        .copied()
        .filter(|tab| settings_page_spec(*tab).is_some())
    {
        let page = settings_page_spec(tab).expect("page");
        for (provider, strategy) in [
            ("duckduckgo", "weles"),
            ("firecrawl", "custom_model"),
            ("exa", "heuristic"),
            ("tavily", "heuristic"),
        ] {
            let mut visible_config = crate::state::config::ConfigState::new();
            visible_config.search_provider = provider.to_string();
            visible_config.compaction_strategy = strategy.to_string();
            for spec in page.visible_fields(&visible_config) {
                let (mut model, mut daemon_rx) = connected_settings_model();
                model.config.search_provider = provider.to_string();
                model.config.compaction_strategy =
                    compaction_strategy_showing(spec.name).to_string();
                if spec.name == "code_review_agent" {
                    model.subagents.entries.push(reviewer_subagent("reviewer"));
                }
                if spec.name == "workspace_repo_monitor_enabled" {
                    model.config.workspace_repo_monitor_include_dirs = "crates".to_string();
                }
                if spec.name == "compaction_custom_api_transport" {
                    model.config.compaction_custom_provider = "custom".to_string();
                    model.config.compaction_custom_model = "local-model".to_string();
                    model.config.compaction_custom_api_transport = "chat_completions".to_string();
                }
                focus_settings_field(&mut model, tab, spec.name);
                let before = (spec.value)(&model.config);
                let status_before = model.status_line.clone();
                let key = if spec.kind == SettingsFieldKind::Toggle {
                    KeyCode::Char(' ')
                } else {
                    KeyCode::Enter
                };
                model.handle_key_modal(key, KeyModifiers::NONE, modal::ModalKind::Settings);
                match spec.kind {
                    SettingsFieldKind::Toggle | SettingsFieldKind::Choice => assert_ne!(
                        (spec.value)(&model.config),
                        before,
                        "{tab:?}/{} did not change when activated",
                        spec.name
                    ),
                    SettingsFieldKind::Text
                    | SettingsFieldKind::Secret
                    | SettingsFieldKind::Number
                    | SettingsFieldKind::Textarea => assert_eq!(
                        model.settings.editing_field(),
                        Some(spec.name),
                        "{tab:?}/{} did not open an editor",
                        spec.name
                    ),
                    SettingsFieldKind::Picker => assert!(
                        model.modal.top() != Some(modal::ModalKind::Settings)
                            || model.settings.editing_field() == Some(spec.name),
                        "{tab:?}/{} did not open a picker or editor",
                        spec.name
                    ),
                    SettingsFieldKind::Action => assert!(
                        daemon_rx.try_recv().is_ok()
                            || model.modal.top() != Some(modal::ModalKind::Settings)
                            || model.status_line != status_before,
                        "{tab:?}/{} did nothing when run",
                        spec.name
                    ),
                }
            }
        }
    }
}

fn reviewer_subagent(id: &str) -> crate::state::SubAgentEntry {
    crate::state::SubAgentEntry {
        claude_permission_mode: None,
        id: id.to_string(),
        name: id.to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("code_review".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        api_transport: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: None,
    }
}

fn finish_assistant_turn(model: &mut TuiModel, thread_id: &str, content: &str) {
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: thread_id.to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: content.to_string(),
            timestamp: 1,
            ..Default::default()
        },
    });
    model.handle_done_event(
        thread_id.to_string(),
        0,
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
}

fn tts_requests(daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>) -> usize {
    let mut count = 0;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::TextToSpeech { .. }) {
            count += 1;
        }
    }
    count
}

#[test]
fn auto_speak_reads_a_finished_reply_only_when_enabled_and_only_for_the_visible_thread() {
    for (auto_speak, visible, expected) in [(true, true, 1), (false, true, 0), (true, false, 0)] {
        let (mut model, mut daemon_rx) = make_model();
        model.config.agent_config_raw = Some(serde_json::json!({
            "audio": { "tts": { "enabled": true, "auto_speak": auto_speak } }
        }));
        for thread in ["thread-visible", "thread-background"] {
            model.chat.reduce(chat::ChatAction::ThreadCreated {
                thread_id: thread.to_string(),
                title: thread.to_string(),
            });
        }
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-visible".to_string()));
        let finished = if visible {
            "thread-visible"
        } else {
            "thread-background"
        };
        finish_assistant_turn(&mut model, finished, "Done, all tests pass.");
        assert_eq!(
            tts_requests(&mut daemon_rx),
            expected,
            "auto_speak={auto_speak} visible={visible}"
        );
    }
}

#[test]
fn review_agent_cycles_through_this_threads_agent_and_enabled_subagents() {
    let (mut model, mut daemon_rx) = connected_settings_model();
    model.subagents.entries.push(reviewer_subagent("reviewer"));
    let mut disabled = reviewer_subagent("disabled-reviewer");
    disabled.enabled = false;
    model.subagents.entries.push(disabled);
    focus_settings_field(&mut model, SettingsTab::Workflows, "code_review_agent");

    let mut picked = Vec::new();
    for _ in 0..3 {
        model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        picked.extend(
            drain_config_writes(&mut daemon_rx)
                .into_iter()
                .filter(|(path, _)| path == "/code_review_agent")
                .map(|(_, value)| value),
        );
    }
    assert_eq!(picked, vec!["\"reviewer\"", "\"thread\"", "\"reviewer\""]);
}

#[test]
fn commits_per_review_rejects_values_outside_one_to_fifty() {
    for (typed, expected) in [("12", Some("12")), ("0", None), ("51", None), ("x", None)] {
        let (mut model, mut daemon_rx) = connected_settings_model();
        focus_settings_field(
            &mut model,
            SettingsTab::Workflows,
            "code_review_commit_window",
        );
        model
            .settings
            .start_editing("code_review_commit_window", typed);
        model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        let writes = drain_config_writes(&mut daemon_rx)
            .into_iter()
            .filter(|(path, _)| path == "/code_review_commit_window")
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        match expected {
            Some(value) => assert_eq!(writes, vec![value.to_string()], "typed {typed:?}"),
            None => {
                assert!(writes.is_empty(), "typed {typed:?}: {writes:?}");
                assert!(
                    model.status_line.contains("1 to 50"),
                    "{}",
                    model.status_line
                );
            }
        }
    }
}
