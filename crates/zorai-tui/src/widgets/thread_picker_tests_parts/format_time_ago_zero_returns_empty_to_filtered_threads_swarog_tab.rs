use super::super::from_tasks_to_is_weles_thread::*;
use super::super::hit_test_for_workspace_to_now_millis::*;
use super::super::is_svarog_agent_name_to_hit_test::*;
use super::super::*;
use crate::state::chat::{AgentThread, ChatAction, ChatState};
use crate::state::modal::{ModalState, ThreadPickerTab};
use crate::state::task::{AgentTask, GoalRun, GoalRunStatus, TaskAction, TaskState, TaskStatus};
use crate::state::workspace::WorkspaceState;
use crate::state::{SubAgentEntry, SubAgentsState};
use ratatui::layout::{Position, Rect};
use ratatui::widgets::{Block, BorderType, Borders};
use zorai_protocol::{
    WorkspaceActor, WorkspacePriority, WorkspaceSettings, WorkspaceTask, WorkspaceTaskStatus,
    WorkspaceTaskType,
};

#[test]
fn format_time_ago_zero_returns_empty() {
    assert_eq!(format_time_ago(0), "");
}

#[test]
fn format_tokens_zero_returns_empty() {
    assert_eq!(format_tokens(0), "");
}

#[test]
fn format_tokens_thousands() {
    let s = format_tokens(1500);
    assert!(s.contains("k tok"));
}

#[test]
fn format_tokens_billions() {
    assert_eq!(format_tokens(1_500_000_000), "1.5B tok");
}

#[test]
fn format_tokens_small() {
    let s = format_tokens(500);
    assert_eq!(s, "500 tok");
}

pub(super) fn make_chat(threads: Vec<AgentThread>) -> ChatState {
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadListReceived(threads));
    chat
}

pub(super) fn make_subagents(entries: Vec<SubAgentEntry>) -> SubAgentsState {
    let mut state = SubAgentsState::new();
    state.entries = entries;
    state
}

pub(super) fn make_tasks_with_goal_thread(thread_id: &str) -> TaskState {
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-1".into(),
        title: "Goal one".into(),
        thread_id: Some(thread_id.into()),
        active_thread_id: Some(thread_id.into()),
        ..Default::default()
    }));
    tasks
}

#[test]
fn thread_picker_status_label_reflects_running_paused_and_stopped_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "thread-running".into(),
            title: "Running thread".into(),
            ..Default::default()
        },
        AgentThread {
            id: "thread-paused".into(),
            title: "Paused thread".into(),
            ..Default::default()
        },
        AgentThread {
            id: "thread-stopped".into(),
            title: "Stopped thread".into(),
            messages: vec![crate::state::chat::AgentMessage {
                role: crate::state::chat::MessageRole::Assistant,
                content: "partial response [stopped]".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
    ]);
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![
        AgentTask {
            id: "task-running".into(),
            thread_id: Some("thread-running".into()),
            status: Some(TaskStatus::InProgress),
            ..Default::default()
        },
        AgentTask {
            id: "task-paused".into(),
            thread_id: Some("thread-paused".into()),
            status: Some(TaskStatus::Blocked),
            ..Default::default()
        },
    ]));
    tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
        id: "goal-stopped".into(),
        title: "Stopped goal".into(),
        thread_id: Some("thread-stopped".into()),
        status: Some(GoalRunStatus::Cancelled),
        ..Default::default()
    }));

    let running = chat
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-running")
        .expect("running thread exists");
    let paused = chat
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-paused")
        .expect("paused thread exists");
    let stopped = chat
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-stopped")
        .expect("stopped thread exists");
    let index = ThreadPickerStatusIndex::from_state(&chat, &tasks);

    assert_eq!(index.status_for(running), ThreadPickerStatus::Running);
    assert_eq!(index.status_for(paused), ThreadPickerStatus::Paused);
    assert_eq!(index.status_for(stopped), ThreadPickerStatus::Stopped);
}

