use super::whatsapp_modal_esc_sends_stop_and_closes_to_clicking_rendered_settings::*;
use crate::app::*;
#[test]
fn settings_modal_mouse_wheel_scrolls_overflowing_content() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 16;
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Tools));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("settings modal should expose its overlay area");
    let before = render_screen(&mut model).join("\n");
    assert!(
        !before.contains("Inspect generated tools"),
        "expected overflowing settings content to be clipped before scrolling"
    );

    for _ in 0..8 {
        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: overlay_area.x.saturating_add(30),
            row: overlay_area.y.saturating_add(4),
            modifiers: KeyModifiers::NONE,
        });
    }

    let after = render_screen(&mut model).join("\n");
    assert!(
        after.contains("Inspect generated tools"),
        "expected mouse wheel scrolling to reveal lower settings rows"
    );
}

#[test]
fn settings_modal_keyboard_navigation_scrolls_selected_field_into_view() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 16;
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Tools));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let before = render_screen(&mut model).join("\n");
    assert!(
        !before.contains("Inspect generated tools"),
        "expected overflowing settings content to be clipped before keyboard navigation"
    );

    let target_field = model.settings_field_count().saturating_sub(1);
    for _ in 0..target_field {
        let quit = model.handle_key_modal(
            KeyCode::Down,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit);
    }

    assert_eq!(model.settings.field_cursor(), target_field);
    assert_eq!(
        model.settings.current_field_name(),
        "generated_tools_inspect"
    );
    assert!(
        model.settings_modal_scroll > 0,
        "expected keyboard navigation to advance the settings scroll offset"
    );

    let after = render_screen(&mut model).join("\n");
    assert!(
        after.contains("Inspect generated tools"),
        "expected keyboard navigation to reveal the selected lower settings row"
    );
}

#[test]
fn settings_modal_auth_keyboard_navigation_scrolls_selected_provider_into_view() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 16;
    model.auth.loaded = true;
    model.auth.entries = (0..12)
        .map(|i| crate::state::auth::ProviderAuthEntry {
            provider_id: format!("provider-{i}"),
            provider_name: format!("Provider {i}"),
            authenticated: i % 2 == 0,
            auth_source: "api_key".to_string(),
            model: String::new(),
        })
        .collect();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Auth));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let before = render_screen(&mut model).join("\n");
    assert!(
        !before.contains("Provider 10"),
        "expected lower auth rows to be clipped before keyboard navigation"
    );

    for _ in 0..10 {
        let quit = model.handle_key_modal(
            KeyCode::Down,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit);
    }

    assert_eq!(model.auth.selected, 10);
    assert!(
        model.settings_modal_scroll > 0,
        "expected auth keyboard navigation to advance the settings scroll offset"
    );

    let after = render_screen(&mut model).join("\n");
    assert!(
        after.contains("Provider 10"),
        "expected auth keyboard navigation to reveal the selected provider row"
    );
}

#[test]
fn settings_modal_keyboard_navigation_scrolls_lower_audio_fields_into_view() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 16;
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Media));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let before = render_screen(&mut model).join("\n");
    assert!(
        !before.contains("Voice "),
        "expected lower Audio & Images rows to be clipped before keyboard navigation"
    );

    for _ in 0..7 {
        let quit = model.handle_key_modal(
            KeyCode::Down,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit);
    }

    assert_eq!(model.settings.field_cursor(), 7);
    assert!(
        model.settings_modal_scroll > 0,
        "expected keyboard navigation to advance the settings scroll offset"
    );

    let after = render_screen(&mut model).join("\n");
    assert!(
        after.contains("Voice "),
        "expected keyboard navigation to reveal the selected audio field"
    );
}

