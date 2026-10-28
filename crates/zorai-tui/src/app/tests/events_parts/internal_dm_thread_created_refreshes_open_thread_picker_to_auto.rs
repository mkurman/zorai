use super::idle_tick_does_not_request_redraw_to_first_raw_config_load_triggers::*;
use crate::app::*;
#[test]
fn internal_dm_thread_created_refreshes_open_thread_picker() {
    let mut model = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Internal);
    model.sync_thread_picker_item_count();

    assert!(
        model.selected_thread_picker_thread().is_none(),
        "internal picker should start empty in this fixture"
    );

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "dm:svarog:weles".into(),
        title: "Internal DM · Swarog ↔ WELES".into(),
        agent_name: None,
    });

    model.modal.reduce(modal::ModalAction::Navigate(1));

    assert_eq!(
        model
            .selected_thread_picker_thread()
            .map(|thread| thread.id.as_str()),
        Some("dm:svarog:weles")
    );
}

#[test]
fn goal_run_deleted_event_removes_goal_from_task_state() {
    let mut model = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![
            task::GoalRun {
                id: "goal-1".into(),
                title: "Goal One".into(),
                status: Some(task::GoalRunStatus::Cancelled),
                ..Default::default()
            },
            task::GoalRun {
                id: "goal-2".into(),
                title: "Goal Two".into(),
                status: Some(task::GoalRunStatus::Completed),
                ..Default::default()
            },
        ]));

    model.handle_client_event(ClientEvent::GoalRunDeleted {
        goal_run_id: "goal-1".into(),
        deleted: true,
    });

    assert!(model.tasks.goal_run_by_id("goal-1").is_none());
    assert!(model.tasks.goal_run_by_id("goal-2").is_some());
}

#[test]
fn goal_run_deleted_event_reclamps_open_goal_picker_cursor() {
    let mut model = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![
            task::GoalRun {
                id: "goal-1".into(),
                title: "Goal One".into(),
                status: Some(task::GoalRunStatus::Completed),
                ..Default::default()
            },
            task::GoalRun {
                id: "goal-2".into(),
                title: "Goal Two".into(),
                status: Some(task::GoalRunStatus::Cancelled),
                ..Default::default()
            },
        ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(2));

    assert_eq!(model.modal.picker_cursor(), 2);
    assert_eq!(
        model.selected_goal_picker_run().map(|run| run.id.as_str()),
        Some("goal-2")
    );

    model.handle_client_event(ClientEvent::GoalRunDeleted {
        goal_run_id: "goal-2".into(),
        deleted: true,
    });

    assert_eq!(model.modal.picker_cursor(), 1);
    assert_eq!(
        model.selected_goal_picker_run().map(|run| run.id.as_str()),
        Some("goal-1")
    );
}

#[test]
fn goal_run_list_event_reclamps_open_goal_picker_cursor() {
    let mut model = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![
            task::GoalRun {
                id: "goal-1".into(),
                title: "Goal One".into(),
                status: Some(task::GoalRunStatus::Completed),
                ..Default::default()
            },
            task::GoalRun {
                id: "goal-2".into(),
                title: "Goal Two".into(),
                status: Some(task::GoalRunStatus::Cancelled),
                ..Default::default()
            },
        ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(2));

    assert_eq!(model.modal.picker_cursor(), 2);
    assert_eq!(
        model.selected_goal_picker_run().map(|run| run.id.as_str()),
        Some("goal-2")
    );

    model.handle_client_event(ClientEvent::GoalRunList(vec![crate::wire::GoalRun {
        id: "goal-1".into(),
        title: "Goal One".into(),
        status: Some(crate::wire::GoalRunStatus::Completed),
        goal: "Goal One".into(),
        ..Default::default()
    }]));

    assert_eq!(model.modal.picker_cursor(), 1);
    assert_eq!(
        model.selected_goal_picker_run().map(|run| run.id.as_str()),
        Some("goal-1")
    );
}

#[test]
fn operator_question_resolved_event_marks_message_answered_and_clears_actions() {
    let mut model = make_model();
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
            content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
            is_operator_question: true,
            operator_question_id: Some("oq-1".to_string()),
            actions: vec![chat::MessageAction {
                label: "A".to_string(),
                action_type: "operator_question_answer:oq-1:A".to_string(),
                thread_id: Some("thread-1".to_string()),
            }],
            ..Default::default()
        },
    });

    model.handle_client_event(ClientEvent::OperatorQuestionResolved {
        question_id: "oq-1".to_string(),
        answer: "A".to_string(),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert_eq!(thread.messages.len(), 1);
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert_eq!(message.operator_question_answer.as_deref(), Some("A"));
    assert!(message.actions.is_empty());
}

#[test]
fn operator_question_event_does_not_replace_existing_modal() {
    let mut model = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
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
    assert_eq!(message.actions.len(), 2);
    assert_eq!(message.actions[0].label, "A");
    assert_eq!(message.actions[1].label, "B");
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));
    model.modal.reduce(modal::ModalAction::Pop);
    assert_eq!(model.modal.top(), None);
    assert_ne!(
        model.modal.top(),
        Some(modal::ModalKind::OperatorQuestionOverlay)
    );
}

