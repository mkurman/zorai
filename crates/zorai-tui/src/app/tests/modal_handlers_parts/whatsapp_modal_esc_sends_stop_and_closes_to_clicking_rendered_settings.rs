use super::*;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::unbounded_channel;

pub(super) fn make_model() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, daemon_rx) = unbounded_channel();
    (TuiModel::new(event_rx, daemon_tx), daemon_rx)
}

pub(super) fn make_goal_run(
    id: &str,
    title: &str,
    status: crate::state::task::GoalRunStatus,
) -> crate::state::task::GoalRun {
    crate::state::task::GoalRun {
        id: id.to_string(),
        title: title.to_string(),
        status: Some(status),
        goal: format!("Goal for {title}"),
        ..Default::default()
    }
}

pub(super) fn make_goal_run_with_steps(
    id: &str,
    title: &str,
    status: crate::state::task::GoalRunStatus,
    steps: Vec<crate::state::task::GoalRunStep>,
) -> crate::state::task::GoalRun {
    crate::state::task::GoalRun {
        id: id.to_string(),
        title: title.to_string(),
        status: Some(status),
        goal: format!("Goal for {title}"),
        steps,
        ..Default::default()
    }
}

pub(super) fn next_goal_run_detail_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<String> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestGoalRunDetail(goal_run_id) = command {
            return Some(goal_run_id);
        }
    }
    None
}

pub(super) fn next_goal_run_checkpoints_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<String> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestGoalRunCheckpoints(goal_run_id) = command {
            return Some(goal_run_id);
        }
    }
    None
}

pub(super) fn next_goal_hydration_schedule(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<String> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::ScheduleGoalHydrationRefresh(goal_run_id) = command {
            return Some(goal_run_id);
        }
    }
    None
}

pub(super) fn seed_goal_sidebar_model() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (mut model, daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::TaskListReceived(vec![
        task::AgentTask {
            id: "task-1".to_string(),
            title: "Child Task One".to_string(),
            thread_id: Some("thread-1".to_string()),
            goal_run_id: Some("goal-1".to_string()),
            created_at: 1,
            ..Default::default()
        },
        task::AgentTask {
            id: "task-2".to_string(),
            title: "Child Task Two".to_string(),
            thread_id: Some("thread-2".to_string()),
            goal_run_id: Some("goal-1".to_string()),
            created_at: 2,
            ..Default::default()
        },
    ]));
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        crate::state::task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal One".to_string(),
            thread_id: Some("thread-1".to_string()),
            goal: "Goal for Goal One".to_string(),
            child_task_ids: vec!["task-1".to_string(), "task-2".to_string()],
            steps: vec![
                crate::state::task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    ..Default::default()
                },
                crate::state::task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Implement".to_string(),
                    order: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    ));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunCheckpointsReceived {
            goal_run_id: "goal-1".to_string(),
            checkpoints: vec![
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-1".to_string(),
                    checkpoint_type: "plan".to_string(),
                    step_index: Some(1),
                    context_summary_preview: Some("Implement checkpoint".to_string()),
                    ..Default::default()
                },
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-2".to_string(),
                    checkpoint_type: "note".to_string(),
                    context_summary_preview: Some("Loose checkpoint".to_string()),
                    ..Default::default()
                },
            ],
        });
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![
                task::WorkContextEntry {
                    path: "/tmp/first.md".to_string(),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                },
                task::WorkContextEntry {
                    path: "/tmp/second.md".to_string(),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                },
            ],
        },
    ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });
    model.focus = FocusArea::Sidebar;
    (model, daemon_rx)
}

pub(super) fn render_screen(model: &mut TuiModel) -> Vec<String> {
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

pub(super) fn collect_daemon_commands(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Vec<DaemonCommand> {
    let mut commands = Vec::new();
    while let Ok(command) = daemon_rx.try_recv() {
        commands.push(command);
    }
    commands
}

pub(super) fn seed_goal_approval_overlay(
    model: &mut TuiModel,
    approval_id: &str,
    goal_run_id: &str,
    thread_id: &str,
) {
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: thread_id.to_string(),
        title: "Goal Thread".to_string(),
    });
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        crate::state::task::GoalRun {
            id: goal_run_id.to_string(),
            title: "Goal plan review".to_string(),
            thread_id: Some(thread_id.to_string()),
            status: Some(crate::state::task::GoalRunStatus::AwaitingApproval),
            current_step_title: Some("review plan".to_string()),
            approval_count: 1,
            awaiting_approval_id: Some(approval_id.to_string()),
            ..Default::default()
        },
    ));
    model
        .approval
        .reduce(crate::state::ApprovalAction::ApprovalRequired(
            crate::state::PendingApproval {
                approval_id: approval_id.to_string(),
                task_id: goal_run_id.to_string(),
                task_title: Some("Goal plan review".to_string()),
                thread_id: Some(thread_id.to_string()),
                thread_title: Some("Goal Thread".to_string()),
                workspace_id: Some(model.config.honcho_workspace_id.clone()),
                rationale: Some("Review the plan before continuing".to_string()),
                reasons: vec!["operator approval required".to_string()],
                command: "review goal step: review plan".to_string(),
                risk_level: crate::state::RiskLevel::Medium,
                blast_radius: "review plan".to_string(),
                received_at: 1,
                seen_at: None,
            },
        ));
    model
        .approval
        .reduce(crate::state::ApprovalAction::SelectApproval(
            approval_id.to_string(),
        ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: goal_run_id.to_string(),
        step_id: None,
    });
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ApprovalOverlay));
}

