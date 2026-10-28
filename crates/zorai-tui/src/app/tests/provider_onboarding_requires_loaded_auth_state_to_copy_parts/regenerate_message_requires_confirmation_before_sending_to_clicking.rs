use super::super::unbounded_channel;
use super::*;
use std::sync::mpsc;

#[test]
fn regenerate_message_requires_confirmation_before_sending() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
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
            role: chat::MessageRole::User,
            content: "Original prompt".to_string(),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Answer".to_string(),
            ..Default::default()
        },
    });

    model.request_regenerate_message(1);

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert!(
        cmd_rx.try_recv().is_err(),
        "regenerate should wait for confirmation"
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    assert!(!quit);

    let mut saw_send = false;
    while let Ok(command) = cmd_rx.try_recv() {
        if matches!(command, DaemonCommand::SendMessage { .. }) {
            saw_send = true;
            break;
        }
    }
    assert!(
        saw_send,
        "confirmation should eventually send the regenerated prompt"
    );
}

#[test]
fn pin_action_dispatches_without_confirmation_modal() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
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
            id: Some("message-1".to_string()),
            role: chat::MessageRole::User,
            content: "Original prompt".to_string(),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(0));
    model.chat.select_message_action(2);

    assert!(model.execute_selected_inline_message_action());
    assert_ne!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::PinThreadMessageForCompaction {
            thread_id,
            message_id,
        }) => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_id, "message-1");
        }
        other => panic!("expected pin command, got {other:?}"),
    }
}

#[test]
fn delete_message_requires_confirmation_before_removing_message() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 100;
    model.height = 40;
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
            id: Some("m1".to_string()),
            role: chat::MessageRole::Assistant,
            content: "Answer".to_string(),
            ..Default::default()
        },
    });

    model.request_delete_message(0);

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .chat
            .active_thread()
            .map(|thread| thread.messages.len()),
        Some(1),
        "message should remain until deletion is confirmed"
    );
    assert!(
        cmd_rx.try_recv().is_err(),
        "delete should wait for confirmation"
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    assert!(!quit);

    let sent = cmd_rx
        .try_recv()
        .expect("confirmation should send delete command");
    assert!(matches!(sent, DaemonCommand::DeleteMessages { .. }));
    assert_eq!(
        model
            .chat
            .active_thread()
            .map(|thread| thread.messages.len()),
        Some(0),
        "message should be removed after deletion is confirmed"
    );
}

#[test]
fn deleting_from_full_latest_window_fetches_after_five_optimistic_deletes() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.config.tui_chat_history_page_size = 100;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 150,
            loaded_message_start: 50,
            loaded_message_end: 150,
            messages: (50..150)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Answer {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    for _ in 0..4 {
        model.request_delete_message(50);
        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
        assert!(!quit);
        assert!(matches!(
            cmd_rx.try_recv(),
            Ok(DaemonCommand::DeleteMessages { .. })
        ));
        assert!(
            cmd_rx.try_recv().is_err(),
            "delete backfill should wait until five local deletes are queued"
        );
    }

    model.request_delete_message(50);
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    assert!(!quit);
    assert!(matches!(
        cmd_rx.try_recv(),
        Ok(DaemonCommand::DeleteMessages { .. })
    ));
    match cmd_rx
        .try_recv()
        .expect("fifth delete should request five older messages immediately")
    {
        DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_limit, Some(5));
            assert_eq!(message_offset, Some(100));
        }
        other => panic!("expected five-message backfill request, got {other:?}"),
    }
    assert!(
        !model
            .pending_local_message_delete_backfills
            .contains_key("thread-1"),
        "delete backfill counter should reset after threshold request"
    );
}

#[test]
fn configured_history_target_uses_only_max_chat_messages() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 120;
    model.height = 80;
    model.config.tui_chat_history_page_size = 25;

    assert_eq!(model.chat_history_delete_backfill_target_size(), 27);
    model.request_latest_thread_page("thread-1".to_string(), false);

    match cmd_rx
        .try_recv()
        .expect("latest thread request should use configured history target")
    {
        DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_limit, Some(27));
            assert_eq!(message_offset, Some(0));
        }
        other => panic!("expected latest thread request, got {other:?}"),
    }
}

