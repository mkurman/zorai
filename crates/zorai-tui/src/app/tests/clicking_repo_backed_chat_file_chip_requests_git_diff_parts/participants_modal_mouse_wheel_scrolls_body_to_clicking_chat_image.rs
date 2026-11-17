use super::super::{build_model, unauthenticated_entry, unbounded_channel};
use super::*;
use base64::Engine as _;
use std::sync::mpsc;
#[test]
fn participants_modal_mouse_wheel_scrolls_body() {
    let mut model = build_model();
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Participants Thread".to_string(),
        thread_participants: (0..20)
            .map(|idx| crate::wire::ThreadParticipantState {
                agent_id: format!("agent-{idx}"),
                agent_name: format!("Agent {idx}"),
                instruction: format!("Instruction {idx}"),
                status: "active".to_string(),
                last_contribution_at: None,
                deactivated_at: None,
                always_auto_response: false,
                created_at: idx,
                updated_at: idx,
            })
            .collect(),
        ..Default::default()
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.open_thread_participants_modal();

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("participants modal should expose an overlay area");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: overlay_area.x.saturating_add(2),
        row: overlay_area.y.saturating_add(2),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.modal.picker_cursor(), 1);
}

#[test]
fn statistics_modal_mouse_wheel_scrolls_body() {
    let mut model = build_model();
    model.statistics_modal_snapshot = Some(zorai_protocol::AgentStatisticsSnapshot {
        window: zorai_protocol::AgentStatisticsWindow::All,
        generated_at: 1,
        has_incomplete_cost_history: false,
        totals: zorai_protocol::AgentStatisticsTotals {
            input_tokens: 100,
            output_tokens: 100,
            total_tokens: 200,
            cost_usd: 1.0,
            provider_count: 1,
            model_count: 80,
        },
        providers: vec![zorai_protocol::ProviderStatisticsRow {
            provider: "openai".to_string(),
            input_tokens: 100,
            output_tokens: 100,
            total_tokens: 200,
            cost_usd: 1.0,
        }],
        models: (0..80)
            .map(|idx| zorai_protocol::ModelStatisticsRow {
                provider: "openai".to_string(),
                model: format!("model-{idx}"),
                input_tokens: idx,
                output_tokens: idx,
                total_tokens: idx * 2,
                cost_usd: idx as f64 / 10.0,
            })
            .collect(),
        top_models_by_tokens: Vec::new(),
        top_models_by_cost: Vec::new(),
        daily: Vec::new(),
        sessions: Vec::new(),
        session_total: 0,
        session_limit: 25,
        session_offset: 0,
        content: Default::default(),
    });
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Statistics));
    model.width = 80;
    model.height = 20;

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("statistics modal should expose an overlay area");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: overlay_area.x.saturating_add(2),
        row: overlay_area.y.saturating_add(4),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.statistics_modal_scroll, 3);
}

#[test]
fn reselecting_same_sidebar_file_closes_work_context() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![task::WorkContextEntry {
                path: "/tmp/demo.txt".to_string(),
                is_text: true,
                ..Default::default()
            }],
        },
    ));
    model.tasks.reduce(task::TaskAction::SelectWorkPath {
        thread_id: "thread-1".to_string(),
        path: Some("/tmp/demo.txt".to_string()),
    });
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Files));
    model.main_pane_view = MainPaneView::WorkContext;
    model.focus = FocusArea::Sidebar;

    model.handle_sidebar_enter();

    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Sidebar);
}

#[test]
fn reselecting_same_sidebar_todo_closes_work_context() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        goal_run_id: Some("goal-1".to_string()),
        step_index: Some(0),
        items: vec![task::TodoItem {
            id: "todo-1".to_string(),
            content: "todo".to_string(),
            status: Some(task::TodoStatus::Pending),
            position: 0,
            step_index: None,
            created_at: 0,
            updated_at: 0,
        }],
    });
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Todos));
    model.main_pane_view = MainPaneView::WorkContext;
    model.focus = FocusArea::Sidebar;

    model.handle_sidebar_enter();

    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Sidebar);
}

#[test]
fn attention_surface_uses_settings_tab_when_modal_open() {
    let mut model = build_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));

    let (surface, thread_id, goal_run_id) = model.current_attention_target();
    assert_eq!(surface, "modal:settings:subagents");
    assert_eq!(thread_id, None);
    assert_eq!(goal_run_id, None);
}

