use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::{LazyLock, Mutex};
use tokio::sync::mpsc::unbounded_channel;
use zorai_shared::providers::PROVIDER_ID_OPENAI;
#[cfg(test)]
#[cfg(unix)]
#[cfg(unix)]

pub(super) fn make_model() -> TuiModel {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, _daemon_rx) = unbounded_channel();
    TuiModel::new(event_rx, daemon_tx)
}

pub(super) fn make_model_with_daemon_rx() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, daemon_rx) = unbounded_channel();
    (TuiModel::new(event_rx, daemon_tx), daemon_rx)
}

pub(super) fn next_thread_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<(String, Option<usize>, Option<usize>)> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        } = command
        {
            return Some((thread_id, message_limit, message_offset));
        }
    }
    None
}

#[test]
pub(super) fn idle_tick_does_not_request_redraw() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-user".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "idle".to_string(),
            ..Default::default()
        },
    });

    assert!(
        !model.on_tick(),
        "idle ticks should not force full terminal redraws"
    );
}

#[test]
pub(super) fn activity_spinner_repaints_at_half_tick_rate() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.next_system_monitor_tick = u64::MAX;
    model.agent_activity = Some("Thinking".to_string());

    assert!(model.wants_fast_tick());
    let redraws = (0..8).filter(|_| model.on_tick()).count();
    assert_eq!(
        redraws, 4,
        "a quiet tool wait has no daemon events, so the spinner must repaint on its own, but only every other tick"
    );

    model.agent_activity = None;
    model.on_tick();
    assert!(!model.wants_fast_tick());
    assert!(
        !model.on_tick_elapsed(20),
        "without visible activity the TUI must stay on the idle cadence"
    );
}

#[test]
pub(super) fn image_preview_cache_revision_redraws_once_per_cache_change() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;

    let path =
        std::env::temp_dir().join(format!("zorai-preview-redraw-{}.png", uuid::Uuid::new_v4()));
    image::RgbaImage::from_fn(48, 32, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
    })
    .save(&path)
    .expect("fixture PNG should write");

    let _ = widgets::image_preview::render_image_preview_lines(
        path.to_str().expect("temp path should be valid UTF-8"),
        24,
        8,
        &ThemeTokens::default(),
    );

    assert!(
        model.on_tick(),
        "queueing async image preview work should request one redraw"
    );
    assert!(
        !model.on_tick(),
        "unchanged preview cache revision should not create a redraw loop"
    );

    assert!(
        widgets::image_preview::process_preview_jobs_for_path_until_stable_for_tests(
            path.to_str().expect("temp path should be valid UTF-8")
        ),
        "queued preview work should produce a cached preview"
    );

    assert!(
        model.on_tick(),
        "ready async image preview cache should request one redraw from cache"
    );
    assert!(
        !model.on_tick(),
        "unchanged ready cache should not keep redrawing"
    );
}

#[test]
pub(super) fn streaming_chat_wants_fast_tick_even_without_footer_activity() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model.chat.reduce(chat::ChatAction::Delta {
        thread_id: "thread-user".to_string(),
        content: "streaming".to_string(),
    });

    assert!(model.chat.is_streaming());
    assert!(
        model.footer_activity_text().is_none(),
        "this reproduces streaming chat without a separate visible activity string"
    );
    assert!(
        model.wants_fast_tick(),
        "streaming chat should keep daemon event polling on the fast cadence"
    );
    assert!(
        !model.on_tick(),
        "fast polling for streaming chat should not redraw without a state change"
    );
}