#[test]
fn slash_status_opens_loading_modal_and_requests_status_without_sending_chat() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/status");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Status));
    assert!(model.status_modal_loading);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestAgentStatus) => {}
        other => panic!("expected status request, got {:?}", other),
    }
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn slash_statistics_opens_loading_modal_and_requests_all_time_statistics() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/statistics");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Statistics));
    assert!(model.statistics_modal_loading);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestAgentStatistics { window }) => {
            assert_eq!(window, zorai_protocol::AgentStatisticsWindow::All);
        }
        other => panic!("expected statistics request, got {:?}", other),
    }
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn slash_compact_requests_forced_compaction_for_active_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Compaction Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.input.set_text("/compact");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::ForceCompact { thread_id }) => {
            assert_eq!(thread_id, "thread-1");
        }
        other => panic!("expected force-compaction request, got {:?}", other),
    }
    assert!(daemon_rx.try_recv().is_err());
    // Why this matters: without an inline activity hint the user sees a "dead"
    // TUI for the entire daemon round-trip. The render_status_bar widget
    // ignores status_line entirely, so the only visible feedback channel is
    // the per-thread agent_activity spinner in the input placeholder.
    assert_eq!(
        model.thread_agent_activity.get("thread-1").map(String::as_str),
        Some("compacting"),
        "/compact must surface 'compacting' on the per-thread agent activity so the spinner is visible while the daemon works"
    );
}

#[test]
fn slash_compact_without_active_thread_warns_to_start_or_load_thread_first() {
    // Why this matters: the user explicitly asked for this wording. The previous
    // status-line-only message was invisible (status_line is dead in the
    // status bar widget); the input notice surfaces it where the user is
    // already looking.
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/compact");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert!(
        daemon_rx.try_recv().is_err(),
        "no daemon request should be sent when no thread is open"
    );
    assert_eq!(
        model
            .input_notice
            .as_ref()
            .map(|notice| notice.text.as_str()),
        Some("Start or load thread first"),
        "no-thread case must show the explicit guard text as an input notice"
    );
}

#[test]
fn slash_prompt_opens_loading_modal_and_requests_main_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/prompt");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::PromptViewer));
    assert!(model.prompt_modal_loading);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestPromptInspection { agent_id }) => {
            assert!(agent_id.is_none());
        }
        other => panic!("expected prompt inspection request, got {:?}", other),
    }
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn slash_prompt_weles_requests_explicit_agent_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/prompt weles");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestPromptInspection { agent_id }) => {
            assert_eq!(agent_id.as_deref(), Some("weles"));
        }
        other => panic!(
            "expected explicit prompt inspection request, got {:?}",
            other
        ),
    }
}

#[test]
fn slash_participants_opens_modal_with_thread_participant_sections() {
    let (mut model, _daemon_rx) = make_model();
    model.connected = true;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Participant Thread".to_string(),
        agent_name: Some("Svarog".to_string()),
        thread_participants: vec![
            crate::wire::ThreadParticipantState {
                agent_id: "weles".to_string(),
                agent_name: "Weles".to_string(),
                instruction: "verify claims".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 2,
                deactivated_at: None,
                last_contribution_at: Some(3),
                always_auto_response: false,
            },
            crate::wire::ThreadParticipantState {
                agent_id: "rarog".to_string(),
                agent_name: "Rarog".to_string(),
                instruction: "watch approvals".to_string(),
                status: "inactive".to_string(),
                created_at: 1,
                updated_at: 2,
                deactivated_at: Some(4),
                last_contribution_at: None,
                always_auto_response: false,
            },
        ],
        queued_participant_suggestions: vec![crate::wire::ThreadParticipantSuggestion {
            id: "sugg-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check the final answer".to_string(),
            suggestion_kind: "prepared_message".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 5,
            updated_at: 5,
            auto_send_at: None,
            source_message_timestamp: None,
            error: None,
        }],
        ..Default::default()
    })));
    model.input.set_text("/participants");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::ThreadParticipants)
    );
    let body = model.thread_participants_modal_body();
    assert!(
        body.contains("+ Add participant"),
        "missing add participant action: {body}"
    );
    assert!(
        body.contains("Weles") && body.contains("[active]"),
        "missing active participant: {body}"
    );
    assert!(
        body.contains("Rarog") && body.contains("[inactive]"),
        "missing inactive participant: {body}"
    );
    assert!(
        body.contains("Queued Suggestions"),
        "missing suggestion section: {body}"
    );
    assert!(
        body.contains("check the final answer"),
        "missing queued suggestion: {body}"
    );
}

#[test]
fn slash_notifications_opens_notifications_modal() {
    let (mut model, _daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/notifications");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Notifications));
}

#[test]
fn ctrl_n_opens_notifications_modal() {
    let (mut model, _daemon_rx) = make_model();

    let quit = model.handle_key(KeyCode::Char('n'), KeyModifiers::CONTROL);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Notifications));
}