#[test]
fn operator_question_event_preserves_locked_chat_viewport() {
    let mut model = make_model();
    model.width = 100;
    model.height = 40;
    model.show_sidebar_override = Some(false);
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    for idx in 0..40 {
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "thread-1".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: format!("message {idx}"),
                ..Default::default()
            },
        });
    }
    model.chat.reduce(chat::ChatAction::ScrollChat(8));

    let before = widgets::chat::scrollbar_layout(
        model.pane_layout().chat,
        &model.chat,
        &model.theme,
        model.tick_counter,
        model.retry_wait_start_selected,
    )
    .expect("chat should produce a scrollbar layout");

    model.handle_operator_question_event(
        "oq-1".to_string(),
        "Approve this slice?\nA - proceed\nB - revise".to_string(),
        vec!["A".to_string(), "B".to_string()],
        Some("thread-1".to_string()),
    );

    let after = widgets::chat::scrollbar_layout(
        model.pane_layout().chat,
        &model.chat,
        &model.theme,
        model.tick_counter,
        model.retry_wait_start_selected,
    )
    .expect("chat should still produce a scrollbar layout");

    assert!(
        before.scroll > 0,
        "test setup should scroll away from the live bottom"
    );
    assert_eq!(
        after.scroll as isize - before.scroll as isize,
        after.max_scroll as isize - before.max_scroll as isize,
        "locked viewport should stay anchored when a new message extends the transcript"
    );
}

#[test]
fn operator_question_event_keeps_bottom_follow_when_chat_is_at_latest_message() {
    let mut model = make_model();
    model.width = 100;
    model.height = 40;
    model.show_sidebar_override = Some(false);
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    for idx in 0..40 {
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "thread-1".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: format!("message {idx}"),
                ..Default::default()
            },
        });
    }

    model.handle_operator_question_event(
        "oq-1".to_string(),
        "Approve this slice?\nA - proceed\nB - revise".to_string(),
        vec!["A".to_string(), "B".to_string()],
        Some("thread-1".to_string()),
    );

    let after = widgets::chat::scrollbar_layout(
        model.pane_layout().chat,
        &model.chat,
        &model.theme,
        model.tick_counter,
        model.retry_wait_start_selected,
    )
    .expect("chat should produce a scrollbar layout");

    assert_eq!(
        after.scroll, 0,
        "when already at the live bottom, new messages should continue following the stack"
    );
}

#[test]
fn soft_wrapped_streaming_delta_preserves_locked_chat_viewport() {
    let mut model = make_model();
    model.width = 100;
    model.height = 40;
    model.show_sidebar_override = Some(false);
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    for idx in 0..40 {
        model.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id: "thread-1".to_string(),
            message: chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: format!("message {idx}"),
                ..Default::default()
            },
        });
    }
    model.chat.reduce(chat::ChatAction::ScrollChat(8));

    let before = widgets::chat::scrollbar_layout(
        model.pane_layout().chat,
        &model.chat,
        &model.theme,
        model.tick_counter,
        model.retry_wait_start_selected,
    )
    .expect("chat should produce a scrollbar layout");

    model.handle_delta_event("thread-1".to_string(), " long streamed token".repeat(80));

    let after = widgets::chat::scrollbar_layout(
        model.pane_layout().chat,
        &model.chat,
        &model.theme,
        model.tick_counter,
        model.retry_wait_start_selected,
    )
    .expect("chat should still produce a scrollbar layout");

    assert!(
        before.scroll > 0,
        "test setup should scroll away from the live bottom"
    );
    assert_eq!(
        after.scroll as isize - before.scroll as isize,
        after.max_scroll as isize - before.max_scroll as isize,
        "locked viewport should stay anchored even when streamed text adds soft-wrapped rows"
    );
}

#[test]
fn operator_profile_workflow_warning_surfaces_retry_notice() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: None,
        kind: "operator-profile-warning".to_string(),
        message: "Operator profile operation failed".to_string(),
        details: Some("{\"retry_action\":\"request_concierge_welcome\"}".to_string()),
    });
    let rendered = model
        .input_notice_style()
        .expect("warning should be visible");
    assert!(
        rendered.0.contains("Ctrl+R"),
        "warning notice should include retry hint"
    );
}

#[test]
fn auto_compaction_workflow_notice_requests_active_compaction_window() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-compaction".to_string(),
        title: "Compaction".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-compaction".to_string(),
    ));

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-compaction".to_string()),
        kind: "auto-compaction".to_string(),
        message: "Auto compaction applied using heuristic.".to_string(),
        details: Some("{\"split_at\":20,\"total_message_count\":121}".to_string()),
    });

    let (thread_id, message_limit, message_offset) =
        next_thread_request(&mut daemon_rx).expect("expected targeted thread reload request");
    assert_eq!(thread_id, "thread-compaction");
    assert_eq!(message_limit, Some(101));
    assert_eq!(message_offset, Some(0));
}