#[test]
pub(super) fn live_goal_view_is_event_driven_instead_of_repainting_for_animation() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal".to_string(),
            status: Some(task::GoalRunStatus::Running),
            current_step_index: 0,
            steps: vec![task::GoalRunStep {
                id: "step-1".to_string(),
                title: "Implement".to_string(),
                status: Some(task::GoalRunStatus::Running),
                order: 0,
                ..Default::default()
            }],
            events: vec![task::GoalRunEvent {
                id: "event-1".to_string(),
                phase: "tool".to_string(),
                message: "Working".to_string(),
                step_index: Some(0),
                ..Default::default()
            }],
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    assert!(
        !model.wants_fast_tick(),
        "live goal progress must not keep the whole TUI on the fast animation cadence"
    );
    assert!(
        !model.on_tick(),
        "an unchanged live goal must wait for daemon state updates instead of repainting"
    );
}

#[test]
pub(super) fn selected_static_thread_does_not_repaint_for_age_label() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.next_system_monitor_tick = u64::MAX;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-static".to_string(),
        title: "Static Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-static".to_string()));

    assert!(!model.wants_fast_tick());
    assert!(!model.on_tick_elapsed(10));
}

#[test]
pub(super) fn queued_prompt_copy_expiry_requests_one_redraw() {
    let (mut model, _daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.next_system_monitor_tick = u64::MAX;
    model.queued_prompts.push(QueuedPrompt::new("copy me"));
    model.open_queued_prompts_modal();
    model.queued_prompt_action = QueuedPromptAction::Copy;

    model.execute_selected_queued_prompt_action();

    assert!(model.queued_prompts[0].is_copied(model.tick_counter));
    assert!(
        model.on_tick_elapsed(100),
        "visible queued prompt copy feedback expiry should request a redraw"
    );
    assert!(!model.queued_prompts[0].is_copied(model.tick_counter));
    assert!(
        !model.on_tick(),
        "expired queued prompt copy feedback should not create a redraw loop"
    );
}

pub(super) fn saw_list_tasks_command(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> bool {
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::ListTasks) {
            return true;
        }
    }
    false
}

pub(super) fn saw_workspace_task_list_command(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
    expected_workspace_id: &str,
) -> bool {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::ListWorkspaceTasks { workspace_id, .. } = command {
            if workspace_id == expected_workspace_id {
                return true;
            }
        }
    }
    false
}

pub(super) fn workspace_task(
    id: &str,
    status: zorai_protocol::WorkspaceTaskStatus,
) -> zorai_protocol::WorkspaceTask {
    zorai_protocol::WorkspaceTask {
        id: id.to_string(),
        workspace_id: "main".to_string(),
        title: id.to_string(),
        task_type: zorai_protocol::WorkspaceTaskType::Thread,
        description: String::new(),
        definition_of_done: None,
        priority: zorai_protocol::WorkspacePriority::Normal,
        status,
        sort_order: 1,
        reporter: zorai_protocol::WorkspaceActor::User,
        assignee: Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
        reviewer: None,
        thread_id: Some(format!("thread-{id}")),
        goal_run_id: None,
        runtime_history: Vec::new(),
        created_at: 1,
        updated_at: 1,
        started_at: None,
        completed_at: None,
        deleted_at: None,
        last_notice_id: None,
    }
}

