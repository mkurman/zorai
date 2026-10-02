use super::super::{build_model, rendered_chat_area, unbounded_channel};
use super::*;
use ratatui::backend::TestBackend;
use std::sync::mpsc;
#[test]
fn closing_chat_file_preview_returns_to_conversation() {
    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.main_pane_view = MainPaneView::FilePreview(ChatFilePreviewTarget {
        path: "/tmp/demo.txt".to_string(),
        repo_root: None,
        repo_relative_path: None,
    });

    let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.focus, FocusArea::Chat);
}

#[test]
fn closing_same_thread_file_preview_does_not_reload_thread() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.open_file_preview_path("/tmp/demo.txt".to_string());
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestFilePreview { path, .. }) => {
            assert_eq!(path, "/tmp/demo.txt");
        }
        other => panic!("expected file preview request, got {:?}", other),
    }

    let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.chat.active_thread_id(), Some("thread-1"));
    assert!(
        cmd_rx.try_recv().is_err(),
        "returning from a same-thread file preview should not re-request thread data"
    );
}

#[test]
fn goal_view_renders_goal_run_dossier_sections() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Instrument Titan".to_string(),
            goal: "Ship the first dossier-aware goal view.".to_string(),
            steps: vec![task::GoalRunStep {
                id: "step-1".to_string(),
                title: "Phone logging flow".to_string(),
                order: 0,
                ..Default::default()
            }],
            dossier: Some(task::GoalRunDossier {
                projection_state: "in_progress".to_string(),
                summary: Some("Execution is split into build and verification units.".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Goal Mission Control"), "{plain}");
    assert!(plain.contains("Work"), "{plain}");
    assert!(plain.contains("Review"), "{plain}");
    assert!(plain.contains("Activity"), "{plain}");
    assert!(plain.contains("Files"), "{plain}");
    assert!(plain.contains("Goal Prompt"), "{plain}");
    assert!(!plain.contains("Execution Dossier"), "{plain}");
}

#[test]
fn goal_view_renders_goal_control_hints() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Paused Goal".to_string(),
            status: Some(task::GoalRunStatus::Paused),
            current_step_index: 1,
            current_step_title: Some("Implement".to_string()),
            steps: vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Implement".to_string(),
                    order: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Goal Mission Control"), "{plain}");
    assert!(plain.contains("Work"), "{plain}");
    assert!(plain.contains("Review"), "{plain}");
    assert!(plain.contains("Activity"), "{plain}");
    assert!(!plain.contains("Active agent"), "{plain}");
    assert!(!plain.contains("Needs attention"), "{plain}");
}

#[test]
fn goal_view_renders_visual_status_banner_and_control_chips() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;
    model.tick_counter = 12;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Paused Goal".to_string(),
            status: Some(task::GoalRunStatus::Paused),
            current_step_index: 1,
            current_step_title: Some("Implement".to_string()),
            steps: vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Implement".to_string(),
                    order: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Goal Mission Control"), "{plain}");
    assert!(plain.contains("Work"), "{plain}");
    assert!(plain.contains("Review"), "{plain}");
    assert!(!plain.contains("Run Status"), "{plain}");
    assert!(!plain.contains("Controls"), "{plain}");
}

#[test]
fn goal_view_renders_live_activity_with_tools_files_and_todos() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;
    model.tick_counter = 21;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Live Goal".to_string(),
            thread_id: Some("thread-1".to_string()),
            status: Some(task::GoalRunStatus::Running),
            current_step_index: 0,
            current_step_title: Some("Patch UI".to_string()),
            steps: vec![task::GoalRunStep {
                id: "step-1".to_string(),
                title: "Patch UI".to_string(),
                order: 0,
                ..Default::default()
            }],
            events: vec![
                task::GoalRunEvent {
                    id: "event-1".to_string(),
                    phase: "tool".to_string(),
                    message: "apply_patch updated goal view".to_string(),
                    details: Some("Added status hero and activity cards".to_string()),
                    step_index: Some(0),
                    ..Default::default()
                },
                task::GoalRunEvent {
                    id: "event-2".to_string(),
                    phase: "todo".to_string(),
                    message: "goal todo updated".to_string(),
                    step_index: Some(0),
                    todo_snapshot: vec![task::TodoItem {
                        id: "todo-1".to_string(),
                        content: "Inspect failing test".to_string(),
                        status: Some(task::TodoStatus::InProgress),
                        position: 0,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![task::WorkContextEntry {
                path: "crates/zorai-tui/src/widgets/task_view.rs".to_string(),
                source: "apply_patch".to_string(),
                change_kind: Some("diff".to_string()),
                goal_run_id: Some("goal-1".to_string()),
                step_index: Some(0),
                is_text: true,
                ..Default::default()
            }],
        },
    ));
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        goal_run_id: None,
        step_index: None,
        items: vec![task::TodoItem {
            id: "todo-1".to_string(),
            content: "Inspect failing test".to_string(),
            status: Some(task::TodoStatus::InProgress),
            step_index: Some(0),
            position: 0,
            ..Default::default()
        }],
    });
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });
    model
        .goal_workspace
        .set_mode(crate::state::goal_workspace::GoalWorkspaceMode::Activity);

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Run activity"), "{plain}");
    assert!(plain.contains("apply_patch updated goal view"), "{plain}");
    assert!(plain.contains("goal todo updated"), "{plain}");
    assert!(plain.contains("Inspect failing test"), "{plain}");
    assert!(plain.contains("Selected activity"), "{plain}");
    assert!(plain.contains("[details]"), "{plain}");
}