#[test]
fn deleting_from_underfilled_window_fetches_exact_threshold_batch() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.config.tui_chat_history_page_size = 100;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 120,
            loaded_message_start: 20,
            loaded_message_end: 40,
            messages: (20..40)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Answer {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    for _ in 0..5 {
        model.request_delete_message(10);
        model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
        assert!(matches!(
            cmd_rx.try_recv(),
            Ok(DaemonCommand::DeleteMessages { .. })
        ));
    }
    match cmd_rx
        .try_recv()
        .expect("fifth underfilled delete should request exactly five older rows")
    {
        DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_limit, Some(5));
            assert_eq!(message_offset, Some(100));
        }
        other => panic!("expected threshold backfill request, got {other:?}"),
    }
}

#[test]
fn deleting_from_compact_window_fetches_exact_threshold_batch() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 120;
    model.height = 80;
    model.config.tui_chat_history_page_size = 20;
    let target_size = model.chat_history_delete_backfill_target_size();
    assert_eq!(target_size, 22);
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 200,
            loaded_message_start: 180,
            loaded_message_end: 200,
            messages: (180..200)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Answer {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    for _ in 0..5 {
        model.request_delete_message(10);
        model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
        let _ = cmd_rx.try_recv().expect("delete command");
    }

    match cmd_rx
        .try_recv()
        .expect("fifth delete should request one exact backfill batch")
    {
        DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_limit, Some(5));
            assert_eq!(message_offset, Some(20));
        }
        other => panic!("expected history-window-sized backfill request, got {other:?}"),
    }
}

#[test]
fn short_loaded_thread_with_older_history_does_not_fake_scrollbar_rows() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 120;
    model.height = 60;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 2,
            loaded_message_start: 1,
            loaded_message_end: 2,
            messages: vec![chat::AgentMessage {
                id: Some("m1".to_string()),
                role: chat::MessageRole::Assistant,
                content: "Latest visible".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    assert!(
        model.chat_scrollbar_layout().is_none(),
        "unloaded older history should not create fake scrollbar rows when loaded lines fit"
    );

    model.chat.reduce(chat::ChatAction::ScrollChat(1));
    model.on_tick();
    match cmd_rx
        .try_recv()
        .expect("scrolling up should request older history even without a fake scrollbar")
    {
        DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_limit, Some(22));
            assert_eq!(message_offset, Some(1));
        }
        other => panic!("expected older history request, got {other:?}"),
    }
}

#[test]
fn delete_backfill_response_appends_batch_and_resets_counter() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.config.tui_chat_history_page_size = 100;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 120,
            loaded_message_start: 20,
            loaded_message_end: 40,
            messages: (20..40)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Answer {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    for _ in 0..5 {
        model.request_delete_message(10);
        model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
        let _ = cmd_rx.try_recv().expect("delete command");
    }
    let _ = cmd_rx.try_recv().expect("initial delete backfill request");

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        total_message_count: 115,
        loaded_message_start: 15,
        loaded_message_end: 20,
        messages: (15..20)
            .map(|index| crate::wire::AgentMessage {
                id: Some(format!("m{index}")),
                role: crate::wire::MessageRole::Assistant,
                content: format!("Answer {index}"),
                timestamp: index as u64,
                message_kind: "normal".to_string(),
                ..Default::default()
            })
            .collect(),
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    let thread = model.chat.active_thread().expect("thread should remain");
    assert_eq!(thread.loaded_message_start, 15);
    assert_eq!(thread.loaded_message_end, 35);
    assert_eq!(thread.messages.len(), 20);
    assert!(
        !model
            .pending_local_message_delete_backfills
            .contains_key("thread-1"),
        "delete backfill counter should stay reset after applying the response"
    );
    while let Ok(command) = cmd_rx.try_recv() {
        if matches!(command, DaemonCommand::RequestThread { .. }) {
            panic!("exact delete batch response should not enqueue a follow-up request")
        }
    }
}

