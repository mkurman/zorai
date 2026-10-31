use super::*;
use std::fs;
use crate::app::tests::goal_sidebar_tab_cycling_stays_to_collaboration_mouse_clicks_select_rows::goal_sidebar_tab_cycling_stays_mod::*;
use super::super::{build_model, unbounded_channel};
use ratatui::backend::TestBackend;
use std::sync::mpsc;
#[test]
fn submit_operator_profile_answer_allows_empty_input_when_question_is_optional() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "nickname".to_string(),
        field_key: "nickname".to_string(),
        prompt: "Nickname?".to_string(),
        input_kind: "text".to_string(),
        optional: true,
    });

    assert!(model.submit_operator_profile_answer());
    assert!(
        model.operator_profile.loading,
        "optional empty answer should begin submission"
    );
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when submission starts"
    );

    let sent = cmd_rx
        .try_recv()
        .expect("submitting optional empty answer should emit daemon command");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "nickname");
            assert_eq!(answer_json, "null");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn submit_operator_profile_answer_blocks_empty_input_when_question_is_required() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.submit_operator_profile_answer());
    assert!(
        !model.operator_profile.loading,
        "required empty answer should not start submission"
    );
    assert!(
        model.operator_profile.question.is_some(),
        "question should remain while awaiting required answer"
    );
    assert!(
        cmd_rx.try_recv().is_err(),
        "required empty answer should not emit daemon command"
    );
}

#[test]
fn submit_operator_profile_answer_serializes_daemon_boolean_yes_as_true() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "enabled".to_string(),
        field_key: "enabled".to_string(),
        prompt: "Enable operator modeling overall?".to_string(),
        input_kind: "boolean".to_string(),
        optional: false,
    });
    model.input.set_text("yes");

    assert!(model.submit_operator_profile_answer());

    let sent = cmd_rx
        .try_recv()
        .expect("submitting boolean answer should emit daemon command");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "enabled");
            assert_eq!(answer_json, "true");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn keyboard_shortcut_submits_operator_profile_boolean_choice_without_free_text() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "enabled".to_string(),
        field_key: "enabled".to_string(),
        prompt: "Enable operator modeling overall?".to_string(),
        input_kind: "boolean".to_string(),
        optional: false,
    });
    model.operator_profile.bool_answer = Some(true);
    model.focus = FocusArea::Chat;
    model.open_operator_profile_onboarding_modal();

    assert!(!model.handle_key(KeyCode::Char('n'), KeyModifiers::NONE));
    assert_eq!(model.input.buffer(), "");
    let sent = cmd_rx
        .try_recv()
        .expect("n should submit the boolean answer");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer { answer_json, .. } => {
            assert_eq!(answer_json, "false");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn operator_profile_boolean_choice_ignores_stale_held_control_modifier() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "enabled".to_string(),
        field_key: "enabled".to_string(),
        prompt: "Enable operator modeling overall?".to_string(),
        input_kind: "boolean".to_string(),
        optional: false,
    });
    model.operator_profile.bool_answer = Some(true);
    model.focus = FocusArea::Chat;
    model.open_operator_profile_onboarding_modal();

    model.handle_key(
        KeyCode::Modifier(ModifierKeyCode::LeftControl),
        KeyModifiers::NONE,
    );

    assert!(!model.handle_key(KeyCode::Char('n'), KeyModifiers::NONE));
    let sent = cmd_rx
        .try_recv()
        .expect("n should submit even after stale control state");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer { answer_json, .. } => {
            assert_eq!(answer_json, "false");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn enter_submits_operator_profile_boolean_choice_from_chat_focus() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "enabled".to_string(),
        field_key: "enabled".to_string(),
        prompt: "Enable operator modeling overall?".to_string(),
        input_kind: "boolean".to_string(),
        optional: false,
    });
    model.operator_profile.bool_answer = Some(true);
    model.focus = FocusArea::Chat;
    model.open_operator_profile_onboarding_modal();

    assert!(!model.handle_key(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(model.modal.picker_cursor(), 1);
    assert_eq!(model.input.buffer(), "");

    assert!(!model.handle_key(KeyCode::Enter, KeyModifiers::NONE));

    let sent = cmd_rx
        .try_recv()
        .expect("enter should submit selected boolean answer");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer { answer_json, .. } => {
            assert_eq!(answer_json, "false");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn arrows_move_operator_profile_onboarding_selection() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "enabled".to_string(),
        field_key: "enabled".to_string(),
        prompt: "Enable operator modeling overall?".to_string(),
        input_kind: "boolean".to_string(),
        optional: false,
    });
    model.open_operator_profile_onboarding_modal();

    assert_eq!(model.modal.picker_cursor(), 0);
    assert!(!model.handle_key(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(model.modal.picker_cursor(), 1);
    assert!(!model.handle_key(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(model.modal.picker_cursor(), 2);
    assert!(!model.handle_key(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(model.modal.picker_cursor(), 1);
    assert!(!model.handle_key(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(model.modal.picker_cursor(), 0);
}

#[test]
fn reopening_operator_profile_onboarding_preserves_selection() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "enabled".to_string(),
        field_key: "enabled".to_string(),
        prompt: "Enable operator modeling overall?".to_string(),
        input_kind: "boolean".to_string(),
        optional: false,
    });
    model.open_operator_profile_onboarding_modal();
    model.handle_key(KeyCode::Down, KeyModifiers::NONE);

    assert_eq!(model.modal.picker_cursor(), 1);

    model.open_operator_profile_onboarding_modal();

    assert_eq!(model.modal.picker_cursor(), 1);
}

#[test]
fn esc_defers_operator_profile_question() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "enabled".to_string(),
        field_key: "enabled".to_string(),
        prompt: "Enable operator modeling overall?".to_string(),
        input_kind: "boolean".to_string(),
        optional: false,
    });
    model.open_operator_profile_onboarding_modal();

    assert!(!model.handle_key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!model.operator_profile.visible);
    assert_eq!(model.modal.top(), None);

    let sent = cmd_rx
        .try_recv()
        .expect("esc should defer the active onboarding question");
    match sent {
        DaemonCommand::DeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "enabled");
            assert!(defer_until_unix_ms.is_some());
        }
        other => panic!("unexpected command: {other:?}"),
    }

    model.handle_client_event(ClientEvent::OperatorProfileProgress {
        session_id: "sess-1".to_string(),
        answered: 1,
        remaining: 4,
        completion_ratio: 0.2,
    });
    assert!(!model.operator_profile.visible);
    assert_eq!(model.modal.top(), None);
}

