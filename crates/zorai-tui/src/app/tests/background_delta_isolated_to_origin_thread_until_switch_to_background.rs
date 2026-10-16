use super::*;

fn make_model_with_daemon_rx_for_multithread_tests() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, cmd_rx) = unbounded_channel();
    (TuiModel::new(daemon_rx, cmd_tx), cmd_rx)
}

fn seed_two_visible_threads(model: &mut TuiModel) {
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
}

fn drain_daemon_commands(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Vec<DaemonCommand> {
    let mut commands = Vec::new();
    while let Ok(command) = daemon_rx.try_recv() {
        commands.push(command);
    }
    commands
}

fn render_plain_rows(model: &mut TuiModel) -> Vec<String> {
    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("render should succeed");

    let buffer = terminal.backend().buffer();
    (0..model.height)
        .map(|y| {
            (0..model.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect()
}

#[test]
fn active_thread_activity_uses_sticky_chat_row_not_input_placeholder() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model.set_active_thread_activity("thinking");

    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));
    assert!(
        model.actions_bar_visible(),
        "thread activity should reserve the sticky row above the composer"
    );

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let activity_row = input_start_row.saturating_sub(1) as usize;
    let input_rows = input_start_row as usize..model.height.saturating_sub(1) as usize;
    let rows = render_plain_rows(&mut model);
    let input_text = input_rows
        .map(|row| rows.get(row).cloned().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        rows.get(activity_row)
            .is_some_and(|row| row.contains("thinking")),
        "activity should render in the sticky chat row: {}",
        rows.join("\n")
    );
    assert!(
        !input_text.contains("thinking"),
        "activity should not render in the input composer placeholder: {input_text}"
    );
}

#[test]
fn sticky_thread_activity_uses_generic_phase_labels_with_top_padding() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.set_active_thread_activity("reasoning");
    model.tick_counter = 0;
    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let padding_row = input_start_row.saturating_sub(2) as usize;
    let activity_row = input_start_row.saturating_sub(1) as usize;
    let rows = render_plain_rows(&mut model);
    assert!(
        rows.get(padding_row)
            .is_some_and(|row| row.trim().is_empty()),
        "sticky activity should reserve a small top spacer: {}",
        rows.join("\n")
    );
    assert!(
        rows.get(activity_row)
            .is_some_and(|row| row.contains("thinking")),
        "reasoning activity should use a generic thinking label: {}",
        rows.join("\n")
    );

    model.set_active_thread_activity("⚙  fetch_gateway_history");
    model.tick_counter = 0;
    let rows = render_plain_rows(&mut model);
    assert!(
        rows.get(activity_row)
            .is_some_and(|row| row.contains("calling tools")),
        "tool activity should use a generic tools label: {}",
        rows.join("\n")
    );
    assert!(
        !rows.join("\n").contains("fetch_gateway_history"),
        "sticky activity should not duplicate the active tool name"
    );

    model.set_active_thread_activity("writing");
    model.tick_counter = 0;
    let rows = render_plain_rows(&mut model);
    assert!(
        rows.get(activity_row)
            .is_some_and(|row| row.contains("crafting")),
        "message delta activity should use a generic crafting label: {}",
        rows.join("\n")
    );
}

