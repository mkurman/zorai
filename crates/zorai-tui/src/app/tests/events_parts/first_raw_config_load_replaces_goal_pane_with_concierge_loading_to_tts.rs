use super::idle_tick_does_not_request_redraw_to_first_raw_config_load_triggers::*;
use crate::app::*;
use tokio::sync::mpsc::unbounded_channel;
use zorai_shared::providers::*;
#[test]
fn first_raw_config_load_replaces_goal_pane_with_concierge_loading() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));

    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "welcome request should immediately leave the goal pane so the loading hero can render"
    );
    assert_eq!(
        model.chat.active_thread_id(),
        None,
        "welcome request should clear the active thread until concierge content arrives"
    );
    assert!(
        model.concierge.loading,
        "welcome request should start concierge loading before the daemon responds"
    );
    assert!(
        model.should_show_concierge_hero_loading(),
        "goal panes should not block the concierge loading hero after the request is sent"
    );

    let mut saw_welcome = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::RequestConciergeWelcome) {
            saw_welcome = true;
            break;
        }
    }
    assert!(saw_welcome, "expected concierge welcome request");
}

#[test]
fn reconnect_config_load_restores_last_thread_instead_of_requesting_concierge() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_reconnecting_event(3);
    model.handle_connected_event();
    while daemon_rx.try_recv().is_ok() {}

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));

    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "reconnect restore should return to the conversation pane"
    );
    assert_eq!(
        model.chat.active_thread_id(),
        Some("thread-1"),
        "reconnect restore should keep the last visible thread selected"
    );

    let mut saw_welcome = false;
    let mut saw_thread_request = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::RequestConciergeWelcome => saw_welcome = true,
            DaemonCommand::RequestThread { thread_id, .. } if thread_id == "thread-1" => {
                saw_thread_request = true
            }
            _ => {}
        }
    }

    assert!(
        !saw_welcome,
        "reconnect restore should not discard the visible thread for concierge welcome"
    );
    assert!(
        saw_thread_request,
        "reconnect restore should request an authoritative reload for the last visible thread"
    );
}

#[test]
fn reconnect_config_load_still_requests_operator_profile_autostart_summary() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_reconnecting_event(3);
    model.handle_connected_event();
    while daemon_rx.try_recv().is_ok() {}

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));

    let mut saw_summary = false;
    let mut saw_welcome = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::GetOperatorProfileSummary => saw_summary = true,
            DaemonCommand::RequestConciergeWelcome => saw_welcome = true,
            _ => {}
        }
    }

    assert!(
        saw_summary,
        "operator profile autostart should not depend on showing concierge welcome"
    );
    assert!(
        !saw_welcome,
        "reconnect restore should still avoid replacing the restored thread with concierge"
    );
}

#[test]
fn reconnect_restore_resumes_thread_only_if_it_was_streaming_before_disconnect() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "partial answer".to_string(),
    });

    model.handle_reconnecting_event(3);
    model.handle_connected_event();
    while daemon_rx.try_recv().is_ok() {}

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Recovered".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    let mut saw_continue = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::SendMessage {
            thread_id, content, ..
        } = command
        {
            if thread_id.as_deref() == Some("thread-1") && content == "continue" {
                saw_continue = true;
            }
        }
    }

    assert!(
        saw_continue,
        "reconnect restore should resume the interrupted thread with the existing continue path"
    );
}

#[test]
fn reconnect_restore_does_not_resume_idle_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_reconnecting_event(3);
    model.handle_connected_event();
    while daemon_rx.try_recv().is_ok() {}

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Recovered".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::SendMessage { .. }) {
            panic!("idle reconnect restore should not auto-resume the thread");
        }
    }
}

#[test]
fn pump_daemon_events_budgeted_stops_after_limit() {
    let (daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);

    daemon_tx
        .send(ClientEvent::Error("first".to_string()))
        .expect("first event should send");
    daemon_tx
        .send(ClientEvent::Error("second".to_string()))
        .expect("second event should send");
    daemon_tx
        .send(ClientEvent::Error("third".to_string()))
        .expect("third event should send");

    let processed = model.pump_daemon_events_budgeted(2).processed;

    assert_eq!(processed, 2);
    assert_eq!(model.last_error.as_deref(), Some("second"));

    let remaining = model.pump_daemon_events_budgeted(usize::MAX).processed;

    assert_eq!(remaining, 1);
    assert_eq!(model.last_error.as_deref(), Some("third"));
}

#[test]
fn concierge_loading_state_is_visible_before_full_startup_burst_is_drained() {
    let (daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.agent_config_loaded = false;

    daemon_tx
        .send(ClientEvent::AgentConfigRaw(serde_json::json!({
            "provider": PROVIDER_ID_OPENAI,
            "base_url": "https://api.openai.com/v1",
            "model": "gpt-5.4",
        })))
        .expect("config event should send");
    daemon_tx
        .send(ClientEvent::Error("startup follow-up".to_string()))
        .expect("follow-up event should send");

    let processed = model.pump_daemon_events_budgeted(1).processed;

    assert_eq!(processed, 1);
    assert!(
        model.concierge.loading,
        "the first startup frame should keep concierge loading visible"
    );
    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "the loading hero should stay on the conversation view until later startup events are processed"
    );

    let remaining = model.pump_daemon_events_budgeted(usize::MAX).processed;
    assert_eq!(remaining, 1);
    assert!(
        !model.concierge.loading,
        "processing the remaining burst should clear the loading state"
    );
}