#[test]
fn attention_surface_uses_sidebar_tab_for_sidebar_focus() {
    let mut model = build_model();
    model.connected = true;
    model.auth.loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread_1".to_string(),
        title: "Test".to_string(),
    });
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread_1".to_string(),
        goal_run_id: None,
        step_index: None,
        items: vec![task::TodoItem {
            id: "todo_1".to_string(),
            content: "todo".to_string(),
            status: Some(task::TodoStatus::Pending),
            position: 0,
            step_index: None,
            created_at: 0,
            updated_at: 0,
        }],
    });
    model.focus = FocusArea::Sidebar;
    model
        .sidebar
        .reduce(SidebarAction::SwitchTab(SidebarTab::Todos));

    let (surface, thread_id, goal_run_id) = model.current_attention_target();
    assert_eq!(surface, "conversation:sidebar:todos");
    assert_eq!(thread_id.as_deref(), Some("thread_1"));
    assert_eq!(goal_run_id, None);
}

#[test]
fn operator_profile_onboarding_uses_modal_over_provider_onboarding() {
    let mut model = build_model();
    model.connected = true;
    model.auth.loaded = true;
    model.auth.entries = vec![unauthenticated_entry()];
    model.operator_profile.visible = true;
    model.operator_profile.question = Some(OperatorProfileQuestionVm {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });
    model.open_operator_profile_onboarding_modal();

    assert!(
        model.should_show_operator_profile_onboarding(),
        "operator profile onboarding should be active"
    );
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::OperatorProfileOnboarding),
        "operator profile onboarding should own modal input"
    );
    assert!(
        model.should_show_provider_onboarding(),
        "provider onboarding can remain as the background view while operator profile owns the modal"
    );
}

#[test]
fn submit_operator_profile_answer_sends_command_and_clears_input() {
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
    model.input.set_text("Milan");

    assert!(model.submit_operator_profile_answer());
    assert_eq!(model.input.buffer(), "");
    assert!(
        model.operator_profile.question.is_none(),
        "question should clear when submission starts"
    );

    let sent = cmd_rx
        .try_recv()
        .expect("submitting answer should emit daemon command");
    match sent {
        DaemonCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(question_id, "name");
            assert_eq!(answer_json, "\"Milan\"");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn clicking_chat_file_chip_requests_file_preview() {
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

    let temp_path =
        std::env::temp_dir().join(format!("zorai-chat-preview-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&temp_path, "preview me\n").expect("fixture file should be writable");
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Tool,
            tool_name: Some("read_file".to_string()),
            tool_arguments: Some(
                serde_json::json!({ "path": temp_path.display().to_string() }).to_string(),
            ),
            tool_status: Some("done".to_string()),
            content: "preview me".to_string(),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let chip_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::ToolFilePath { message_index: 0 })
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("tool row should expose a clickable file chip");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: chip_pos.x,
        row: chip_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestFilePreview { path, max_bytes }) => {
            assert_eq!(path, temp_path.display().to_string());
            assert_eq!(max_bytes, Some(65_536));
        }
        other => panic!("expected file preview request, got {:?}", other),
    }
    assert!(matches!(model.main_pane_view, MainPaneView::FilePreview(_)));
}

#[test]
fn clicking_chat_image_attachment_requests_file_preview() {
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

    let temp_path =
        std::env::temp_dir().join(format!("zorai-chat-image-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(
        &temp_path,
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO0pGfcAAAAASUVORK5CYII=")
            .expect("fixture PNG should decode"),
    )
    .expect("fixture PNG should write");
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content_blocks: vec![chat::AgentContentBlock::Image {
                url: Some(format!("file://{}", temp_path.display())),
                data_url: None,
                mime_type: Some("image/png".to_string()),
            }],
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let image_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::MessageImage { message_index: 0 })
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("assistant image attachment should be clickable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: image_pos.x,
        row: image_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: image_pos.x,
        row: image_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestFilePreview { path, max_bytes }) => {
            assert_eq!(path, temp_path.display().to_string());
            assert_eq!(max_bytes, Some(65_536));
        }
        other => panic!("expected image preview request, got {:?}", other),
    }
    assert!(matches!(model.main_pane_view, MainPaneView::FilePreview(_)));
}