#[test]
fn thread_picker_status_index_precomputes_statuses_for_render_rows() {
    let chat = make_chat(vec![AgentThread {
        id: "thread-running".into(),
        title: "Running thread".into(),
        ..Default::default()
    }]);
    let mut tasks = TaskState::new();
    tasks.reduce(TaskAction::TaskListReceived(vec![AgentTask {
        id: "task-running".into(),
        thread_id: Some("thread-running".into()),
        status: Some(TaskStatus::InProgress),
        ..Default::default()
    }]));

    let index = ThreadPickerStatusIndex::from_state(&chat, &tasks);

    let thread = chat
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-running")
        .expect("thread exists");
    assert_eq!(index.status_for(thread), ThreadPickerStatus::Running);
}

fn workspace_task(id: &str, task_type: WorkspaceTaskType) -> WorkspaceTask {
    WorkspaceTask {
        id: id.to_string(),
        workspace_id: "main".to_string(),
        title: "Workspace task".to_string(),
        task_type,
        description: "Description".to_string(),
        definition_of_done: None,
        priority: WorkspacePriority::Low,
        status: WorkspaceTaskStatus::InProgress,
        sort_order: 1,
        reporter: WorkspaceActor::User,
        assignee: Some(WorkspaceActor::Agent("svarog".to_string())),
        reviewer: Some(WorkspaceActor::User),
        thread_id: None,
        goal_run_id: None,
        runtime_history: Vec::new(),
        created_at: 1,
        updated_at: 1,
        started_at: Some(1),
        completed_at: None,
        deleted_at: None,
        last_notice_id: None,
    }
}

fn make_workspace(tasks: Vec<WorkspaceTask>) -> WorkspaceState {
    let mut workspace = WorkspaceState::new();
    workspace.set_settings(WorkspaceSettings {
        workspace_id: "main".to_string(),
        workspace_root: None,
        operator: zorai_protocol::WorkspaceOperator::User,
        created_at: 1,
        updated_at: 1,
        repo_monitor_enabled: false,
        repo_monitor_include_dirs: Vec::new(),
        repo_monitor_exclude_dirs: Vec::new(),
    });
    workspace.set_tasks("main".to_string(), tasks);
    workspace
}

#[test]
fn goal_thread_index_collects_goal_run_thread_ids_once_for_picker_use() {
    let tasks = make_tasks_with_goal_thread("thread-existing-goal");
    let index = GoalThreadIndex::from_tasks(&tasks);

    assert!(index.contains_id("thread-existing-goal"));
    assert!(!index.contains_id("thread-normal"));
}

#[test]
fn workspace_threads_route_to_workspace_tab() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            agent_name: Some("Svarog".into()),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "workspace-thread:one".into(),
            agent_name: Some("Domowoj".into()),
            title: "Workspace delivery".into(),
            ..Default::default()
        },
    ]);
    let subagents = make_subagents(Vec::new());
    let tasks = TaskState::new();
    let mut workspace_task = workspace_task("wtask-1", WorkspaceTaskType::Thread);
    workspace_task.thread_id = Some("workspace-thread:one".to_string());
    let workspace = make_workspace(vec![workspace_task]);
    let mut modal = ModalState::new();

    let labels = tab_specs_for_workspace(&chat, &subagents, &tasks, &workspace)
        .into_iter()
        .map(|spec| spec.label)
        .collect::<Vec<_>>();
    assert!(labels.iter().any(|label| label == "[Workspace]"));
    assert!(
        !labels.iter().any(|label| label == "[Domowoj]"),
        "workspace-linked agent threads should not create normal agent tabs"
    );

    let default_threads =
        filtered_threads_for_workspace(&chat, &modal, &subagents, &tasks, &workspace);
    assert_eq!(default_threads.len(), 1);
    assert_eq!(default_threads[0].id, "regular-thread");

    modal.set_thread_picker_tab(ThreadPickerTab::Agent("domowoj".into()));
    assert!(
        filtered_threads_for_workspace(&chat, &modal, &subagents, &tasks, &workspace).is_empty(),
        "workspace-linked threads should be excluded from normal agent tabs"
    );

    modal.set_thread_picker_tab(ThreadPickerTab::Workspace);
    let workspace_threads =
        filtered_threads_for_workspace(&chat, &modal, &subagents, &tasks, &workspace);
    assert_eq!(workspace_threads.len(), 1);
    assert_eq!(workspace_threads[0].id, "workspace-thread:one");
    assert_eq!(
        thread_display_title_for_workspace(workspace_threads[0], &tasks, &workspace),
        "workspace: Domowoj · Workspace delivery"
    );
}