#[test]
fn operator_profile_completion_starts_concierge_loading_before_response() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });
    model.operator_profile.visible = true;
    model.operator_profile.loading = true;

    model.handle_operator_profile_session_completed_event(
        "session-1".to_string(),
        vec!["experience".to_string()],
    );

    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "operator profile completion should also surface the concierge loading view immediately"
    );
    assert_eq!(model.chat.active_thread_id(), None);
    assert!(model.concierge.loading);
    assert!(model.should_show_concierge_hero_loading());

    let mut saw_welcome = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::RequestConciergeWelcome) {
            saw_welcome = true;
            break;
        }
    }
    assert!(saw_welcome, "expected concierge welcome request");
}

#[test]
fn operator_profile_completion_preserves_active_concierge_welcome() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.handle_concierge_welcome_event(
        "Existing welcome".to_string(),
        vec![crate::state::ConciergeActionVm {
            label: "Resume".to_string(),
            action_type: "resume".to_string(),
            thread_id: None,
        }],
    );
    while daemon_rx.try_recv().is_ok() {}

    model.operator_profile.visible = true;
    model.operator_profile.loading = true;
    model.handle_operator_profile_session_completed_event(
        "session-1".to_string(),
        vec!["enabled".to_string()],
    );

    assert!(model.concierge.has_active_welcome());
    assert_eq!(
        model.concierge.welcome_content.as_deref(),
        Some("Existing welcome")
    );
    assert!(!model.concierge.loading);
    while let Ok(command) = daemon_rx.try_recv() {
        assert!(
            !matches!(command, DaemonCommand::RequestConciergeWelcome),
            "completion should not request a replacement welcome while one is active"
        );
    }
}

#[test]
fn partial_concierge_welcome_keeps_loading_animation_until_final_actions_arrive() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::AgentConfigRaw(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    })));
    model.concierge.loading = true;

    model.handle_concierge_welcome_event("Draft welcome".to_string(), vec![]);

    assert!(
        model.concierge.loading,
        "partial concierge content should keep the loading animation active"
    );
    assert!(
        matches!(model.main_pane_view, MainPaneView::Conversation),
        "partial concierge content should stay in the conversation pane"
    );
    assert_eq!(model.chat.active_thread_id(), Some("concierge"));
    assert!(
        model.actions_bar_visible(),
        "loading banner should remain visible while the welcome is still streaming"
    );

    model.handle_concierge_welcome_event(
        "Final welcome".to_string(),
        vec![crate::state::ConciergeActionVm {
            label: "Start new session".to_string(),
            action_type: "start_new".to_string(),
            thread_id: None,
        }],
    );

    assert!(
        !model.concierge.loading,
        "final concierge welcome should clear the loading animation"
    );
}

#[test]
fn operator_profile_autostart_waits_for_streaming_concierge_welcome_to_finish() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = false;

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
    }));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::OperatorProfileSummary {
        summary_json: serde_json::json!({
            "field_count": 0,
            "fields": {},
            "consents": {
                "enabled": true
            }
        })
        .to_string(),
    });

    while let Ok(command) = daemon_rx.try_recv() {
        assert!(
            !matches!(
                command,
                crate::state::DaemonCommand::StartOperatorProfileSession { .. }
            ),
            "operator profile onboarding should not start while concierge welcome is streaming"
        );
    }
    assert!(
        model.concierge.loading,
        "concierge welcome should still be in its streaming/loading phase"
    );

    model.handle_concierge_welcome_event(
        "Final welcome".to_string(),
        vec![crate::state::ConciergeActionVm {
            label: "Send a message".to_string(),
            action_type: "focus_chat".to_string(),
            thread_id: None,
        }],
    );

    let mut started_onboarding = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            crate::state::DaemonCommand::StartOperatorProfileSession { kind }
                if kind == "first_run_onboarding"
        ) {
            started_onboarding = true;
            break;
        }
    }
    assert!(
        started_onboarding,
        "operator profile onboarding should start after the final concierge welcome arrives"
    );
}