#[test]
fn slash_approvals_opens_approval_center_modal() {
    let (mut model, _daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/approvals");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ApprovalCenter));
}

#[test]
fn prompt_viewer_down_scrolls_prompt_body() {
    let (mut model, _daemon_rx) = make_model();
    model.prompt_modal_snapshot = Some(crate::client::AgentPromptInspectionVm {
        agent_id: "swarog".to_string(),
        agent_name: "Svarog".to_string(),
        provider_id: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        sections: vec![crate::client::AgentPromptInspectionSectionVm {
            id: "base_prompt".to_string(),
            title: "Base Prompt".to_string(),
            content: (0..220)
                .map(|idx| format!("token-{idx}"))
                .collect::<Vec<_>>()
                .join(" "),
        }],
        final_prompt: (0..320)
            .map(|idx| format!("final-token-{idx}"))
            .collect::<Vec<_>>()
            .join(" "),
        tools: Vec::new(),
    });
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::PromptViewer));
    model.width = 120;
    model.height = 40;

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::PromptViewer,
    );

    assert!(!quit);
    assert_eq!(model.prompt_modal_scroll, 1);
}

#[test]
fn status_viewer_down_scrolls_status_body() {
    let (mut model, _daemon_rx) = make_model();
    model.status_modal_snapshot = Some(crate::client::AgentStatusSnapshotVm {
        tier: "mission_control".to_string(),
        activity: "waiting_for_operator".to_string(),
        active_thread_id: Some("thread-1".to_string()),
        active_goal_run_id: None,
        active_goal_run_title: Some("Close release gap".to_string()),
        provider_health_json: r#"{"openai":{"can_execute":true,"trip_count":0}}"#.to_string(),
        gateway_statuses_json: r#"{"slack":{"status":"connected"}}"#.to_string(),
        recent_actions_json: serde_json::to_string(
            &(0..40)
                .map(|idx| {
                    serde_json::json!({
                        "action_type": format!("tool_{idx}"),
                        "summary": format!("summary {idx}"),
                        "timestamp": 1712345678_u64 + idx,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    });
    model.status_modal_diagnostics_json = Some(
        serde_json::json!({
            "aline": {
                "available": true,
                "watcher_state": "running",
                "imported_count": 1,
                "generated_count": 1,
            }
        })
        .to_string(),
    );
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Status));
    model.width = 120;
    model.height = 40;

    let quit = model.handle_key_modal(KeyCode::Down, KeyModifiers::NONE, modal::ModalKind::Status);

    assert!(!quit);
    assert_eq!(model.status_modal_scroll, 1);
}

#[test]
fn help_viewer_down_scrolls_help_body() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Help));
    model.width = 80;
    model.height = 20;

    let quit = model.handle_key_modal(KeyCode::Down, KeyModifiers::NONE, modal::ModalKind::Help);

    assert!(!quit);
    assert_eq!(model.help_modal_scroll, 1);
}

#[test]
fn settings_modal_auth_search_filters_providers_and_acts_on_the_filtered_row() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 30;
    model.auth.loaded = true;
    model.auth.entries = [("anthropic", "Anthropic"), ("openai", "OpenAI"), ("openrouter", "OpenRouter")]
        .into_iter()
        .map(|(id, name)| crate::state::auth::ProviderAuthEntry {
            provider_id: id.to_string(),
            provider_name: name.to_string(),
            authenticated: false,
            auth_source: "api_key".to_string(),
            model: String::new(),
        })
        .collect();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Auth));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    for code in [KeyCode::Char('/')]
        .into_iter()
        .chain("router".chars().map(KeyCode::Char))
        .chain([KeyCode::Enter, KeyCode::Enter, KeyCode::Enter])
    {
        assert!(!model.handle_key_modal(code, KeyModifiers::NONE, modal::ModalKind::Settings));
    }

    let screen = render_screen(&mut model).join("\n");
    assert!(screen.contains("Search: router"));
    assert!(!screen.contains("Anthropic"), "non-matching providers must be hidden");
    assert_eq!(
        model.auth.login_target.as_deref(),
        Some("openrouter"),
        "the selection indexes the filtered list, so the action must target the visible provider, not the first unfiltered one"
    );
}
