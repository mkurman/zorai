use super::idle_tick_does_not_request_redraw_to_first_raw_config_load_triggers::*;
use crate::app::*;
#[test]
fn image_generation_result_refreshes_thread_and_work_context() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            ..Default::default()
        },
    ]));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::GenerateImageResult {
        content: r#"{"ok":true,"thread_id":"thread-1","path":"/tmp/generated-image.png"}"#
            .to_string(),
    });

    assert_eq!(
        next_thread_request(&mut daemon_rx),
        Some((
            "thread-1".to_string(),
            Some(model.config.tui_chat_history_page_size as usize),
            Some(0),
        ))
    );
    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::RequestThreadWorkContext(thread_id)) if thread_id == "thread-1"
    ));
    assert_eq!(
        model.status_line,
        "Image generated: /tmp/generated-image.png"
    );
}

#[test]
fn late_tool_result_after_done_does_not_restore_footer_activity() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "generate_image".to_string(),
        arguments: "{\"prompt\":\"test\"}".to_string(),
        weles_review: None,

        message_id: None,
    });
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("⚙  generate_image")
    );

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,

        message_id: None,
    });
    assert!(
        model.footer_activity_text().is_none(),
        "done should clear the footer activity for the completed turn"
    );

    model.handle_client_event(ClientEvent::ToolResult {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "generate_image".to_string(),
        content: "{\"path\":\"/tmp/image.png\"}".to_string(),
        is_error: false,
        weles_review: None,

        message_id: None,
    });

    assert!(
        model.footer_activity_text().is_none(),
        "late tool results after done must not restore the footer activity badge"
    );
}

#[cfg(unix)]
#[test]
fn text_to_speech_tool_result_autoplays_audio_in_chat_threads() {
    with_fake_mpv_in_path(|| {
        let mut model = make_model();
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

        let audio_path =
            std::env::temp_dir().join(format!("zorai-test-speech-{}.mp3", std::process::id()));
        std::fs::write(&audio_path, b"fake mp3 bytes").expect("fake audio file should exist");

        model.handle_client_event(ClientEvent::ToolResult {
            thread_id: "thread-1".to_string(),
            call_id: "call-1".to_string(),
            name: "text_to_speech".to_string(),
            content: serde_json::json!({
                "path": audio_path.display().to_string(),
            })
            .to_string(),
            is_error: false,
            weles_review: None,

            message_id: None,
        });

        assert_eq!(model.status_line, "Playing synthesized speech...");

        if let Some(mut child) = model.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(audio_path);
    });
}

#[test]
fn collaboration_sessions_event_populates_workspace_state_without_error_modal() {
    let mut model = make_model();

    model.handle_collaboration_sessions_event(
        serde_json::json!([
            {
                "id": "session-1",
                "parent_task_id": "task-1",
                "agents": [{"role": "research"}, {"role": "testing"}],
                "disagreements": [
                    {
                        "id": "disagreement-1",
                        "topic": "deployment strategy",
                        "positions": ["roll forward", "roll back"],
                        "votes": [{"task_id": "subagent-1"}]
                    }
                ]
            }
        ])
        .to_string(),
    );

    assert!(matches!(model.main_pane_view, MainPaneView::Collaboration));
    assert_eq!(model.modal.top(), None);
    assert!(!model.error_active);
    assert_eq!(model.status_line, "Collaboration sessions loaded");
    assert_eq!(model.collaboration.rows().len(), 2);
    assert_eq!(
        model
            .collaboration
            .selected_row()
            .and_then(crate::state::collaboration::CollaborationRowVm::disagreement_id),
        Some("disagreement-1")
    );
}

#[test]
fn collaboration_vote_result_requests_refresh_and_updates_status() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::CollaborationVoteResult {
        report_json: serde_json::json!({
            "session_id": "session-1",
            "resolution": "resolved"
        })
        .to_string(),
    });

    assert_eq!(model.status_line, "Vote recorded: resolved.");
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected collaboration refresh after vote result"),
        DaemonCommand::GetCollaborationSessions
    ));
}