#[test]
fn auto_compaction_workflow_notice_applies_post_compaction_tokens_when_present() {
    // Why this matters: the user reported the header staying at 100% after
    // compaction. The pre-compaction value was already clamped (oversized
    // history → over the model window → clamps to 100%), and we used to either
    // null tokens (causing a 0% flash) or preserve the stale clamped value
    // (causing the 100% stuck-state the user saw). The daemon now publishes
    // post_compaction_total_tokens in the WorkflowNotice details so the TUI
    // can drop the header to the new value immediately, without waiting for
    // the separate ContextWindowUpdate event.
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-compaction".to_string(),
        title: "Compaction".to_string(),
        total_message_count: 121,
        loaded_message_start: 0,
        loaded_message_end: 121,
        active_context_window_start: Some(0),
        active_context_window_end: Some(121),
        active_context_window_tokens: Some(239_700_000),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "before compaction".to_string(),
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-compaction".to_string(),
    ));
    while daemon_rx.try_recv().is_ok() {}

    assert_eq!(
        model.current_header_usage_summary().current_tokens,
        239_700_000
    );

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-compaction".to_string()),
        kind: "auto-compaction".to_string(),
        message: "Auto compaction applied using heuristic.".to_string(),
        details: Some(
            "{\"split_at\":20,\"total_message_count\":122,\"post_compaction_total_tokens\":4800,\"post_compaction_window_start\":20,\"post_compaction_window_end\":122}"
                .to_string(),
        ),
    });

    assert_eq!(
        model.current_header_usage_summary().current_tokens,
        4_800,
        "header must drop to post-compaction tokens immediately when the daemon includes them in the notice details"
    );
    assert!(matches!(
        next_thread_request(&mut daemon_rx),
        Some((thread_id, Some(102), Some(0))) if thread_id == "thread-compaction"
    ));
}

#[test]
fn auto_compaction_workflow_notice_keeps_last_known_tokens_when_post_compaction_missing() {
    // Backwards-compatibility safety net: if the daemon ever omits the
    // post-compaction token data (older daemon, race), the TUI must keep the
    // last-known value rather than null it — that was the previous 0%-flash bug.
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-compaction".to_string(),
        title: "Compaction".to_string(),
        total_message_count: 121,
        loaded_message_start: 0,
        loaded_message_end: 121,
        active_context_window_start: Some(0),
        active_context_window_end: Some(121),
        active_context_window_tokens: Some(239_700_000),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "before compaction".to_string(),
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-compaction".to_string(),
    ));
    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-compaction".to_string()),
        kind: "auto-compaction".to_string(),
        message: "Auto compaction applied using heuristic.".to_string(),
        details: Some("{\"split_at\":20,\"total_message_count\":121}".to_string()),
    });

    assert_eq!(
        model.current_header_usage_summary().current_tokens,
        239_700_000,
        "without post-compaction data in details, the TUI must preserve the last-known value rather than null it"
    );
    assert!(matches!(
        next_thread_request(&mut daemon_rx),
        Some((thread_id, Some(101), Some(0))) if thread_id == "thread-compaction"
    ));
}

#[test]
fn replayed_operator_question_after_each_thread_load_shows_once() {
    let mut model = make_model();
    let wire_thread = || crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        total_message_count: 2,
        loaded_message_start: 0,
        loaded_message_end: 2,
        messages: (0..2)
            .map(|index| crate::wire::AgentMessage {
                id: Some(format!("m{index}")),
                role: crate::wire::MessageRole::Assistant,
                content: format!("Answer {index}"),
                timestamp: index as u64 + 1,
                message_kind: "normal".to_string(),
                ..Default::default()
            })
            .collect(),
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    };
    let question = || ClientEvent::OperatorQuestion {
        question_id: "oq-1".to_string(),
        content: "Which event?\nA - Meta\nB - other".to_string(),
        options: vec!["A".to_string(), "B".to_string()],
        session_id: None,
        thread_id: Some("thread-1".to_string()),
    };
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    for _ in 0..2 {
        model.handle_client_event(ClientEvent::ThreadDetail(Some(wire_thread())));
        model.handle_client_event(question());
    }

    let thread = model.chat.active_thread().expect("thread should exist");
    let questions = thread
        .messages
        .iter()
        .filter(|message| message.operator_question_id.as_deref() == Some("oq-1"))
        .collect::<Vec<_>>();
    assert_eq!(
        questions.len(),
        1,
        "the daemon replays unanswered questions on every thread load, so the TUI must dedupe them"
    );
    assert_eq!(
        questions[0].actions.len(),
        2,
        "the replayed question must stay answerable"
    );
}