#[test]
fn sticky_thread_actions_include_current_thread_owner_label() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-user".to_string(),
            agent_name: Some("Weles".to_string()),
            title: "User Thread".to_string(),
            messages: vec![chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "Approve?".to_string(),
                actions: vec![chat::MessageAction {
                    label: "Approve".to_string(),
                    action_type: "approve".to_string(),
                    thread_id: Some("thread-user".to_string()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    assert!(
        model.actions_bar_visible(),
        "thread actions should reserve the sticky row above the composer"
    );

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let activity_row = input_start_row.saturating_sub(1) as usize;
    let rows = render_plain_rows(&mut model);

    assert!(
        rows.get(activity_row)
            .is_some_and(|row| row.contains("Weles [Approve]")),
        "sticky actions should include the current thread owner before buttons: {}",
        rows.join("\n")
    );
}

#[test]
fn sticky_thread_activity_phase_labels_rotate_during_same_event() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    let mut reasoning_labels = std::collections::BTreeSet::new();
    for tick in [0, 40, 80, 120] {
        model.tick_counter = tick;
        model.set_active_thread_activity("reasoning");
        let rows = render_plain_rows(&mut model);
        let activity_row = model
            .height
            .saturating_sub(model.input_height() + 1)
            .saturating_sub(1) as usize;
        reasoning_labels.insert(rows.get(activity_row).cloned().unwrap_or_default());
    }
    assert!(
        reasoning_labels.len() >= 4,
        "reasoning should rotate through several labels: {reasoning_labels:?}"
    );

    let mut tool_labels = std::collections::BTreeSet::new();
    for tick in [0, 40, 80, 120] {
        model.tick_counter = tick;
        model.set_active_thread_activity("⚙  fetch_gateway_history");
        let rows = render_plain_rows(&mut model);
        let activity_row = model
            .height
            .saturating_sub(model.input_height() + 1)
            .saturating_sub(1) as usize;
        tool_labels.insert(rows.get(activity_row).cloned().unwrap_or_default());
    }
    assert!(
        tool_labels.len() >= 4,
        "tool use should rotate through several labels: {tool_labels:?}"
    );

    let mut writing_labels = std::collections::BTreeSet::new();
    for tick in [0, 40, 80, 120] {
        model.tick_counter = tick;
        model.set_active_thread_activity("writing");
        let rows = render_plain_rows(&mut model);
        let activity_row = model
            .height
            .saturating_sub(model.input_height() + 1)
            .saturating_sub(1) as usize;
        writing_labels.insert(rows.get(activity_row).cloned().unwrap_or_default());
    }
    assert!(
        writing_labels.len() >= 4,
        "message output should rotate through several labels: {writing_labels:?}"
    );
}

#[test]
fn stale_done_before_first_stream_does_not_clear_pending_prompt_activity() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.submit_prompt("next question".to_string());

    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));
    assert!(
        model
            .pending_prompt_response_threads
            .contains("thread-user"),
        "submitted prompt should mark the thread as awaiting first response activity"
    );

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-user".to_string(),
        input_tokens: 100,
        output_tokens: 200,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,

        message_id: None,
    });

    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "a done event before first output should not clear pending prompt activity"
    );
    assert!(
        model
            .pending_prompt_response_threads
            .contains("thread-user"),
        "stale done should leave first-response pending state intact"
    );

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-user".to_string(),
        content: "new answer".to_string(),
    });

    assert_eq!(model.footer_activity_text().as_deref(), Some("writing"));
    assert!(
        !model
            .pending_prompt_response_threads
            .contains("thread-user"),
        "first output should consume the pending prompt marker"
    );
}

#[test]
fn background_delta_isolated_to_origin_thread_until_switch() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-other".to_string(),
        content: "background partial".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(model.footer_activity_text().is_none());
    assert_eq!(model.chat.streaming_content(), "");

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert_eq!(model.footer_activity_text().as_deref(), Some("writing"));
    assert_eq!(model.chat.streaming_content(), "background partial");
}

#[test]
fn pumping_daemon_events_stops_after_first_active_streaming_delta() {
    let (daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    seed_two_visible_threads(&mut model);

    daemon_tx
        .send(ClientEvent::Delta {
            thread_id: "thread-user".to_string(),
            content: "Hel".to_string(),
        })
        .expect("delta should enqueue");
    daemon_tx
        .send(ClientEvent::Delta {
            thread_id: "thread-user".to_string(),
            content: "lo".to_string(),
        })
        .expect("second delta should enqueue");
    daemon_tx
        .send(ClientEvent::Done {
            thread_id: "thread-user".to_string(),
            input_tokens: 1,
            output_tokens: 1,
            cost: None,
            provider: None,
            model: None,
            tps: None,
            generation_ms: None,
            reasoning: None,
            provider_final_result_json: None,

            message_id: None,
        })
        .expect("done should enqueue");

    let processed = model.pump_daemon_events_budgeted(32).processed;

    assert_eq!(
        processed, 1,
        "streaming frames should flush after the first delta"
    );
    assert_eq!(model.chat.streaming_content(), "Hel");

    let processed_after = model.pump_daemon_events_budgeted(32).processed;
    assert_eq!(
        processed_after, 1,
        "next frame should advance the stream incrementally"
    );
    assert_eq!(model.chat.streaming_content(), "Hello");
}

#[test]
fn pumping_daemon_events_does_not_pause_on_background_event_during_active_stream() {
    let (daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-user".to_string(),
        content: "Hel".to_string(),
    });

    daemon_tx
        .send(ClientEvent::Delta {
            thread_id: "thread-other".to_string(),
            content: "background".to_string(),
        })
        .expect("background delta should enqueue");
    daemon_tx
        .send(ClientEvent::Delta {
            thread_id: "thread-user".to_string(),
            content: "lo".to_string(),
        })
        .expect("foreground delta should enqueue");

    let processed = model.pump_daemon_events_budgeted(32).processed;

    assert_eq!(
        processed, 2,
        "background traffic should not prevent the next visible delta from being rendered"
    );
    assert_eq!(model.chat.streaming_content(), "Hello");
    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
}