#[test]
fn goal_view_related_tasks_use_status_checkbox_without_duplicate_status_text() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal".to_string(),
            thread_id: Some("thread-1".to_string()),
            steps: vec![task::GoalRunStep {
                id: "step-1".to_string(),
                title: "Step".to_string(),
                order: 0,
                ..Default::default()
            }],
            ..Default::default()
        }));
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        goal_run_id: Some("goal-1".to_string()),
        step_index: Some(0),
        items: vec![
            task::TodoItem {
                id: "todo-1".to_string(),
                content: "Collect and index sources".to_string(),
                status: Some(task::TodoStatus::InProgress),
                step_index: Some(0),
                position: 0,
                ..Default::default()
            },
            task::TodoItem {
                id: "todo-2".to_string(),
                content: "Ground the user's background".to_string(),
                status: Some(task::TodoStatus::Completed),
                step_index: Some(0),
                position: 1,
                ..Default::default()
            },
            task::TodoItem {
                id: "todo-3".to_string(),
                content: "Review plan".to_string(),
                status: Some(task::TodoStatus::Blocked),
                step_index: Some(0),
                position: 2,
                ..Default::default()
            },
        ],
    });
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });
    model.goal_workspace.set_step_expanded("step-1", true);

    let plain = render_task_view(&mut model);

    assert!(plain.contains("[~] Collect and index sources"), "{plain}");
    assert!(
        plain.contains("[x] Ground the user's background"),
        "{plain}"
    );
    assert!(plain.contains("[!] Review plan"), "{plain}");
    assert!(
        !plain.contains("Collect and index sources running"),
        "{plain}"
    );
    assert!(
        !plain.contains("Ground the user's background done"),
        "{plain}"
    );
    assert!(!plain.contains("Review plan blocked"), "{plain}");
}

#[test]
fn goal_view_paused_restart_renders_review_guidance() {
    fn render_task_view(model: &mut TuiModel) -> String {
        let backend = TestBackend::new(model.width, model.height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| model.render(frame))
            .expect("task view render should succeed");

        let chat_area = rendered_chat_area(model);
        let buffer = terminal.backend().buffer();
        (chat_area.y..chat_area.y.saturating_add(chat_area.height))
            .map(|y| {
                (chat_area.x..chat_area.x.saturating_add(chat_area.width))
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let mut model = build_model();
    model.focus = FocusArea::Chat;
    model.show_sidebar_override = Some(false);
    model.task_show_live_todos = false;
    model.task_show_timeline = false;
    model.task_show_files = false;

    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Paused Goal".to_string(),
            status: Some(task::GoalRunStatus::Paused),
            current_step_title: Some("Implement".to_string()),
            events: vec![task::GoalRunEvent {
                id: "event-1".to_string(),
                phase: "restart".to_string(),
                message: "Daemon restarted; goal run paused for operator review.".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });
    model
        .goal_workspace
        .set_mode(crate::state::goal_workspace::GoalWorkspaceMode::Activity);

    let plain = render_task_view(&mut model);

    assert!(plain.contains("Run activity"), "{plain}");
    assert!(
        plain.contains("Daemon restarted; goal run paused"),
        "{plain}"
    );
    assert!(plain.contains("operator review."), "{plain}");
}