#[test]
fn repeated_deletes_request_every_five_and_reset_counter() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.config.tui_chat_history_page_size = 100;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 170,
            loaded_message_start: 70,
            loaded_message_end: 170,
            messages: (70..170)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Answer {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    let mut delete_count = 0usize;
    let mut backfill_requests = Vec::new();
    for _ in 0..10 {
        let last_index = model
            .chat
            .active_thread()
            .map(|thread| thread.messages.len().saturating_sub(1))
            .expect("thread should remain");
        model.request_delete_message(last_index);
        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
        assert!(!quit);
        assert_ne!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

        while let Ok(command) = cmd_rx.try_recv() {
            match command {
                DaemonCommand::DeleteMessages { .. } => delete_count += 1,
                DaemonCommand::RequestThread {
                    thread_id,
                    message_limit,
                    message_offset,
                } => backfill_requests.push((thread_id, message_limit, message_offset)),
                other => panic!("unexpected command during optimistic delete: {other:?}"),
            }
        }
    }

    let thread = model.chat.active_thread().expect("thread should remain");
    assert_eq!(thread.messages.len(), 90);
    assert!(
            !model.chat.active_thread_older_page_pending(),
            "delete backfills should run in the background without showing the older-history loading row"
        );
    assert_eq!(delete_count, 10);
    assert_eq!(
        backfill_requests,
        vec![
            ("thread-1".to_string(), Some(5), Some(100)),
            ("thread-1".to_string(), Some(5), Some(105)),
        ]
    );
    assert!(
        !model
            .pending_local_message_delete_backfills
            .contains_key("thread-1"),
        "delete counter should reset after each threshold batch"
    );
}

#[test]
fn deleting_while_scrolled_reclamps_locked_chat_cursor() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 100;
    model.height = 30;
    model.config.tui_chat_history_page_size = 100;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 120,
            loaded_message_start: 20,
            loaded_message_end: 120,
            messages: (20..120)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!(
                        "Answer {index}\nline two for {index}\nline three for {index}"
                    ),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model
        .chat
        .reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));
    let before = model
        .chat_scrollbar_layout()
        .expect("test transcript should be scrollable before delete");
    model.set_chat_scroll_offset(before.max_scroll);
    assert_eq!(model.chat.scroll_offset(), before.max_scroll);

    model.request_delete_message(99);
    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    let after = model
        .chat_scrollbar_layout()
        .expect("test transcript should remain scrollable after delete");
    assert!(
        after.max_scroll < before.max_scroll,
        "deleting a rendered message should reduce the maximum scroll"
    );
    assert_eq!(
        model.chat.scroll_offset(),
        after.max_scroll,
        "optimistic delete should move the stored cursor with the shorter transcript"
    );
}

#[test]
fn scrolling_while_delete_count_below_threshold_does_not_request_normal_older_page() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 100;
    model.height = 40;
    model.config.tui_chat_history_page_size = 100;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 170,
            loaded_message_start: 70,
            loaded_message_end: 170,
            messages: (70..170)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Answer {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    for _ in 0..4 {
        let last_index = model
            .chat
            .active_thread()
            .map(|thread| thread.messages.len().saturating_sub(1))
            .expect("thread should remain");
        model.request_delete_message(last_index);
        model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
    }
    while cmd_rx.try_recv().is_ok() {}

    model
        .chat
        .reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));
    for _ in 0..chat::CHAT_HISTORY_FETCH_DEBOUNCE_TICKS {
        model.on_tick();
    }

    let mut next_request = None;
    while let Ok(command) = cmd_rx.try_recv() {
        if matches!(command, DaemonCommand::RequestThread { .. }) {
            next_request = Some(command);
            break;
        }
    }
    assert!(
        next_request.is_none(),
        "below-threshold delete backfill must not fall through to normal older-history fetch"
    );
    assert!(
        !model.chat.active_thread_older_page_pending(),
        "below-threshold delete backfill must not leave the chat in loading-older state"
    );
}

