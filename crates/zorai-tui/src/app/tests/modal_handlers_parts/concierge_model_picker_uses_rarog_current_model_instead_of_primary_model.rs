use super::whatsapp_modal_esc_sends_stop_and_closes_to_clicking_rendered_settings::*;
use crate::app::*;
#[test]
fn concierge_model_picker_uses_rarog_current_model_instead_of_primary_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.model = "gpt-5.4".to_string();
    model.concierge.model = Some("claude-sonnet-4-5".to_string());
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::ConciergeModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.concierge.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(model.config.model, "gpt-5.4");
}

#[test]
fn concierge_custom_model_entry_does_not_mutate_primary_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.model = "gpt-5.4".to_string();
    model.concierge.model = Some("gpt-5.4-mini".to_string());
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Concierge));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::ConciergeModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.settings.editing_field(), Some("concierge_model"));
    model.settings.reduce(SettingsAction::InsertChar('x'));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(model.concierge.model.as_deref(), Some("gpt-5.4-minix"));
    assert_eq!(model.config.model, "gpt-5.4");
}

#[test]
fn thread_picker_right_arrow_switches_to_rarog_tab() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Rarog
    );
}

#[test]
fn thread_picker_agent_tab_switch_debounces_refresh_until_deadline() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Rarog
    );
    assert_eq!(
        model.thread_picker_loading_tab(),
        None,
        "the tab must not look busy while the user is still switching"
    );
    assert!(daemon_rx.try_recv().is_err());

    let deadline = model
        .pending_thread_picker_refresh_deadline()
        .expect("tab switch should queue a refresh deadline");
    assert!(
        !model.dispatch_due_thread_picker_refresh(deadline - std::time::Duration::from_millis(1))
    );
    assert!(daemon_rx.try_recv().is_err());

    assert!(model.dispatch_due_thread_picker_refresh(deadline));
    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::RefreshThreadsForAgent {
            agent_filter: Some(filter),
        }) if filter == "rarog"
    ));
    assert_eq!(
        model.thread_picker_loading_tab(),
        Some(modal::ThreadPickerTab::Rarog)
    );

    model.handle_client_event(ClientEvent::ThreadList(Vec::new()));
    assert_eq!(model.thread_picker_loading_tab(), None);
}

#[test]
fn thread_picker_agent_tab_switch_only_refreshes_latest_selection() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));

    assert!(!model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    ));
    let first_deadline = model
        .pending_thread_picker_refresh_deadline()
        .expect("first switch should queue a refresh");
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert!(!model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    ));
    let final_deadline = model
        .pending_thread_picker_refresh_deadline()
        .expect("second switch should replace the queued refresh");

    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Weles
    );
    assert_eq!(model.thread_picker_loading_tab(), None);
    assert!(final_deadline > first_deadline);
    assert!(daemon_rx.try_recv().is_err());

    assert!(!model.dispatch_due_thread_picker_refresh(first_deadline));
    assert!(daemon_rx.try_recv().is_err());

    assert!(model.dispatch_due_thread_picker_refresh(final_deadline));

    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::RefreshThreadsForAgent {
            agent_filter: Some(filter),
        }) if filter == "weles"
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn thread_picker_gateway_tab_switch_requests_unfiltered_refresh() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));

    for _ in 0..7 {
        assert!(!model.handle_key_modal(
            KeyCode::Right,
            KeyModifiers::NONE,
            modal::ModalKind::ThreadPicker,
        ));
    }

    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Gateway
    );
    assert_eq!(model.thread_picker_loading_tab(), None);
    assert!(daemon_rx.try_recv().is_err());

    let deadline = model
        .pending_thread_picker_refresh_deadline()
        .expect("gateway switch should queue a refresh");
    assert!(model.dispatch_due_thread_picker_refresh(deadline));

    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::RefreshThreadsForAgent { agent_filter: None })
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn closing_thread_picker_cancels_pending_source_refresh() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));

    assert!(!model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    ));
    let deadline = model
        .pending_thread_picker_refresh_deadline()
        .expect("source switch should queue a refresh");

    model.close_top_modal();

    assert_eq!(model.pending_thread_picker_refresh_deadline(), None);
    assert!(!model.dispatch_due_thread_picker_refresh(deadline));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn thread_picker_left_right_cycles_all_sources() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Rarog
    );

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Weles
    );

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Goals
    );

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Workspace
    );

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Playgrounds
    );

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Internal
    );

    let quit = model.handle_key_modal(
        KeyCode::Left,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Playgrounds
    );
}

#[test]
fn thread_picker_enter_selects_filtered_rarog_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "regular-thread".into(),
            title: "Regular work".into(),
            ..Default::default()
        },
        chat::AgentThread {
            id: "heartbeat-1".into(),
            title: "HEARTBEAT SYNTHESIS".into(),
            ..Default::default()
        },
    ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Rarog);
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(model.chat.active_thread_id(), Some("heartbeat-1"));
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "heartbeat-1");
            assert_eq!(
                message_limit,
                Some(model.chat_history_delete_backfill_target_size())
            );
            assert_eq!(message_offset, Some(0));
        }
        other => panic!("expected thread request, got {:?}", other),
    }
}

#[test]
fn thread_picker_new_conversation_uses_selected_agent_for_first_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Weles);
    model.sync_thread_picker_item_count();

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);

    model.submit_prompt("tell me your secrets".to_string());

    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            Ok(DaemonCommand::SendMessage {
                thread_id,
                target_agent_id,
                content,
                ..
            }) => {
                assert_eq!(thread_id, None);
                assert_eq!(target_agent_id.as_deref(), Some("weles"));
                assert_eq!(content, "tell me your secrets");
                break;
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
    }
}