#[test]
fn workspace_goal_threads_route_to_workspace_not_goals_tab() {
    let chat = make_chat(vec![AgentThread {
        id: "thread-existing-workspace-goal".into(),
        agent_name: Some("Domowoj".into()),
        title: "Workspace goal run".into(),
        ..Default::default()
    }]);
    let subagents = make_subagents(Vec::new());
    let tasks = make_tasks_with_goal_thread("thread-existing-workspace-goal");
    let mut workspace_task = workspace_task("wtask-goal", WorkspaceTaskType::Goal);
    workspace_task.goal_run_id = Some("goal-1".to_string());
    let workspace = make_workspace(vec![workspace_task]);
    let mut modal = ModalState::new();

    modal.set_thread_picker_tab(ThreadPickerTab::Goals);
    assert!(
        filtered_threads_for_workspace(&chat, &modal, &subagents, &tasks, &workspace).is_empty(),
        "workspace goal threads should not leak into the normal Goals tab"
    );

    modal.set_thread_picker_tab(ThreadPickerTab::Workspace);
    let workspace_threads =
        filtered_threads_for_workspace(&chat, &modal, &subagents, &tasks, &workspace);
    assert_eq!(workspace_threads.len(), 1);
    assert_eq!(workspace_threads[0].id, "thread-existing-workspace-goal");
}

pub(super) fn sample_subagent(id: &str, name: &str, builtin: bool) -> SubAgentEntry {
    SubAgentEntry {
        claude_permission_mode: None,
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
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: None,
    }
}

#[test]
fn filtered_threads_default_to_swarog_and_exclude_rarog_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            agent_name: Some("Svarog".into()),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "concierge".into(),
            title: "Concierge".into(),
            ..Default::default()
        },
        AgentThread {
            id: "heartbeat-1".into(),
            title: "HEARTBEAT SYNTHESIS".into(),
            ..Default::default()
        },
        AgentThread {
            id: "dm:rarog:swarog".into(),
            title: "Internal DM · Rarog ↔ Svarog".into(),
            ..Default::default()
        },
        AgentThread {
            id: "weles-thread".into(),
            title: "WELES governance review".into(),
            ..Default::default()
        },
    ]);
    let modal = ModalState::new();

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "regular-thread");
}

#[test]
fn tab_specs_include_playgrounds_tab() {
    let chat = make_chat(Vec::new());
    let subagents = make_subagents(Vec::new());
    let tabs = tab_specs(&chat, &subagents);

    assert_eq!(tabs.len(), 8);
    assert_eq!(tabs[3].label, "[Goals]");
    assert_eq!(tabs[4].label, "[Workspace]");
    assert!(
        tabs.iter()
            .any(|spec| spec.label.as_str() == "[Playgrounds]"),
        "expected thread picker tabs to expose a Playgrounds tab"
    );
}

#[test]
fn tab_specs_include_user_defined_subagent_without_duplicating_weles() {
    let chat = make_chat(vec![
        AgentThread {
            id: "thread-domowoj".into(),
            agent_name: Some("Domowoj".into()),
            title: "Domowoj helps".into(),
            ..Default::default()
        },
        AgentThread {
            id: "thread-weles".into(),
            agent_name: Some("Weles".into()),
            title: "Weles helps".into(),
            ..Default::default()
        },
    ]);
    let subagents = make_subagents(vec![
        sample_subagent("domowoj", "Domowoj", false),
        sample_subagent("weles_builtin", "Weles", true),
    ]);

    let tabs = tab_specs(&chat, &subagents);
    let labels = tabs
        .iter()
        .map(|spec| spec.label.as_str())
        .collect::<Vec<_>>();

    assert!(labels.contains(&"[Domowoj]"));
    assert_eq!(
        labels.iter().filter(|label| **label == "[Weles]").count(),
        1,
        "Weles should remain a dedicated single tab"
    );
}