#[test]
fn inactive_stale_delete_backfill_page_does_not_enqueue_background_bridge() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 100;
    model.height = 40;
    model.config.tui_chat_history_page_size = 100;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread 1".to_string(),
            total_message_count: 170,
            loaded_message_start: 70,
            loaded_message_end: 170,
            messages: (70..170)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Answer {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-2".to_string(),
            title: "Thread 2".to_string(),
            messages: vec![chat::AgentMessage {
                id: Some("other-1".to_string()),
                role: chat::MessageRole::Assistant,
                content: "Other thread".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    for _ in 0..99 {
        let last_index = model
            .chat
            .active_thread()
            .map(|thread| thread.messages.len().saturating_sub(1))
            .expect("thread should remain");
        model.request_delete_message(last_index);
        model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
    }
    while cmd_rx.try_recv().is_ok() {}

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-2".to_string()));
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread 1".to_string(),
        total_message_count: 170,
        loaded_message_start: 99,
        loaded_message_end: 169,
        messages: (99..169)
            .map(|index| crate::wire::AgentMessage {
                id: Some(format!("m{index}")),
                role: crate::wire::MessageRole::Assistant,
                content: format!("Answer {index}"),
                timestamp: index as u64,
                message_kind: "normal".to_string(),
                ..Default::default()
            })
            .collect(),
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    assert_eq!(model.chat.active_thread_id(), Some("thread-2"));
    let mut history_request = None;
    while let Ok(command) = cmd_rx.try_recv() {
        if matches!(command, DaemonCommand::RequestThread { .. }) {
            history_request = Some(command);
            break;
        }
    }
    assert!(
            history_request.is_none(),
            "inactive delete backfills must not enqueue background requests that can starve selected thread loads"
        );
    assert!(
        !model
            .pending_local_message_delete_backfills
            .contains_key("thread-1"),
        "inactive delete backfill state should be cleared instead of looping in the background"
    );
}

#[test]
fn opening_thread_clears_pending_delete_backfills_before_requesting_detail() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model
        .pending_local_message_delete_backfills
        .insert("thread-1".to_string(), 99);
    model.pending_local_message_delete_fetches.insert(
        "thread-1".to_string(),
        PendingDeleteBackfillFetch {
            message_limit: 5,
            message_offset: 95,
            outstanding_rows: 5,
            requested_at_tick: model.tick_counter,
        },
    );

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-2".to_string()));
    model.request_latest_thread_page("thread-2".to_string(), true);

    assert!(model.pending_local_message_delete_backfills.is_empty());
    assert!(model.pending_local_message_delete_fetches.is_empty());
    match cmd_rx
        .try_recv()
        .expect("selected thread detail should be requested immediately")
    {
        DaemonCommand::RequestThread { thread_id, .. } => {
            assert_eq!(thread_id, "thread-2");
        }
        other => panic!("expected selected thread request, got {other:?}"),
    }
}

#[test]
fn locally_deleted_thread_reload_required_does_not_reload_from_db() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.config.tui_chat_history_page_size = 100;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 101,
            loaded_message_start: 1,
            loaded_message_end: 101,
            messages: (1..101)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Answer {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.request_delete_message(50);
    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    let _ = cmd_rx.try_recv().expect("delete command");
    assert!(
        cmd_rx.try_recv().is_err(),
        "delete backfill should be delayed and batched"
    );
    assert_eq!(
        model
            .chat
            .local_deleted_message_count_for_thread("thread-1"),
        1,
        "optimistic delete should keep one local tombstone until daemon confirmation"
    );

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-1".to_string(),
    });

    assert!(
            cmd_rx.try_recv().is_err(),
            "local message deletion should suppress the daemon reload event instead of reloading the same cached thread from db"
        );
    assert_eq!(
        model
            .chat
            .local_deleted_message_count_for_thread("thread-1"),
        0,
        "daemon delete confirmation should clear the optimistic tombstone"
    );
}

#[test]
fn short_loaded_thread_with_older_history_requests_older_page_without_scrolling() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 120;
    model.height = 60;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 2,
            loaded_message_start: 1,
            loaded_message_end: 2,
            messages: vec![chat::AgentMessage {
                id: Some("m1".to_string()),
                role: chat::MessageRole::Assistant,
                content: "Latest visible".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.on_tick();
    match cmd_rx
        .try_recv()
        .expect("a loaded tail that leaves the chat half empty cannot be scrolled, so older history must load on its own")
    {
        DaemonCommand::RequestThread {
            thread_id,
            message_offset,
            ..
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_offset, Some(1));
        }
        other => panic!("expected older history request, got {other:?}"),
    }
}