#[test]
fn background_reasoning_isolated_to_origin_thread_until_switch() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::Reasoning {
        thread_id: "thread-other".to_string(),
        content: "background reasoning".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(model.footer_activity_text().is_none());
    assert_eq!(model.chat.streaming_reasoning(), "");

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert_eq!(model.footer_activity_text().as_deref(), Some("reasoning"));
    assert_eq!(model.chat.streaming_reasoning(), "background reasoning");
}

#[test]
fn background_tool_events_do_not_clear_selected_thread_activity() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-user".to_string(),
        call_id: "user-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,

        message_id: None,
    });
    assert_eq!(model.chat.active_tool_calls().len(), 1);

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-other".to_string(),
        call_id: "other-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"ls\"}".to_string(),
        weles_review: None,

        message_id: None,
    });
    model.handle_client_event(ClientEvent::ToolResult {
        thread_id: "thread-other".to_string(),
        call_id: "other-call".to_string(),
        name: "bash_command".to_string(),
        content: "Cargo.toml".to_string(),
        is_error: false,
        weles_review: None,

        message_id: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert_eq!(model.chat.active_tool_calls().len(), 1);
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("⚙  bash_command")
    );

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert!(!model.chat.has_running_tool_calls());
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("⚙  bash_command ✓")
    );
}

#[test]
fn background_retry_status_isolated_to_origin_thread_until_switch() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::RetryStatus {
        thread_id: "thread-other".to_string(),
        phase: "waiting".to_string(),
        attempt: 2,
        max_retries: 3,
        delay_ms: 5_000,
        failure_class: "provider".to_string(),
        message: "retrying after disconnect".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(model.footer_activity_text().is_none());
    assert!(model.chat.retry_status().is_none());

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert_eq!(model.footer_activity_text().as_deref(), Some("retry wait"));
    assert!(model.chat.retry_status().is_some());
}

#[test]
fn background_done_finalizes_origin_thread_without_clearing_selected_thread_busy_state() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);
    model.set_active_thread_activity("thinking");

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-other".to_string(),
        content: "background answer".to_string(),
    });
    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-other".to_string(),
        input_tokens: 11,
        output_tokens: 17,
        cost: None,
        provider: Some("openai".to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,

        message_id: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    let thread = model
        .chat
        .active_thread()
        .expect("background thread should exist");
    let last_message = thread
        .messages
        .last()
        .expect("done should append assistant");
    assert_eq!(last_message.content, "background answer");
    assert!(model.footer_activity_text().is_none());
}

#[test]
fn active_thread_reload_required_preserves_thinking_when_waiting_for_first_response() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx_for_multithread_tests();
    model.config.tui_chat_history_page_size = 123;
    model.connected = true;
    seed_two_visible_threads(&mut model);
    model.submit_prompt("inspect the race".to_string());
    drain_daemon_commands(&mut daemon_rx);

    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("thinking"),
        "reload before the first assistant event should preserve pending thinking"
    );
    assert!(
        model.assistant_busy(),
        "reload before the first assistant event should keep the thread marked busy"
    );

    let commands = drain_daemon_commands(&mut daemon_rx);
    assert!(
        commands.iter().any(|command| matches!(
            command,
            DaemonCommand::RequestThread {
                thread_id,
                message_limit: Some(123),
                message_offset: Some(0),
            } if thread_id == "thread-user"
        )),
        "reload should request fresh thread detail"
    );
    assert!(
        commands.iter().any(|command| matches!(
            command,
            DaemonCommand::RequestThreadTodos(thread_id) if thread_id == "thread-user"
        )),
        "reload should request fresh todos"
    );
    assert!(
        commands.iter().any(|command| matches!(
            command,
            DaemonCommand::RequestThreadWorkContext(thread_id) if thread_id == "thread-user"
        )),
        "reload should request fresh work context"
    );
}