#[cfg(unix)]
pub(super) fn with_fake_mpv_in_path<F: FnOnce()>(test: F) {
    static PATH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    let _guard = PATH_LOCK.lock().expect("path lock should not be poisoned");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let temp_dir =
        std::env::temp_dir().join(format!("zorai-test-mpv-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).expect("fake mpv dir should be created");

    let fake_mpv = temp_dir.join("mpv");
    std::fs::write(&fake_mpv, "#!/bin/sh\nsleep 5\n").expect("fake mpv should be written");
    let mut permissions = std::fs::metadata(&fake_mpv)
        .expect("fake mpv metadata should exist")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_mpv, permissions).expect("fake mpv should be executable");

    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{old_path}", temp_dir.display()));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));

    std::env::set_var("PATH", old_path);
    let _ = std::fs::remove_file(&fake_mpv);
    let _ = std::fs::remove_dir(&temp_dir);

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

pub(super) fn next_goal_run_page_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<(
    String,
    Option<usize>,
    Option<usize>,
    Option<usize>,
    Option<usize>,
)> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestGoalRunDetailPage {
            goal_run_id,
            step_offset,
            step_limit,
            event_offset,
            event_limit,
        } = command
        {
            return Some((
                goal_run_id,
                step_offset,
                step_limit,
                event_offset,
                event_limit,
            ));
        }
    }
    None
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

pub(super) fn active_goal_run_sidebar_model() -> TuiModel {
    let mut model = make_model();
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
            created_at: 10,
            ..Default::default()
        },
        task::AgentTask {
            id: "task-2".to_string(),
            title: "Child Task Two".to_string(),
            thread_id: Some("thread-2".to_string()),
            goal_run_id: Some("goal-1".to_string()),
            created_at: 20,
            ..Default::default()
        },
    ]));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal Title".to_string(),
            thread_id: Some("thread-1".to_string()),
            goal: "goal definition body".to_string(),
            current_step_title: Some("Implement".to_string()),
            child_task_ids: vec!["task-1".to_string(), "task-2".to_string()],
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
                task::GoalRunStep {
                    id: "step-3".to_string(),
                    title: "Verify".to_string(),
                    order: 2,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunCheckpointsReceived {
            goal_run_id: "goal-1".to_string(),
            checkpoints: vec![
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-1".to_string(),
                    checkpoint_type: "plan".to_string(),
                    step_index: Some(0),
                    context_summary_preview: Some("Checkpoint for Plan".to_string()),
                    ..Default::default()
                },
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-2".to_string(),
                    checkpoint_type: "verify".to_string(),
                    step_index: Some(2),
                    context_summary_preview: Some("Checkpoint for Verify".to_string()),
                    ..Default::default()
                },
            ],
        });
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![
                task::WorkContextEntry {
                    path: "/tmp/plan.md".to_string(),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                },
                task::WorkContextEntry {
                    path: "/tmp/report.md".to_string(),
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
    model
}

#[test]
pub(super) fn connected_event_defers_concierge_welcome_until_config_loads() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_connected_event();

    let mut saw_refresh = false;
    let mut saw_get_config = false;
    let mut saw_refresh_services = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::Refresh => saw_refresh = true,
            DaemonCommand::GetConfig => saw_get_config = true,
            DaemonCommand::RefreshServices => saw_refresh_services = true,
            DaemonCommand::RequestConciergeWelcome => {
                panic!("concierge welcome should wait until config is loaded")
            }
            _ => {}
        }
    }

    assert!(saw_refresh, "connect should still request thread refresh");
    assert!(
        saw_get_config,
        "connect should request config on the startup-critical lane"
    );
    assert!(
        !saw_refresh_services,
        "connect should defer heavy service refresh until config has loaded"
    );
    assert!(
        !model.concierge.loading,
        "concierge loading should not start until welcome is actually requested"
    );
}

#[test]
pub(super) fn first_raw_config_load_triggers_concierge_welcome_request() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = false;

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
        "managed_execution": {
            "sandbox_enabled": false,
            "security_level": "yolo"
        }
    }));

    assert!(
        model.agent_config_loaded,
        "raw config should mark config as loaded"
    );
    assert_eq!(model.config.managed_security_level, "yolo");
    assert!(
        model.concierge.loading,
        "first config load should start concierge welcome"
    );
    let mut saw_welcome = false;
    let mut saw_refresh_services = false;
    let mut saw_provider_auth_states = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::RequestConciergeWelcome => saw_welcome = true,
            DaemonCommand::RefreshServices => saw_refresh_services = true,
            DaemonCommand::GetProviderAuthStates => saw_provider_auth_states = true,
            _ => {}
        }
    }
    assert!(saw_welcome, "expected concierge welcome request");
    assert!(
        saw_refresh_services,
        "config load should trigger the deferred heavy startup refresh after concierge is queued"
    );
    assert!(
        saw_provider_auth_states,
        "config load should release deferred startup follow-up requests"
    );
}