fn expanded_thread_model(lines_per_message: usize) -> TuiModel {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 120;
    model.height = 60;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 6,
            loaded_message_start: 0,
            loaded_message_end: 6,
            messages: (0..6)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: vec![format!("Answer {index}"); lines_per_message].join("\n\n"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model
        .chat
        .active_thread_mut()
        .expect("thread selected")
        .history_window_expanded = true;
    model.chat.set_history_page_size(2);
    assert_eq!(
        model
            .chat
            .active_thread()
            .map(|thread| thread.messages.len()),
        Some(6)
    );
    model
}

#[test]
fn expanded_history_stays_loaded_when_latest_page_would_not_fill_chat() {
    let mut model = expanded_thread_model(1);

    model.maybe_schedule_chat_history_collapse();

    assert_eq!(
        model
            .chat
            .active_thread()
            .and_then(|thread| thread.collapse_deadline_tick),
        None,
        "collapsing to a tail that fits on screen would immediately re-request the same older page"
    );
}

#[test]
fn expanded_history_collapses_when_latest_page_still_fills_chat() {
    let mut model = expanded_thread_model(40);

    model.maybe_schedule_chat_history_collapse();

    assert!(
        model
            .chat
            .active_thread()
            .and_then(|thread| thread.collapse_deadline_tick)
            .is_some(),
        "a tail that still overflows the chat should keep the bounded-memory collapse"
    );
}

#[test]
fn scrolling_after_delete_backfill_uses_advanced_older_cursor() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 100;
    model.height = 40;
    model.config.tui_chat_history_page_size = 100;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            total_message_count: 170,
            loaded_message_start: 70,
            loaded_message_end: 170,
            messages: (70..170)
                .map(|index| chat::AgentMessage {
                    id: Some(format!("m{index}")),
                    role: chat::MessageRole::Assistant,
                    content: format!("Answer {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    for _ in 0..5 {
        let last_index = model
            .chat
            .active_thread()
            .map(|thread| thread.messages.len().saturating_sub(1))
            .expect("thread should remain");
        model.request_delete_message(last_index);
        model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::ChatActionConfirm,
        );
        let _ = cmd_rx.try_recv().expect("delete command");
    }
    match cmd_rx.try_recv().expect("threshold backfill request") {
        DaemonCommand::RequestThread {
            message_limit,
            message_offset,
            ..
        } => {
            assert_eq!(message_limit, Some(5));
            assert_eq!(message_offset, Some(100));
        }
        other => panic!("expected threshold backfill request, got {other:?}"),
    }

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        total_message_count: 165,
        loaded_message_start: 65,
        loaded_message_end: 70,
        messages: (65..70)
            .map(|index| crate::wire::AgentMessage {
                id: Some(format!("m{index}")),
                role: crate::wire::MessageRole::Assistant,
                content: format!("Answer {index}"),
                timestamp: index as u64,
                message_kind: "normal".to_string(),
                ..Default::default()
            })
            .collect(),
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));
    while cmd_rx.try_recv().is_ok() {}

    model
        .chat
        .reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));
    for _ in 0..chat::CHAT_HISTORY_FETCH_DEBOUNCE_TICKS {
        model.on_tick();
    }

    let mut next_request = None;
    while let Ok(command) = cmd_rx.try_recv() {
        if matches!(command, DaemonCommand::RequestThread { .. }) {
            next_request = Some(command);
            break;
        }
    }
    match next_request
        .expect("scrolling above the backfilled row should request the next older page")
    {
        DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_limit, Some(110));
            assert_eq!(message_offset, Some(105));
        }
        other => panic!("expected next older-page request, got {other:?}"),
    }
}

#[test]
fn clicking_cancel_in_chat_action_confirm_does_not_delete_message() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 100;
    model.height = 40;
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
            id: Some("m1".to_string()),
            role: chat::MessageRole::Assistant,
            content: "Answer".to_string(),
            ..Default::default()
        },
    });

    model.request_delete_message(0);
    let (_, overlay_area) = model
        .current_modal_area()
        .expect("chat action confirm modal should be visible");
    let (_, cancel_rect) = render_helpers::chat_action_confirm_button_bounds(overlay_area)
        .expect("confirm modal should expose button bounds");
    let click_col = cancel_rect.x.saturating_add(1);
    let click_row = cancel_rect.y;

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click_col,
        row: click_row,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click_col,
        row: click_row,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        model.modal.top(),
        None,
        "cancel click should close the modal"
    );
    assert_eq!(
        model
            .chat
            .active_thread()
            .map(|thread| thread.messages.len()),
        Some(1),
        "cancel click must not delete the message"
    );
    assert!(
        cmd_rx.try_recv().is_err(),
        "cancel click must not send a delete command"
    );
}