#[test]
fn collaboration_sessions_event_surfaces_escalation_notice() {
    let mut model = make_model();

    model.handle_collaboration_sessions_event(
        serde_json::json!([
            {
                "id": "session-1",
                "parent_task_id": "task-1",
                "disagreements": [
                    {
                        "id": "disagreement-1",
                        "topic": "deployment strategy",
                        "positions": ["roll forward", "roll back"],
                        "resolution": "pending",
                        "confidence_gap": 0.1,
                        "votes": []
                    }
                ]
            }
        ])
        .to_string(),
    );

    let notice = model
        .input_notice_style()
        .expect("escalation should surface a notice");
    assert!(notice.0.contains("Collaboration escalation"));
    assert!(
        model
            .collaboration
            .selected_session()
            .and_then(|session| session.escalation.as_ref())
            .is_some(),
        "session should carry escalation summary for workspace rendering"
    );
}

#[test]
fn operator_question_event_appends_inline_message_and_actions_without_modal() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::OperatorQuestion {
        question_id: "oq-1".to_string(),
        content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
        options: vec!["A".to_string(), "B".to_string()],
        session_id: None,
        thread_id: Some("thread-1".to_string()),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert_eq!(
        thread.messages.len(),
        1,
        "operator question should append an inline transcript message"
    );
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert!(message.is_operator_question);
    assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
    assert_eq!(
        message.content,
        "Approve this slice?\nA - proceed\nB - revise"
    );
    assert_eq!(message.actions.len(), 3);
    assert_eq!(message.actions[0].label, "A");
    assert_eq!(message.actions[1].label, "B");
    assert_eq!(message.actions[2].label, "Other");
    assert_eq!(model.modal.top(), None);
    assert_ne!(
        model.modal.top(),
        Some(modal::ModalKind::OperatorQuestionOverlay)
    );
}

#[test]
fn thread_deleted_event_removes_thread_from_chat_state() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
        chat::AgentThread {
            id: "thread-2".into(),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));
    model.open_thread_conversation("thread-1".into());

    model.handle_client_event(ClientEvent::ThreadDeleted {
        thread_id: "thread-1".into(),
        deleted: true,
    });

    assert!(model
        .chat
        .threads()
        .iter()
        .all(|thread| thread.id != "thread-1"));
}

#[test]
fn thread_deleted_event_refreshes_runtime_state() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
    ]));

    model.handle_client_event(ClientEvent::ThreadDeleted {
        thread_id: "thread-1".into(),
        deleted: true,
    });

    assert!(matches!(daemon_rx.try_recv(), Ok(DaemonCommand::Refresh)));
}

#[test]
fn stale_thread_detail_after_delete_does_not_recreate_thread() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
        chat::AgentThread {
            id: "thread-2".into(),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));
    model.open_thread_conversation("thread-1".into());

    model.handle_client_event(ClientEvent::ThreadDeleted {
        thread_id: "thread-1".into(),
        deleted: true,
    });
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".into(),
        title: "Thread One".into(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "stale detail".into(),
            timestamp: 1,
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    assert!(model
        .chat
        .threads()
        .iter()
        .all(|thread| thread.id != "thread-1"));
    assert_eq!(model.chat.active_thread_id(), Some("thread-2"));
}

#[test]
fn stale_thread_list_after_delete_does_not_recreate_thread() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadList(vec![
        crate::wire::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
        crate::wire::AgentThread {
            id: "thread-2".into(),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));
    model.open_thread_conversation("thread-1".into());

    model.handle_client_event(ClientEvent::ThreadDeleted {
        thread_id: "thread-1".into(),
        deleted: true,
    });
    model.handle_client_event(ClientEvent::ThreadList(vec![
        crate::wire::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
        crate::wire::AgentThread {
            id: "thread-2".into(),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));

    assert!(model
        .chat
        .threads()
        .iter()
        .all(|thread| thread.id != "thread-1"));
    assert_eq!(model.chat.active_thread_id(), Some("thread-2"));
}

#[test]
fn thread_deleted_event_reclamps_open_thread_picker_cursor() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            agent_name: Some("Svarog".into()),
            title: "Thread One".into(),
            ..Default::default()
        },
        chat::AgentThread {
            id: "thread-2".into(),
            agent_name: Some("Svarog".into()),
            title: "Thread Two".into(),
            ..Default::default()
        },
    ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(2));

    assert_eq!(model.modal.picker_cursor(), 2);
    assert_eq!(
        model
            .selected_thread_picker_thread()
            .map(|thread| thread.id.as_str()),
        Some("thread-2")
    );

    model.handle_client_event(ClientEvent::ThreadDeleted {
        thread_id: "thread-2".into(),
        deleted: true,
    });

    assert_eq!(model.modal.picker_cursor(), 1);
    assert_eq!(
        model
            .selected_thread_picker_thread()
            .map(|thread| thread.id.as_str()),
        Some("thread-1")
    );
}
