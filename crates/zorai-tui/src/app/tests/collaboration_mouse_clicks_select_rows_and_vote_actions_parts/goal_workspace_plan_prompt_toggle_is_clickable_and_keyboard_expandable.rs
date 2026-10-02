use super::*;
use crate::app::tests::goal_sidebar_tab_cycling_stays_to_collaboration_mouse_clicks_select_rows::goal_sidebar_tab_cycling_stays_mod::*;
use super::super::rendered_chat_area;
use ratatui::backend::TestBackend;
#[test]
fn goal_workspace_plan_prompt_toggle_is_clickable_and_keyboard_expandable() {
    let mut model = goal_sidebar_model();

    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::PlanPromptToggle,
    );
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(model.goal_workspace.prompt_expanded());
    assert!(render_chat_plain(&mut model).contains("goal definition body"));

    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;
    model.goal_workspace.set_selected_plan_row(0);
    model
        .goal_workspace
        .set_selected_plan_item(Some(goal_workspace::GoalPlanSelection::PromptToggle));

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);
    assert!(model.goal_workspace.prompt_expanded());
}

#[test]
fn goal_workspace_plan_step_is_clickable_and_keyboard_expandable() {
    let mut model = goal_sidebar_model();

    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::PlanTodo {
            step_id: String::new(),
            todo_id: "todo-1".to_string(),
        },
    );
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(
        matches!(
            model.goal_workspace.selected_plan_item(),
            Some(goal_workspace::GoalPlanSelection::Todo { todo_id, .. })
                if todo_id == "todo-1" || todo_id == "todo-impl-1"
        ),
        "expected a worker todo row to be selected, got {:?}",
        model.goal_workspace.selected_plan_item()
    );
    let plain = render_chat_plain(&mut model);
    assert!(plain.contains("Draft outline"), "{plain}");
}

#[test]
fn goal_workspace_plan_main_thread_row_opens_thread_with_return_to_goal_path() {
    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;
    model.goal_workspace.set_selected_plan_row(1);
    model.goal_workspace.set_selected_plan_item(Some(
        goal_workspace::GoalPlanSelection::MainThread {
            thread_id: "thread-exec".to_string(),
        },
    ));

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.chat.active_thread_id(), Some("thread-exec"));
    assert!(model.mission_control_return_to_goal_target().is_some());
}

#[test]
fn goal_workspace_step_footer_actions_are_clickable() {
    let mut model = goal_sidebar_model();
    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::FooterAction(
            widgets::goal_workspace::GoalWorkspaceAction::OpenActions,
        ),
    );

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::GoalStepActionPicker)
    );
}

#[test]
fn goal_workspace_prompt_footer_actions_are_clickable_when_goal_has_no_steps() {
    let mut model = goal_sidebar_model();
    if let Some(run) = model.tasks.goal_run_by_id_mut("goal-1") {
        run.status = Some(task::GoalRunStatus::Failed);
        run.steps.clear();
        run.current_step_title = None;
        run.current_step_owner_profile = None;
        run.active_thread_id = None;
        run.execution_thread_ids.clear();
    }
    model.goal_workspace.set_selected_plan_row(0);
    model
        .goal_workspace
        .set_selected_plan_item(Some(goal_workspace::GoalPlanSelection::PromptToggle));

    let click = find_goal_workspace_hit_position(
        &model,
        widgets::goal_workspace::GoalWorkspaceHitTarget::FooterAction(
            widgets::goal_workspace::GoalWorkspaceAction::OpenActions,
        ),
    );

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::GoalStepActionPicker)
    );
    let items = model.goal_action_picker_items();
    assert!(items.contains(&crate::app::commands::GoalActionPickerItem::DeleteGoal));
}

#[test]
fn goal_workspace_progress_mode_restores_checkpoint_and_dossier_views() {
    let mut model = goal_sidebar_model();
    model
        .goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::Work);

    let plain = render_chat_plain(&mut model);

    assert!(plain.contains("Worker progress"), "{plain}");
    assert!(plain.contains("Draft outline"), "{plain}");
    assert!(plain.contains("Verify sources"), "{plain}");
}

#[test]
fn goal_workspace_active_agent_mode_restores_assignments_and_threads() {
    let mut model = goal_sidebar_model();
    model
        .goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::Review);

    let plain = render_chat_plain(&mut model);

    assert!(plain.contains("Supervisor review"), "{plain}");
    assert!(plain.contains("No supervisor review is pending"), "{plain}");
}

#[test]
fn goal_workspace_threads_mode_lists_clickable_threads() {
    let mut model = goal_sidebar_model();
    model
        .goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::Threads);

    let plain = render_chat_plain(&mut model);

    assert!(plain.contains("Threads"), "{plain}");
    assert!(plain.contains("Worker"), "{plain}");
    assert!(plain.contains("thread-1"), "{plain}");
    assert!(plain.contains("Child Task Two"), "{plain}");
    assert!(plain.contains("thread-2"), "{plain}");
}

#[test]
fn goal_workspace_threads_mode_enter_opens_selected_thread() {
    let mut model = goal_sidebar_model();
    model.focus = FocusArea::Chat;
    model
        .goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::Threads);
    model
        .goal_workspace
        .set_focused_pane(goal_workspace::GoalWorkspacePane::Timeline);
    model.goal_workspace.set_selected_timeline_row(0);

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.chat.active_thread_id(), Some("thread-1"));
}