#[test]
fn final_concierge_welcome_requests_operator_profile_autostart_after_incomplete_summary() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = TuiModel::new(event_rx, daemon_tx);

    model.handle_concierge_welcome_event(
        "Final welcome".to_string(),
        vec![crate::state::ConciergeActionVm {
            label: "Send a message".to_string(),
            action_type: "focus_chat".to_string(),
            thread_id: None,
        }],
    );

    let mut requested_summary = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            crate::state::DaemonCommand::GetOperatorProfileSummary
        ) {
            requested_summary = true;
            break;
        }
    }
    assert!(
        requested_summary,
        "final concierge welcome should request profile summary before auto-starting onboarding"
    );

    model.handle_client_event(ClientEvent::OperatorProfileSummary {
        summary_json: serde_json::json!({
            "field_count": 0,
            "fields": {},
            "consents": {
                "enabled": true
            }
        })
        .to_string(),
    });

    let mut started_onboarding = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            crate::state::DaemonCommand::StartOperatorProfileSession { kind }
                if kind == "first_run_onboarding"
        ) {
            started_onboarding = true;
            break;
        }
    }
    assert!(
        started_onboarding,
        "incomplete profile summary should start first-run operator profile onboarding"
    );
}

#[test]
fn final_concierge_welcome_skips_operator_profile_autostart_after_consent_summary() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = TuiModel::new(event_rx, daemon_tx);

    model.handle_concierge_welcome_event(
        "Final welcome".to_string(),
        vec![crate::state::ConciergeActionVm {
            label: "Send a message".to_string(),
            action_type: "focus_chat".to_string(),
            thread_id: None,
        }],
    );

    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            crate::state::DaemonCommand::GetOperatorProfileSummary
        ) {
            break;
        }
    }

    model.handle_client_event(ClientEvent::OperatorProfileSummary {
        summary_json: serde_json::json!({
            "model": {},
            "consents": {
                "enabled": true,
                "allow_message_statistics": false,
                "allow_approval_learning": true,
                "allow_attention_tracking": false,
                "allow_implicit_feedback": true
            }
        })
        .to_string(),
    });

    let mut started_onboarding = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            crate::state::DaemonCommand::StartOperatorProfileSession { kind }
                if kind == "first_run_onboarding"
        ) {
            started_onboarding = true;
            break;
        }
    }
    assert!(
        !started_onboarding,
        "completed consent summary should not reopen first-run operator profile onboarding"
    );
}

#[test]
fn whatsapp_qr_event_opens_modal_and_sets_ascii_payload() {
    let mut model = make_model();
    assert!(model.modal.top().is_none());

    model.handle_client_event(ClientEvent::WhatsAppLinkQr {
        ascii_qr: "██\n██".to_string(),
        expires_at_ms: Some(123),
    });

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::WhatsAppLink)
    );
    assert_eq!(model.modal.whatsapp_link().ascii_qr(), Some("██\n██"));
    assert_eq!(model.modal.whatsapp_link().expires_at_ms(), Some(123));
}

#[test]
fn whatsapp_status_events_update_modal_state() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::WhatsAppLinkStatus {
        state: "connected".to_string(),
        phone: Some("+12065550123".to_string()),
        last_error: None,
    });
    assert_eq!(
        model.modal.whatsapp_link().phase(),
        crate::state::modal::WhatsAppLinkPhase::Connected
    );

    model.handle_client_event(ClientEvent::WhatsAppLinkError {
        message: "scan timeout".to_string(),
        recoverable: true,
    });
    assert_eq!(
        model.modal.whatsapp_link().phase(),
        crate::state::modal::WhatsAppLinkPhase::Error
    );
    assert!(model
        .modal
        .whatsapp_link()
        .status_text()
        .contains("scan timeout"));

    model.handle_client_event(ClientEvent::WhatsAppLinkDisconnected {
        reason: Some("socket closed".to_string()),
    });
    assert_eq!(
        model.modal.whatsapp_link().phase(),
        crate::state::modal::WhatsAppLinkPhase::Disconnected
    );
    assert!(model
        .modal
        .whatsapp_link()
        .status_text()
        .contains("socket closed"));
}

#[test]
fn tts_request_surfaces_pending_footer_activity_until_audio_starts() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Say this aloud".to_string(),
            timestamp: 1,
            ..Default::default()
        },
    });

    model.speak_latest_assistant_message();

    let command = daemon_rx.try_recv().expect("expected TTS command");
    assert!(matches!(command, DaemonCommand::TextToSpeech { .. }));
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("preparing speech")
    );

    model.handle_client_event(ClientEvent::TextToSpeechResult {
        content: r#"{"path":"/tmp/speech.mp3"}"#.to_string(),
    });

    assert!(
        model.footer_activity_text().is_none(),
        "pending TTS activity should clear once audio is ready to play"
    );
}

#[test]
fn first_raw_config_load_shows_rarog_runtime_before_concierge_config_reply() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = false;

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
        "concierge": {
            "enabled": true,
            "provider": "max-coding-plan",
            "model": "MiniMax-M3.1-Flash-Preview",
            "reasoning_effort": "low",
        },
    }));

    assert!(model.should_show_concierge_hero_loading());
    let profile = model.current_header_agent_profile();
    assert_eq!(
        profile.provider, "max-coding-plan",
        "the welcome loads before GetConciergeConfig answers, so the header must already show Rarog, not Svarog"
    );
    assert_eq!(profile.model, "MiniMax-M3.1-Flash-Preview");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("low"));
}