#[test]
fn tab_specs_include_builtin_persona_when_threads_exist() {
    let chat = make_chat(vec![AgentThread {
        id: "thread-perun".into(),
        agent_name: Some("Perun".into()),
        title: "Perun triage".into(),
        ..Default::default()
    }]);
    let subagents = make_subagents(Vec::new());

    let tabs = tab_specs(&chat, &subagents);

    assert!(tabs.iter().any(|spec| spec.label == "[Perun]"));
}

#[test]
fn gateway_tab_is_inserted_after_internal_and_before_subagents() {
    let chat = make_chat(vec![AgentThread {
        id: "thread-domowoj".into(),
        agent_name: Some("Domowoj".into()),
        title: "Domowoj triage".into(),
        ..Default::default()
    }]);
    let subagents = make_subagents(vec![sample_subagent("domowoj", "Domowoj", false)]);

    let labels = tab_specs(&chat, &subagents)
        .into_iter()
        .map(|spec| spec.label)
        .collect::<Vec<_>>();

    assert_eq!(
        labels,
        vec![
            "[Svarog]".to_string(),
            "[Rarog]".to_string(),
            "[Weles]".to_string(),
            "[Goals]".to_string(),
            "[Workspace]".to_string(),
            "[Playgrounds]".to_string(),
            "[Internal]".to_string(),
            "[Gateway]".to_string(),
            "[Domowoj]".to_string(),
        ]
    );
}

#[test]
fn thread_picker_tabs_auto_scroll_to_keep_selected_agent_visible() {
    let chat = make_chat(Vec::new());
    let subagents = make_subagents(vec![
        sample_subagent("radogost", "Radogost", false),
        sample_subagent("rod", "Rod", false),
        sample_subagent("dola", "dola", false),
        sample_subagent("swarozyc", "Swarozyc", false),
        sample_subagent("swietowit", "Swietowit", false),
    ]);
    let selected = ThreadPickerTab::Agent("dola".to_string());
    let tabs_area = Rect::new(0, 0, 24, 1);
    let cells = tab_cells(&chat, &subagents);
    let scroll = tab_scroll_offset(tabs_area.width, &cells, &selected);

    let visible_labels = visible_tab_cells(tabs_area, &cells, scroll)
        .into_iter()
        .map(|(_, _, label)| label)
        .collect::<Vec<_>>();

    assert!(scroll > 0, "expected overflow to produce horizontal scroll");
    assert!(
        visible_labels.iter().any(|label| label.contains("dola")),
        "expected selected tab to remain visible after auto-scroll, got {visible_labels:?}"
    );
}

#[test]
fn thread_picker_hit_test_tracks_scrolled_tab_positions() {
    let chat = make_chat(Vec::new());
    let subagents = make_subagents(vec![
        sample_subagent("radogost", "Radogost", false),
        sample_subagent("rod", "Rod", false),
        sample_subagent("dola", "dola", false),
        sample_subagent("swarozyc", "Swarozyc", false),
    ]);
    let selected = ThreadPickerTab::Agent("dola".to_string());
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(selected.clone());
    let area = Rect::new(0, 0, 28, 8);
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(area);
    let tabs_row = thread_picker_layout(inner)
        .tabs
        .expect("narrow picker keeps the tab strip");
    let cells = tab_cells(&chat, &subagents);
    let scroll = tab_scroll_offset(tabs_row.width, &cells, &selected);
    let (_, selected_rect, _) = visible_tab_cells(tabs_row, &cells, scroll)
        .into_iter()
        .find(|(tab, _, _)| *tab == selected)
        .expect("selected tab should stay visible");
    let mouse = Position::new(selected_rect.x, selected_rect.y);

    let hit = hit_test(area, &chat, &modal, &subagents, mouse);

    assert_eq!(hit, Some(ThreadPickerHitTarget::Tab(selected)));
}