#[test]
fn active_thread_reload_required_clears_stale_busy_state_without_pending_first_response() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx_for_multithread_tests();
    model.config.tui_chat_history_page_size = 123;
    model.connected = true;
    seed_two_visible_threads(&mut model);
    model.set_active_thread_activity("thinking");

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    assert!(
        model.footer_activity_text().is_none(),
        "reload fallback for the selected thread must clear stale busy UI"
    );
    assert!(
        !model.assistant_busy(),
        "reload fallback should not leave the selected thread busy forever"
    );

    let commands = drain_daemon_commands(&mut daemon_rx);
    assert!(
        commands.iter().any(|command| matches!(
            command,
            DaemonCommand::RequestThread {
                thread_id,
                message_limit: Some(123),
                message_offset: Some(0),
            } if thread_id == "thread-user"
        )),
        "reload should request fresh thread detail"
    );
}

#[test]
fn background_workflow_notice_isolated_to_origin_thread_until_switch() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);
    model.set_active_thread_activity("thinking");

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-other".to_string()),
        kind: "skill-gate".to_string(),
        message: "background skill gate".to_string(),
        details: Some(r#"{"recommended_skill":"onecontext"}"#.to_string()),
    });

    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));

    assert_eq!(model.footer_activity_text().as_deref(), Some("skill gate"));
}

#[test]
fn background_thread_created_does_not_copy_or_select_active_transcript() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-user".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "current thread answer".to_string(),
            ..Default::default()
        },
    });

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-background".to_string(),
        title: "Background Thread".to_string(),
        agent_name: Some("DeepSeekorrr".to_string()),
    });

    assert_eq!(
        model.chat.active_thread_id(),
        Some("thread-user"),
        "background thread creation must not steal the visible conversation"
    );

    let background = model
        .chat
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-background")
        .expect("background thread should be tracked");
    assert!(
        background.messages.is_empty(),
        "background thread must not inherit messages from the active transcript"
    );
}

#[test]
fn cached_chat_render_does_not_reuse_previous_thread_after_spawned_navigation() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-root".to_string(),
        title: "Root Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-child".to_string(),
        title: "Child Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-root".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "root-only transcript".to_string(),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-child".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "child-only transcript".to_string(),
            ..Default::default()
        },
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-root".to_string()));

    let root_rows = render_plain_rows(&mut model).join("\n");
    assert!(root_rows.contains("root-only transcript"));

    assert!(model
        .chat
        .open_spawned_thread("thread-root", "thread-child"));

    let child_rows = render_plain_rows(&mut model).join("\n");
    assert!(
        child_rows.contains("child-only transcript"),
        "chat render cache must rebuild after active thread changes"
    );
    assert!(
        !child_rows.contains("root-only transcript"),
        "chat render cache must not reuse rows from the previous active thread"
    );
}

#[test]
fn paginated_thread_list_refresh_preserves_selected_thread_missing_from_page() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-running".to_string(),
        title: "Running Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-selected".to_string(),
        title: "Selected Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-selected".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "selected-only transcript".to_string(),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-selected".to_string(),
    ));

    model.handle_client_event(ClientEvent::ThreadList(vec![crate::wire::AgentThread {
        id: "thread-running".to_string(),
        title: "Running Thread".to_string(),
        updated_at: 200,
        ..Default::default()
    }]));

    assert_eq!(
        model.chat.active_thread_id(),
        Some("thread-selected"),
        "a paginated list page must not clear the selected thread just because it is not present"
    );

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-running".to_string(),
        title: "Running Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "running-only transcript".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        total_message_count: 1,
        loaded_message_start: 0,
        loaded_message_end: 1,
        ..Default::default()
    });

    assert_eq!(
        model.chat.active_thread_id(),
        Some("thread-selected"),
        "a later detail response for another thread must not take over the selected thread"
    );
    let selected = model
        .chat
        .active_thread()
        .expect("selected thread should still be active");
    assert!(
        selected
            .messages
            .iter()
            .any(|message| message.content == "selected-only transcript"),
        "selected thread content should remain attached to the selected thread"
    );
}