#[test]
fn goal_workspace_threads_mode_pins_and_opens_spawned_goal_descendant() {
    let mut model = goal_sidebar_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-spawned".to_string(),
        title: "Spawned worker".to_string(),
    });
    model
        .tasks
        .reduce(task::TaskAction::TaskUpdate(task::AgentTask {
            id: "task-spawned".to_string(),
            title: "Spawned worker".to_string(),
            thread_id: Some("thread-spawned".to_string()),
            parent_thread_id: Some("thread-exec".to_string()),
            goal_run_id: None,
            created_at: 30,
            ..Default::default()
        }));
    model.focus = FocusArea::Chat;
    model
        .goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::Threads);
    model
        .goal_workspace
        .set_focused_pane(goal_workspace::GoalWorkspacePane::Timeline);

    let targets = crate::widgets::goal_workspace::timeline_targets(
        &model.tasks,
        "goal-1",
        &model.goal_workspace,
    );
    let spawned_index = targets
        .iter()
        .find_map(|(index, target)| match target {
            crate::widgets::goal_workspace::GoalWorkspaceHitTarget::ThreadRow(thread_id)
                if thread_id == "thread-spawned" =>
            {
                Some(*index)
            }
            _ => None,
        })
        .expect("spawned descendant should be pinned into goal threads");
    model
        .goal_workspace
        .set_selected_timeline_row(spawned_index);

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert_eq!(model.chat.active_thread_id(), Some("thread-spawned"));
}

#[test]
fn goal_workspace_needs_attention_mode_restores_non_empty_attention_surface() {
    let mut model = goal_sidebar_model();
    if let Some(run) = model.tasks.goal_run_by_id_mut("goal-1") {
        run.status = Some(task::GoalRunStatus::AwaitingReview);
        run.pending_review_report = Some("upstream returned an empty error body".to_string());
    }
    model
        .goal_workspace
        .set_mode(goal_workspace::GoalWorkspaceMode::Review);

    let plain = render_chat_plain(&mut model);

    assert!(plain.contains("[report]"), "{plain}");
    assert!(plain.contains("[Accept]"), "{plain}");
    assert!(
        plain.contains("upstream returned an empty error"),
        "{plain}"
    );
}

#[test]
fn goal_sidebar_task_open_renders_back_to_goal_and_escape_returns_to_goal() {
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

    let mut model = goal_sidebar_model();
    model.activate_goal_sidebar_tab(GoalSidebarTab::Tasks);
    model.select_goal_sidebar_row(0);

    assert!(model.handle_goal_sidebar_enter());
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::Task { ref task_id }) if task_id == "task-1"
    ));

    let plain = render_task_view(&mut model);
    assert!(plain.contains("Back to goal"), "{plain}");

    let handled = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert!(!handled);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun { ref goal_run_id, .. }) if goal_run_id == "goal-1"
    ));
}

#[test]
fn goal_sidebar_task_back_to_goal_row_click_returns_to_goal() {
    let mut model = goal_sidebar_model();
    model.activate_goal_sidebar_tab(GoalSidebarTab::Tasks);
    model.select_goal_sidebar_row(0);

    assert!(model.handle_goal_sidebar_enter());
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::Task { ref task_id }) if task_id == "task-1"
    ));

    let chat_area = rendered_chat_area(&model);
    let back_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                match widgets::task_view::hit_test(
                    chat_area,
                    &model.tasks,
                    match &model.main_pane_view {
                        MainPaneView::Task(target) => target,
                        _ => return None,
                    },
                    &model.theme,
                    model.task_view_scroll,
                    model.task_show_live_todos,
                    model.task_show_timeline,
                    model.task_show_files,
                    pos,
                ) {
                    Some(widgets::task_view::TaskViewHitTarget::BackToGoal) => Some(pos),
                    _ => None,
                }
            })
        })
        .expect("task view should expose a clickable back-to-goal row");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: back_pos.x,
        row: back_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: back_pos.x,
        row: back_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun { ref goal_run_id, .. }) if goal_run_id == "goal-1"
    ));
}

#[test]
fn mission_control_thread_router_open_active_thread_prefers_active_thread_id() {
    let mut model = mission_control_thread_router_model(Some("thread-active"), Some("thread-root"));

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-active"));
    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    assert!(render_chat_plain(&mut model).contains("Return to goal"));
}

#[test]
fn mission_control_thread_router_fallback_to_root_thread_sets_status() {
    let mut model = mission_control_thread_router_model(None, Some("thread-root"));

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert_eq!(model.chat.active_thread_id(), Some("thread-root"));
    assert_eq!(
        model.status_line,
        "Opened root goal thread as fallback because no active goal thread was available"
    );
}

#[test]
fn mission_control_thread_router_ignores_open_thread_shortcut_when_unavailable() {
    let mut model = mission_control_thread_router_model(None, None);

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
    assert_eq!(model.chat.active_thread_id(), None);
    assert_eq!(model.status_line, "");
}

#[test]
fn threads_return_to_goal_exposes_return_to_goal_affordance_when_opened_from_mission_control() {
    let mut model = mission_control_thread_router_model(Some("thread-active"), Some("thread-root"));

    let handled = model.handle_key(KeyCode::Char('o'), KeyModifiers::CONTROL);

    assert!(!handled);
    let plain = render_chat_plain(&mut model);
    assert!(plain.contains("Return to goal"), "{plain}");
}