pub(super) fn sample_subagent(id: &str, name: &str, builtin: bool) -> crate::state::SubAgentEntry {
    crate::state::SubAgentEntry {
        id: id.to_string(),
        name: name.to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin,
        immutable_identity: builtin,
        disable_allowed: !builtin,
        delete_allowed: !builtin,
        protected_reason: builtin.then(|| "builtin".to_string()),
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        claude_permission_mode: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: None,
    }
}

pub(super) fn navigate_model_picker_to(model: &mut TuiModel, model_id: &str) {
    let index = model
        .available_model_picker_models()
        .iter()
        .position(|entry| entry.id == model_id)
        .expect("expected model to exist in picker");
    if index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(index as i32));
    }
}

pub(super) fn make_runtime_assignment(
    role_id: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> task::GoalAgentAssignment {
    task::GoalAgentAssignment {
        role_id: role_id.to_string(),
        enabled: true,
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
        inherit_from_main: false,
    }
}

pub(super) fn make_goal_owner_profile(
    agent_label: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> task::GoalRuntimeOwnerProfile {
    task::GoalRuntimeOwnerProfile {
        agent_label: agent_label.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
    }
}

#[test]
pub(super) fn whatsapp_modal_esc_sends_stop_and_closes() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));

    let quit = model.handle_key_modal(
        KeyCode::Esc,
        KeyModifiers::NONE,
        modal::ModalKind::WhatsAppLink,
    );
    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert!(matches!(
        daemon_rx.try_recv().expect("expected stop command"),
        DaemonCommand::WhatsAppLinkStop
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected unsubscribe command"),
        DaemonCommand::WhatsAppLinkUnsubscribe
    ));
}

#[test]
pub(super) fn whatsapp_modal_esc_keeps_connected_session_running() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .set_whatsapp_link_connected(Some("+48663977535".to_string()));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));

    let quit = model.handle_key_modal(
        KeyCode::Esc,
        KeyModifiers::NONE,
        modal::ModalKind::WhatsAppLink,
    );
    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert!(matches!(
        daemon_rx.try_recv().expect("expected unsubscribe command"),
        DaemonCommand::WhatsAppLinkUnsubscribe
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
pub(super) fn whatsapp_modal_cancel_sends_stop_and_closes() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));

    let quit = model.handle_key_modal(
        KeyCode::Char('c'),
        KeyModifiers::NONE,
        modal::ModalKind::WhatsAppLink,
    );
    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert!(matches!(
        daemon_rx.try_recv().expect("expected stop command"),
        DaemonCommand::WhatsAppLinkStop
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected unsubscribe command"),
        DaemonCommand::WhatsAppLinkUnsubscribe
    ));
}

#[test]
pub(super) fn command_palette_tools_opens_settings_tools_tab() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
    let tools_index = model
        .modal
        .command_items()
        .iter()
        .position(|item| item.command == "tools")
        .expect("tools command should exist");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(tools_index as i32));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Settings));
    assert_eq!(model.settings.active_tab(), SettingsTab::Tools);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected auth refresh"),
        DaemonCommand::GetProviderAuthStates
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected codex auth refresh"),
        DaemonCommand::GetOpenAICodexAuthStatus
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected sub-agent refresh"),
        DaemonCommand::ListSubAgents
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected rarog refresh"),
        DaemonCommand::GetConciergeConfig
    ));
}

#[test]
pub(super) fn clicking_rendered_settings_tab_switches_tabs() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 40;
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("settings modal should expose its overlay area");
    let screen = render_screen(&mut model);
    let sidebar_start = overlay_area.x as usize + 1;
    let (chat_row, chat_col) = screen
        .iter()
        .enumerate()
        .skip(overlay_area.y as usize)
        .find_map(|(row, line)| {
            let sidebar: String = line.chars().skip(sidebar_start).take(20).collect();
            sidebar
                .find("Chat")
                .map(|col| (row as u16, (sidebar_start + col) as u16))
        })
        .expect("rendered settings sidebar should include Chat");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chat_col,
        row: chat_row,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.settings.active_tab(), SettingsTab::Chat);
}