#[test]
fn wide_thread_picker_sidebar_lists_every_subagent_that_the_tab_strip_would_scroll_away() {
    let chat = make_chat(Vec::new());
    let subagents = make_subagents(vec![
        sample_subagent("claudus-maximus", "claudus-maximus", false),
        sample_subagent("coder", "coder", false),
        sample_subagent("dazhbog", "Dazhbog", false),
        sample_subagent("deepseekorrr-baby", "Deepseekorrr-baby", false),
        sample_subagent("kojo-test", "kojo-test", false),
    ]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Swarog);
    let area = Rect::new(0, 0, 120, 40);
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(area);
    let sidebar = thread_picker_layout(inner)
        .sidebar
        .expect("wide picker uses the sidebar");
    let rows = sidebar_rows(tab_specs(&chat, &subagents));

    let kojo_row = rows
        .iter()
        .position(|row| {
            matches!(row, ThreadPickerSidebarRow::Tab(ThreadPickerTab::Agent(id), label)
                if id == "kojo-test" && label == "kojo-test")
        })
        .expect("kojo-test must have its own sidebar row");
    assert!(kojo_row < sidebar.height as usize);
    let groups = rows
        .iter()
        .filter_map(|row| match row {
            ThreadPickerSidebarRow::Group(title) => Some(*title),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(groups, vec!["AGENTS", "SOURCES", "SUB-AGENTS"]);

    let hit = hit_test(
        area,
        &chat,
        &modal,
        &subagents,
        Position::new(sidebar.x + 4, sidebar.y + kojo_row as u16),
    );
    assert_eq!(
        hit,
        Some(ThreadPickerHitTarget::Tab(ThreadPickerTab::Agent(
            "kojo-test".to_string()
        )))
    );
}

#[test]
fn thread_picker_sidebar_order_matches_left_right_navigation_order() {
    let chat = make_chat(Vec::new());
    let subagents = make_subagents(vec![sample_subagent("coder", "coder", false)]);
    let specs = tab_specs(&chat, &subagents);
    let sidebar_tabs = sidebar_rows(specs.clone())
        .into_iter()
        .filter_map(|row| match row {
            ThreadPickerSidebarRow::Tab(tab, _) => Some(tab),
            _ => None,
        })
        .collect::<Vec<_>>();
    let navigation_tabs = specs.into_iter().map(|spec| spec.tab).collect::<Vec<_>>();
    assert_eq!(sidebar_tabs, navigation_tabs);
}

#[test]
fn filtered_threads_default_tab_excludes_playground_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "regular-thread".into(),
            agent_name: Some("Svarog".into()),
            title: "Regular work".into(),
            ..Default::default()
        },
        AgentThread {
            id: "playground:domowoj:thread-user".into(),
            title: "Participant Playground · Domowoj @ thread-user".into(),
            ..Default::default()
        },
        AgentThread {
            id: "goal:goal_1".into(),
            agent_name: Some("Domowoj".into()),
            title: "Run concrete moat pass".into(),
            ..Default::default()
        },
    ]);
    let modal = ModalState::new();

    let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "regular-thread");
}

#[test]
fn filtered_threads_dynamic_agent_tab_excludes_goal_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "thread-domowoj".into(),
            agent_name: Some("Domowoj".into()),
            title: "Normal agent conversation".into(),
            ..Default::default()
        },
        AgentThread {
            id: "goal:goal_1".into(),
            agent_name: Some("Domowoj".into()),
            title: "Run concrete moat pass".into(),
            ..Default::default()
        },
    ]);
    let subagents = make_subagents(vec![sample_subagent("domowoj", "Domowoj", false)]);
    let mut modal = ModalState::new();
    modal.set_thread_picker_tab(ThreadPickerTab::Agent("domowoj".to_string()));

    let threads = filtered_threads(&chat, &modal, &subagents);

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "thread-domowoj");
}

#[test]
fn filtered_threads_swarog_tab_excludes_dynamic_subagent_threads() {
    let chat = make_chat(vec![
        AgentThread {
            id: "thread-svarog".into(),
            agent_name: Some("Svarog".into()),
            title: "Root planning".into(),
            ..Default::default()
        },
        AgentThread {
            id: "thread-domowoj".into(),
            agent_name: Some("Domowoj".into()),
            title: "Spawned child execution".into(),
            ..Default::default()
        },
    ]);
    let subagents = make_subagents(vec![sample_subagent("domowoj", "Domowoj", false)]);
    let modal = ModalState::new();

    let threads = filtered_threads(&chat, &modal, &subagents);

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, "thread-svarog");
}