#[test]
fn operator_profile_onboarding_keeps_existing_welcome_message_visible() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 120;
    model.height = 36;
    model.connected = true;
    model.agent_config_loaded = true;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Conversation;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Welcome".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Welcome to Zorai onboarding".to_string(),
            ..Default::default()
        },
    });
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "enabled".to_string(),
        field_key: "enabled".to_string(),
        prompt: "Enable operator modeling overall?".to_string(),
        input_kind: "boolean".to_string(),
        optional: false,
    });
    model.operator_profile.bool_answer = Some(true);
    model.open_operator_profile_onboarding_modal();

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("operator profile render should succeed");

    let buffer = terminal.backend().buffer();
    let rendered = (0..model.height)
        .map(|y| {
            (0..model.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Welcome to Zorai onboarding"));
    assert!(rendered.contains("Operator Profile"));
}

#[test]
fn mouse_click_on_operator_profile_boolean_choice_submits() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 120;
    model.height = 36;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Conversation;
    model.operator_profile.visible = true;
    model.operator_profile.session_kind = Some("first_run_onboarding".to_string());
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "enabled".to_string(),
        field_key: "enabled".to_string(),
        prompt: "Enable operator modeling overall?".to_string(),
        input_kind: "boolean".to_string(),
        optional: false,
    });
    model.operator_profile.bool_answer = Some(true);
    model.open_operator_profile_onboarding_modal();

    let (_, conversation_area) = model
        .current_modal_area()
        .expect("operator profile modal area should be available");
    let view = model.operator_profile_onboarding_view();
    let regions = widgets::operator_profile_onboarding::hit_test_regions(conversation_area, &view)
        .expect("onboarding should expose clickable regions");
    let no_region = regions
        .iter()
        .find(|(target, _)| {
            *target
                == widgets::operator_profile_onboarding::OperatorProfileOnboardingHitTarget::BoolChoice(
                    false,
                )
        })
        .map(|(_, region)| *region)
        .expect("no button should be clickable");
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: no_region.x,
        row: no_region.y,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(model.input.buffer(), "");

    let sent = cmd_rx
        .try_recv()
        .expect("clicking no should emit daemon command");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "enabled");
            assert_eq!(answer_json, "false");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn skip_operator_profile_question_clears_stale_question_immediately() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.skip_operator_profile_question());
    assert!(model.operator_profile.loading);
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when skip starts"
    );

    let sent = cmd_rx.try_recv().expect("skip should emit daemon command");
    match sent {
        DaemonCommand::SkipOperatorProfileQuestion {
            session_id,
            question_id,
            reason,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "name");
            assert_eq!(reason.as_deref(), Some("tui_skip_shortcut"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn defer_operator_profile_question_clears_stale_question_immediately() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.defer_operator_profile_question());
    assert!(!model.operator_profile.visible);
    assert!(!model.operator_profile.loading);
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when defer closes onboarding"
    );
    assert_eq!(model.modal.top(), None);

    let sent = cmd_rx.try_recv().expect("defer should emit daemon command");
    match sent {
        DaemonCommand::DeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "name");
            assert!(defer_until_unix_ms.is_some());
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn clicking_bottom_action_bar_submits_operator_question_answer() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
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
    model.chat.select_message(Some(0));

    let concierge_area = model.pane_layout().concierge;
    let action_pos = (concierge_area.y..concierge_area.y.saturating_add(concierge_area.height))
        .find_map(|row| {
            (concierge_area.x..concierge_area.x.saturating_add(concierge_area.width)).find_map(
                |column| {
                    let pos = Position::new(column, row);
                    if widgets::concierge::hit_test(
                        concierge_area,
                        model.chat.active_actions(),
                        model.concierge.selected_action,
                        Some(
                            model
                                .current_conversation_agent_profile()
                                .agent_label
                                .as_str(),
                        ),
                        pos,
                    ) == Some(widgets::concierge::ConciergeHitTarget::Action(0))
                    {
                        Some(pos)
                    } else {
                        None
                    }
                },
            )
        })
        .expect("operator question should expose a clickable concierge action bar");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: action_pos.x,
        row: action_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: action_pos.x,
        row: action_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    let sent = cmd_rx
        .try_recv()
        .expect("clicking the action bar should answer the operator question");
    match sent {
        DaemonCommand::AnswerOperatorQuestion {
            question_id,
            answer,
        } => {
            assert_eq!(question_id, "oq-1");
            assert_eq!(answer, "A");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn operator_question_other_sends_the_typed_text_as_the_answer() {
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
    model.handle_client_event(ClientEvent::OperatorQuestion {
        question_id: "oq-1".to_string(),
        content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
        options: vec!["A".to_string(), "B".to_string()],
        session_id: None,
        thread_id: Some("thread-1".to_string()),
    });
    let labels = model
        .chat
        .active_thread()
        .and_then(|thread| thread.messages.last())
        .map(|message| {
            message
                .actions
                .iter()
                .map(|action| action.label.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(labels, vec!["A", "B", "Other"]);

    model.execute_concierge_message_action(0, 2);
    assert!(
        cmd_rx.try_recv().is_err(),
        "choosing Other must wait for the typed answer"
    );
    model.submit_prompt("neither, split it first".to_string());

    match cmd_rx.try_recv().expect("typed answer should be sent") {
        DaemonCommand::AnswerOperatorQuestion {
            question_id,
            answer,
        } => {
            assert_eq!(question_id, "oq-1");
            assert_eq!(answer, "neither, split it first");
        }
        other => {
            panic!("the typed text must answer the question, not start a new prompt: {other:?}")
        }
    }
    let message = model
        .chat
        .active_thread()
        .and_then(|thread| thread.messages.last())
        .expect("question message should exist");
    assert_eq!(
        message.operator_question_answer.as_deref(),
        Some("neither, split it first")
    );
}

#[test]
fn tab_completes_active_file_reference_instead_of_changing_focus() {
    let mut model = build_model();
    let cwd = make_temp_dir();
    let docs_dir = cwd.join("docs");
    fs::create_dir_all(&docs_dir).expect("docs directory should be creatable");
    fs::write(docs_dir.join("notes.txt"), "hello").expect("file should be writable");
    let reference = format!("@{}/do", cwd.display());
    model.input.set_text(&reference);

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Input);
    assert_eq!(model.input.buffer(), format!("@{}/docs/", cwd.display()));
}

#[test]
fn tab_focus_cycles_when_not_inside_file_reference() {
    let mut model = build_model();
    model.focus = FocusArea::Input;
    model.input.set_text("hello world");

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(model.input.buffer(), "hello world");
}

#[test]
fn tab_inside_unmatched_file_reference_keeps_input_focus() {
    let mut model = build_model();
    let cwd = make_temp_dir();
    let _guard = CurrentDirGuard::enter(&cwd);
    model.focus = FocusArea::Input;
    model.input.set_text("@missing");

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Input);
    assert_eq!(model.input.buffer(), "@missing");
    assert!(
        model.status_line.contains("No matches"),
        "unmatched completion should surface a notice"
    );
}

#[test]
fn leading_agent_directive_supports_internal_delegate() {
    let known = vec!["weles".to_string()];
    let directive =
        crate::state::input_refs::parse_leading_agent_directive("!weles check X", &known)
            .expect("directive should parse");

    assert_eq!(
        directive.kind,
        crate::state::input_refs::LeadingAgentDirectiveKind::InternalDelegate
    );
}

#[test]
fn leading_agent_directive_supports_deactivate_phrases() {
    let known = vec!["weles".to_string()];

    for phrase in ["stop", "leave", "done", "return"] {
        let directive = crate::state::input_refs::parse_leading_agent_directive(
            &format!("@weles {phrase}"),
            &known,
        )
        .expect("directive should parse");

        assert_eq!(
            directive.kind,
            crate::state::input_refs::LeadingAgentDirectiveKind::ParticipantDeactivate
        );
    }
}

#[test]
fn leading_agent_directive_is_case_insensitive() {
    let known = vec!["weles".to_string()];
    let directive =
        crate::state::input_refs::parse_leading_agent_directive("!WeLeS check X", &known)
            .expect("directive should parse");

    assert_eq!(
        directive.kind,
        crate::state::input_refs::LeadingAgentDirectiveKind::InternalDelegate
    );
}

#[test]
fn leading_agent_directive_unknown_alias_falls_back() {
    let known = vec!["weles".to_string()];
    let directive =
        crate::state::input_refs::parse_leading_agent_directive("@unknown inspect @foo", &known);

    assert!(directive.is_none());
}

#[test]
fn leading_agent_directive_preserves_file_refs() {
    let known = vec!["weles".to_string()];
    let directive =
        crate::state::input_refs::parse_leading_agent_directive("@weles inspect @foo/bar", &known)
            .expect("directive should parse");

    assert_eq!(directive.body, "inspect @foo/bar");
}

pub(super) fn sample_collaboration_sessions() -> Vec<crate::state::CollaborationSessionVm> {
    vec![crate::state::CollaborationSessionVm {
        id: "session-1".to_string(),
        parent_task_id: Some("task-1".to_string()),
        parent_thread_id: None,
        agent_count: 2,
        disagreement_count: 1,
        consensus_summary: None,
        escalation: None,
        disagreements: vec![crate::state::CollaborationDisagreementVm {
            id: "disagreement-1".to_string(),
            topic: "deployment strategy".to_string(),
            positions: vec!["roll forward".to_string(), "roll back".to_string()],
            vote_count: 0,
            resolution: None,
        }],
    }]
}

#[test]
fn collaboration_tab_cycles_between_navigator_detail_and_input() {
    let mut model = build_model();
    model.main_pane_view = MainPaneView::Collaboration;
    model.focus = FocusArea::Chat;
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SessionsLoaded(
            sample_collaboration_sessions(),
        ));

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.collaboration.focus(),
        crate::state::CollaborationPaneFocus::Detail
    );

    let handled = model.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Input);

    let handled = model.handle_key(KeyCode::BackTab, KeyModifiers::SHIFT);
    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert_eq!(
        model.collaboration.focus(),
        crate::state::CollaborationPaneFocus::Detail
    );
}

#[test]
fn collaboration_arrow_keys_navigate_rows_and_detail_actions() {
    let mut model = build_model();
    model.main_pane_view = MainPaneView::Collaboration;
    model.focus = FocusArea::Chat;
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SessionsLoaded(
            sample_collaboration_sessions(),
        ));

    let handled = model.handle_key(KeyCode::Down, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.collaboration.selected_row_index(), 1);

    let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.collaboration.focus(),
        crate::state::CollaborationPaneFocus::Detail
    );

    let handled = model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.collaboration.selected_detail_action_index(), 1);

    let handled = model.handle_key(KeyCode::Left, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.collaboration.selected_detail_action_index(), 0);

    let handled = model.handle_key(KeyCode::Left, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.collaboration.focus(),
        crate::state::CollaborationPaneFocus::Navigator
    );
}

#[test]
fn collaboration_enter_in_detail_sends_vote_command() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Collaboration;
    model.focus = FocusArea::Chat;
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SessionsLoaded(
            sample_collaboration_sessions(),
        ));
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SelectRow(1));
    model
        .collaboration
        .reduce(crate::state::CollaborationAction::SetFocus(
            crate::state::CollaborationPaneFocus::Detail,
        ));

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);

    match cmd_rx
        .try_recv()
        .expect("expected collaboration vote command from detail enter")
    {
        DaemonCommand::VoteOnCollaborationDisagreement {
            parent_task_id,
            disagreement_id,
            task_id,
            position,
            confidence,
        } => {
            assert_eq!(parent_task_id, "task-1");
            assert_eq!(disagreement_id, "disagreement-1");
            assert_eq!(task_id, "operator");
            assert_eq!(position, "roll forward");
            assert_eq!(confidence, Some(1.0));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}