#[test]
fn paginated_thread_list_refresh_preserves_other_loaded_threads() {
    let mut model = build_model();
    for thread_id in ["thread-running", "thread-selected", "thread-other"] {
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: thread_id.to_string(),
            title: thread_id.to_string(),
        });
    }
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-other".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "other-only transcript".to_string(),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-selected".to_string(),
    ));

    model.handle_client_event(ClientEvent::ThreadList(vec![crate::wire::AgentThread {
        id: "thread-running".to_string(),
        title: "Running Thread".to_string(),
        updated_at: 200,
        ..Default::default()
    }]));

    let other = model
        .chat
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-other")
        .expect("loaded threads missing from a paginated refresh page should remain cached");
    assert!(
        other
            .messages
            .iter()
            .any(|message| message.content == "other-only transcript"),
        "non-active cached thread content should not vanish on a partial list refresh"
    );
}

#[test]
fn late_skill_scout_notice_after_done_does_not_restore_footer_activity() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-user".to_string(),
        content: "finished answer".to_string(),
    });
    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-user".to_string(),
        input_tokens: 1,
        output_tokens: 2,
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
        "done should leave the active thread idle"
    );

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-user".to_string()),
        kind: "skill-community-scout".to_string(),
        message: "Community scout update".to_string(),
        details: Some(r#"{"candidates":["skill-a"]}"#.to_string()),
    });

    assert!(
        model.footer_activity_text().is_none(),
        "late skill scout notices for completed turns must not wake the footer"
    );
}

#[test]
fn background_todos_and_work_context_stay_attached_to_origin_thread() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);

    model.handle_client_event(ClientEvent::ThreadTodos {
        thread_id: "thread-other".to_string(),
        goal_run_id: None,
        step_index: None,
        items: vec![crate::wire::TodoItem {
            id: "todo-1".to_string(),
            content: "Check routed done event".to_string(),
            status: Some(crate::wire::TodoStatus::InProgress),
            position: 0,
            step_index: None,
            created_at: 1,
            updated_at: 1,
        }],
    });
    model.handle_client_event(ClientEvent::WorkContext(crate::wire::ThreadWorkContext {
        thread_id: "thread-other".to_string(),
        entries: vec![crate::wire::WorkContextEntry {
            path: "crates/zorai-tui/src/app/events.rs".to_string(),
            kind: Some(crate::wire::WorkContextEntryKind::RepoChange),
            source: "test".to_string(),
            is_text: true,
            updated_at: 1,
            ..Default::default()
        }],
    }));

    assert!(model.tasks.todos_for_thread("thread-user").is_empty());
    assert_eq!(model.tasks.todos_for_thread("thread-other").len(), 1);
    assert_eq!(
        model
            .tasks
            .work_context_for_thread("thread-other")
            .expect("background work context should be stored")
            .entries
            .len(),
        1
    );
}

#[test]
fn background_participant_suggestion_keeps_selected_thread_activity_and_origin_thread_binding() {
    let mut model = build_model();
    seed_two_visible_threads(&mut model);
    model.set_active_thread_activity("thinking");

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-other".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "suggestion-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check the race".to_string(),
            suggestion_kind: "prepared_message".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            auto_send_at: None,
            source_message_timestamp: None,
            error: None,
        },
    });

    assert_eq!(model.footer_activity_text().as_deref(), Some("thinking"));
    assert_eq!(model.queued_prompts.len(), 1);
    assert_eq!(
        model.queued_prompts[0].thread_id.as_deref(),
        Some("thread-other")
    );
    assert_eq!(
        model.queued_prompts[0].participant_agent_name.as_deref(),
        Some("Weles")
    );
}

#[test]
fn spawned_thread_keeps_back_to_parent_link_after_closing_a_preview() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    for (thread_id, title) in [("thread-root", "Root Thread"), ("thread-child", "Child Thread")] {
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: thread_id.to_string(),
            title: title.to_string(),
        });
    }
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-root".to_string()));
    assert!(model
        .chat
        .open_spawned_thread("thread-root", "thread-child"));
    model.set_mission_control_return_targets(None, Some("thread-root".to_string()));
    model.main_pane_view = MainPaneView::Conversation;

    model.open_work_context_for_thread(
        "thread-child".to_string(),
        None,
        Some("thread-child".to_string()),
        None,
        "Todo details".to_string(),
    );
    assert!(model.dismiss_active_main_pane(FocusArea::Chat));

    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.chat.active_thread_id(), Some("thread-child"));
    assert_eq!(
        model.mission_control_return_to_thread_id().as_deref(),
        Some("thread-root"),
        "returning from a preview must keep the child's way back to its parent"
    );
}