#[test]
fn slash_new_defaults_to_svarog_target_for_first_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;

    assert!(model.execute_slash_command_line("/new"));
    model.submit_prompt("default me".to_string());

    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            Ok(DaemonCommand::SendMessage {
                thread_id,
                target_agent_id,
                content,
                ..
            }) => {
                assert_eq!(thread_id, None);
                assert_eq!(
                    target_agent_id.as_deref(),
                    Some(zorai_protocol::AGENT_ID_SWAROG)
                );
                assert_eq!(content, "default me");
                break;
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
    }
}

#[test]
fn slash_new_with_custom_subagent_targets_first_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .subagents
        .entries
        .push(sample_subagent("domowoj", "Domowoj", false));

    assert!(model.execute_slash_command_line("/new domowoj"));
    model.submit_prompt("inspect the workspace".to_string());

    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            Ok(DaemonCommand::SendMessage {
                thread_id,
                target_agent_id,
                content,
                ..
            }) => {
                assert_eq!(thread_id, None);
                assert_eq!(target_agent_id.as_deref(), Some("domowoj"));
                assert_eq!(content, "inspect the workspace");
                break;
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
    }
}

#[test]
fn slash_thread_with_agent_preselects_matching_source() {
    let (mut model, _daemon_rx) = make_model();
    model
        .subagents
        .entries
        .push(sample_subagent("domowoj", "Domowoj", false));

    assert!(model.execute_slash_command_line("/thread domowoj"));

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ThreadPicker));
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Agent("domowoj".to_string())
    );
}

#[test]
fn slash_thread_without_args_preselects_active_thread_owner_agent() {
    let (mut model, _daemon_rx) = make_model();
    model
        .subagents
        .entries
        .push(sample_subagent("domowoj", "Domowoj", false));
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-domowoj".to_string(),
            agent_name: Some("Domowoj".to_string()),
            title: "Domowoj thread".to_string(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-domowoj".to_string()));

    assert!(model.execute_slash_command_line("/thread"));

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ThreadPicker));
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Agent("domowoj".to_string())
    );
}

#[test]
fn slash_image_prompt_dispatches_generate_image_for_active_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    assert!(model.execute_slash_command_line("/image retro robot portrait"));

    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            Ok(DaemonCommand::GenerateImage { args_json }) => {
                let payload: serde_json::Value =
                    serde_json::from_str(&args_json).expect("image payload should parse");
                assert_eq!(
                    payload.get("thread_id").and_then(|v| v.as_str()),
                    Some("thread-1")
                );
                assert_eq!(
                    payload.get("prompt").and_then(|v| v.as_str()),
                    Some("retro robot portrait")
                );
                break;
            }
            other => panic!("expected generate-image command, got {:?}", other),
        }
    }
}

#[test]
fn slash_video_prompt_dispatches_generate_video_for_active_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    assert!(model.execute_slash_command_line("/video drone shot over a foggy forest"));

    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            Ok(DaemonCommand::GenerateVideo { args_json }) => {
                let payload: serde_json::Value =
                    serde_json::from_str(&args_json).expect("video payload should parse");
                assert_eq!(
                    payload.get("thread_id").and_then(|v| v.as_str()),
                    Some("thread-1")
                );
                assert_eq!(
                    payload.get("prompt").and_then(|v| v.as_str()),
                    Some("drone shot over a foggy forest")
                );
                break;
            }
            other => panic!("expected generate-video command, got {:?}", other),
        }
    }
    assert_eq!(model.status_line, "Generating video...");
}

#[test]
fn slash_video_without_prompt_prefills_composer_instead_of_dispatching() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;

    assert!(model.execute_slash_command_line("/video"));

    while let Ok(command) = daemon_rx.try_recv() {
        assert!(
            !matches!(command, DaemonCommand::GenerateVideo { .. }),
            "empty /video must not start a generation"
        );
    }
    assert_eq!(model.input.buffer(), "/video ");
}

#[test]
fn thread_picker_delete_requires_confirmation_before_sending_delete_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            agent_name: Some("Svarog".into()),
            title: "Thread One".into(),
            ..Default::default()
        },
    ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Delete,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert!(
        daemon_rx.try_recv().is_err(),
        "delete should wait for confirmation"
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected delete-thread command"),
        DaemonCommand::DeleteThread { thread_id } if thread_id == "thread-1"
    ));
}

#[test]
fn thread_picker_delete_stops_streaming_thread_before_delete() {
    let (mut model, mut daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            agent_name: Some("Svarog".into()),
            title: "Thread One".into(),
            ..Default::default()
        },
    ]));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::Delta {
        thread_id: "thread-1".to_string(),
        content: "working".to_string(),
    });
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    model.handle_key_modal(
        KeyCode::Delete,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(matches!(
        daemon_rx.try_recv().expect("expected stop-stream command"),
        DaemonCommand::StopStream { thread_id } if thread_id == "thread-1"
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected delete-thread command"),
        DaemonCommand::DeleteThread { thread_id } if thread_id == "thread-1"
    ));
}

#[test]
fn thread_picker_ctrl_s_busy_thread_requires_confirmation_before_stop() {
    let (mut model, mut daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            agent_name: Some("Svarog".into()),
            title: "Thread One".into(),
            ..Default::default()
        },
    ]));
    model.open_thread_conversation("thread-1".into());
    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::RequestThread { .. }) => break,
            Ok(_) => continue,
            Err(_) => panic!("expected thread request when opening conversation"),
        }
    }
    model.handle_delta_event("thread-1".into(), "streaming".into());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected stop-stream command"),
        DaemonCommand::StopStream { thread_id } if thread_id == "thread-1"
    ));
}
