use super::*;
use crate::history::IntentPredictionRow;
use crate::session_manager::SessionManager;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};
use zorai_protocol::{WorkspaceOperator, WorkspaceSettings};

fn sample_task(id: &str, thread_id: Option<&str>, goal_run_id: Option<&str>) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: id.to_string(),
        description: String::new(),
        status: TaskStatus::Queued,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: 0,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: thread_id.map(str::to_string),
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: goal_run_id.map(str::to_string),
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        completion_contract: None,
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_api_transport: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    }
}

fn sample_goal_run(id: &str, thread_id: Option<&str>) -> GoalRun {
    GoalRun {
        id: id.to_string(),
        title: id.to_string(),
        goal: String::new(),
        client_request_id: None,
        status: GoalRunStatus::Queued,
        priority: TaskPriority::Normal,
        created_at: 0,
        updated_at: 0,
        started_at: None,
        completed_at: None,
        thread_id: thread_id.map(str::to_string),
        session_id: None,
        current_step_index: 0,
        current_step_title: None,
        current_step_kind: None,
        planner_owner_profile: None,
        current_step_owner_profile: None,
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 0,
        max_replans: 3,
        plan_summary: None,
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        step_failure_history: Vec::new(),
        stopped_reason: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        pending_review_report: None,
        duration_ms: None,
        steps: Vec::new(),
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: Default::default(),
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        supervision_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    }
}

fn sample_anticipatory_item(id: &str, kind: &str, title: &str, summary: &str) -> AnticipatoryItem {
    AnticipatoryItem {
        id: id.to_string(),
        kind: kind.to_string(),
        title: title.to_string(),
        summary: summary.to_string(),
        bullets: Vec::new(),
        intent_prediction: None,
        confidence: 0.0,
        goal_run_id: None,
        thread_id: None,
        preferred_client_surface: None,
        preferred_attention_surface: None,
        created_at: 0,
        updated_at: 0,
    }
}

async fn enable_repo_monitor_for_workspace_root(
    engine: &AgentEngine,
    workspace_root: &std::path::Path,
) {
    let now = now_millis();
    engine
        .history
        .upsert_workspace_settings(&WorkspaceSettings {
            workspace_id: "repo-monitor-test".to_string(),
            workspace_root: Some(workspace_root.to_string_lossy().to_string()),
            operator: WorkspaceOperator::User,
            repo_monitor_enabled: true,
            repo_monitor_include_dirs: vec![".".to_string()],
            repo_monitor_exclude_dirs: Vec::new(),
            created_at: now,
            updated_at: now,
        })
        .await
        .expect("enable repo monitor for test workspace root");
}

#[test]
fn circular_hour_distance_wraps_across_midnight() {
    assert_eq!(circular_hour_distance(23, 1), 2);
    assert_eq!(circular_hour_distance(5, 5), 0);
}

#[test]
fn truncate_hint_shortens_long_strings() {
    let value = "x".repeat(160);
    let shortened = truncate_hint(&value);
    assert!(shortened.len() < value.len());
    assert!(shortened.ends_with('…'));
}

#[test]
fn attention_surface_suppresses_briefs_in_settings() {
    assert!(!should_surface_anticipatory_kind(
        "morning_brief",
        Some("modal:settings:provider")
    ));
    assert!(!should_surface_anticipatory_kind(
        "stuck_hint",
        Some("modal:approval")
    ));
}

#[test]
fn attention_surface_allows_task_and_conversation_contexts() {
    assert!(should_surface_anticipatory_kind(
        "morning_brief",
        Some("conversation:chat")
    ));
    assert!(should_surface_anticipatory_kind(
        "stuck_hint",
        Some("task:detail")
    ));
}

#[test]
fn task_attention_priority_prefers_goal_then_thread() {
    let attention = AttentionFocus {
        thread_id: Some("thread_1".to_string()),
        goal_run_id: Some("goal_1".to_string()),
    };
    let goal_match = sample_task("task_goal", Some("thread_9"), Some("goal_1"));
    let thread_match = sample_task("task_thread", Some("thread_1"), Some("goal_9"));

    assert_eq!(task_attention_priority(&goal_match, &attention), 2);
    assert_eq!(task_attention_priority(&thread_match, &attention), 1);
}

#[test]
fn goal_attention_priority_prefers_exact_goal_match() {
    let attention = AttentionFocus {
        thread_id: Some("thread_1".to_string()),
        goal_run_id: Some("goal_1".to_string()),
    };
    let exact = sample_goal_run("goal_1", Some("thread_2"));
    let thread_only = sample_goal_run("goal_2", Some("thread_1"));

    assert_eq!(goal_attention_priority(&exact, &attention), 2);
    assert_eq!(goal_attention_priority(&thread_only, &attention), 1);
}

#[tokio::test]
async fn anticipatory_tick_ignores_weles_owned_stuck_tasks() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.stuck_detection = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut weles_task = sample_task("task-weles", Some("thread-weles"), None);
    weles_task.title = "WELES".to_string();
    weles_task.status = TaskStatus::Blocked;
    weles_task.sub_agent_def_id = Some("weles_builtin".to_string());
    weles_task.blocked_reason = Some("waiting for lane availability: daemon-main".to_string());
    weles_task.started_at = Some(1);

    engine.tasks.lock().await.push_back(weles_task);
    engine.run_anticipatory_tick().await;

    assert!(
        engine.anticipatory.read().await.items.is_empty(),
        "WELES-owned blocked tasks should not surface as anticipatory hints"
    );
}

#[tokio::test]
async fn session_start_prewarm_hydrates_active_attention_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-prewarm"), None)
        .await
        .unwrap();
    engine.mark_operator_present("test").await;
    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .hydration_by_thread
            .contains_key("thread-prewarm"),
        "session-start prewarm should hydrate the active attention thread before surfacing items"
    );
}

#[tokio::test]
async fn session_start_prewarm_resolves_persisted_attention_goal_thread_after_live_queue_clear() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut goal = sample_goal_run("goal-attention", Some("thread-attention-goal"));
    goal.status = GoalRunStatus::Completed;
    goal.updated_at = now_millis();
    engine.goal_runs.lock().await.push_back(goal);
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();

    engine
        .record_operator_attention("conversation:chat", None, Some("goal-attention"))
        .await
        .unwrap();
    engine.mark_operator_present("test").await;
    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .hydration_by_thread
            .contains_key("thread-attention-goal"),
        "session-start prewarm should resolve active attention goal ids through persisted goal rows"
    );
}

#[tokio::test]
async fn session_start_prewarm_hydrates_persisted_active_task_thread_after_live_queue_clear() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut task = sample_task("task-prewarm", Some("thread-task-prewarm"), None);
    task.status = TaskStatus::InProgress;
    task.started_at = Some(now_millis().saturating_sub(5_000));
    engine.tasks.lock().await.push_back(task);
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    engine.mark_operator_present("test").await;
    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .hydration_by_thread
            .contains_key("thread-task-prewarm"),
        "session-start prewarm should hydrate active task threads from persisted task rows"
    );
}

#[tokio::test]
async fn session_start_prewarm_hydrates_persisted_active_goal_thread_after_live_queue_clear() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut goal = sample_goal_run("goal-prewarm", Some("thread-goal-prewarm"));
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now_millis();
    engine.goal_runs.lock().await.push_back(goal);
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();

    engine.mark_operator_present("test").await;
    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .hydration_by_thread
            .contains_key("thread-goal-prewarm"),
        "session-start prewarm should hydrate active goal threads from persisted goal rows"
    );
}

#[tokio::test]
async fn anticipatory_tick_routes_stuck_hint_to_thread_surface_with_idle_signal() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.stuck_detection = true;
    config.anticipatory.stuck_detection_delay_seconds = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();

    let now = now_millis();
    let mut stale_task = sample_task("task-stale", Some("thread-surface"), None);
    stale_task.title = "Review failing command".to_string();
    stale_task.status = TaskStatus::InProgress;
    stale_task.started_at = Some(now.saturating_sub(30_000));
    stale_task.last_error = Some("command timed out while waiting for output".to_string());
    engine.tasks.lock().await.push_back(stale_task);
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    engine
        .set_thread_client_surface("thread-surface", zorai_protocol::ClientSurface::Electron)
        .await;
    engine
        .record_operator_attention("task:detail", Some("thread-surface"), None)
        .await
        .unwrap();
    {
        let mut runtime = engine.anticipatory.write().await;
        runtime.last_presence_at = Some(now.saturating_sub(60_000));
        runtime.active_attention_updated_at = Some(now.saturating_sub(60_000));
    }

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "stuck_hint")
        .expect("expected a stuck hint for the stale task");
    assert_eq!(item.preferred_client_surface.as_deref(), Some("electron"));
    assert_eq!(
        item.preferred_attention_surface.as_deref(),
        Some("task:detail")
    );
    assert!(
        item.bullets
            .iter()
            .any(|bullet| bullet.contains("Operator attention has been idle")),
        "idle-aware heuristics should be surfaced in the stuck hint bullets"
    );

    let notice = timeout(Duration::from_millis(250), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::WorkflowNotice {
                    kind,
                    thread_id,
                    message,
                    details,
                }) => {
                    break (kind, thread_id, message, details);
                }
                Ok(_) => continue,
                Err(error) => panic!("expected workflow notice, got event error: {error}"),
            }
        }
    })
    .await
    .expect("thread-targeted anticipatory notice should arrive");

    assert_eq!(notice.0, "anticipatory");
    assert_eq!(notice.1, "thread-surface");
    assert!(notice.2.contains("Task May Be Stuck"));
    assert!(notice
        .3
        .as_deref()
        .is_some_and(|details| details.contains("Operator attention has been idle")));
}

#[tokio::test]
async fn morning_brief_inherits_route_from_top_goal_surface() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let now = now_millis();
    let mut goal = sample_goal_run("goal-route", Some("thread-route"));
    goal.title = "Resume release work".to_string();
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now;
    goal.current_step_title = Some("publish the package".to_string());
    engine.goal_runs.lock().await.push_back(goal);
    engine
        .set_goal_run_client_surface("goal-route", zorai_protocol::ClientSurface::Tui)
        .await;
    engine.mark_operator_present("test").await;

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "morning_brief")
        .expect("expected a morning brief");
    assert_eq!(item.preferred_client_surface.as_deref(), Some("tui"));
}

#[tokio::test]
async fn morning_brief_uses_persisted_active_goal_after_live_queue_clear() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let now = now_millis();
    let mut goal = sample_goal_run("goal-persisted-brief", Some("thread-persisted-brief"));
    goal.title = "Resume persisted release work".to_string();
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now;
    goal.current_step_title = Some("publish persisted package".to_string());
    engine.goal_runs.lock().await.push_back(goal);
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();
    engine.mark_operator_present("test").await;

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "morning_brief")
        .expect("expected a morning brief from persisted active goal rows");
    assert_eq!(item.goal_run_id.as_deref(), Some("goal-persisted-brief"));
    assert_eq!(item.thread_id.as_deref(), Some("thread-persisted-brief"));
    assert!(
        item.bullets
            .iter()
            .any(|bullet| bullet.contains("Resume persisted release work")),
        "morning brief should summarize persisted active goals"
    );
}

#[tokio::test]
async fn anticipatory_updates_are_persisted_as_inbox_notifications() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut item = sample_anticipatory_item(
        "digest-1",
        "stuck_hint",
        "Task May Be Stuck",
        "Release packaging has stalled for 28m.",
    );
    item.bullets = vec![
        "No terminal output has arrived since the last retry.".to_string(),
        "Operator attention is focused on the release thread.".to_string(),
    ];
    item.confidence = 0.73;
    item.thread_id = Some("thread-release".to_string());
    item.created_at = 10;
    item.updated_at = 20;

    engine.emit_anticipatory_update(vec![item]).await;

    let notifications = engine
        .history
        .list_notifications(false, Some(10))
        .await
        .unwrap();
    let notification = notifications
        .iter()
        .find(|candidate| candidate.id == "anticipatory:digest-1")
        .expect("anticipatory notification should be persisted");

    assert_eq!(notification.source, "anticipatory");
    assert_eq!(notification.kind, "stuck_hint");
    assert_eq!(notification.title, "Task May Be Stuck");
    assert!(notification.body.contains("Release packaging has stalled"));
    assert!(notification.body.contains("No terminal output has arrived"));
    assert_eq!(notification.severity, "warning");
    assert_eq!(notification.archived_at, None);
    assert_eq!(notification.actions.len(), 1);
    assert_eq!(notification.actions[0].action_type, "open_thread");
    assert_eq!(
        notification.actions[0].target.as_deref(),
        Some("thread-release")
    );
}

#[tokio::test]
async fn anticipatory_updates_archive_stale_notifications() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut item_one = sample_anticipatory_item(
        "digest-1",
        "stuck_hint",
        "Task May Be Stuck",
        "Thread A looks blocked.",
    );
    item_one.confidence = 0.82;
    item_one.thread_id = Some("thread-a".to_string());
    item_one.created_at = 10;
    item_one.updated_at = 20;

    let mut item_two = sample_anticipatory_item(
        "digest-2",
        "intent_prediction",
        "Likely Next Action",
        "You will probably inspect the release checklist.",
    );
    item_two.confidence = 0.65;
    item_two.thread_id = Some("thread-b".to_string());
    item_two.created_at = 11;
    item_two.updated_at = 21;

    engine
        .emit_anticipatory_update(vec![item_one, item_two])
        .await;

    let mut updated_item = sample_anticipatory_item(
        "digest-1",
        "stuck_hint",
        "Task May Be Stuck",
        "Thread A still looks blocked.",
    );
    updated_item.confidence = 0.84;
    updated_item.thread_id = Some("thread-a".to_string());
    updated_item.created_at = 10;
    updated_item.updated_at = 30;

    engine.emit_anticipatory_update(vec![updated_item]).await;

    let notifications = engine
        .history
        .list_notifications(true, Some(10))
        .await
        .unwrap();
    let active = notifications
        .iter()
        .find(|candidate| candidate.id == "anticipatory:digest-1")
        .expect("active anticipatory notification should remain");
    let archived = notifications
        .iter()
        .find(|candidate| candidate.id == "anticipatory:digest-2")
        .expect("stale anticipatory notification should remain in history");

    assert_eq!(active.archived_at, None);
    assert!(archived.archived_at.is_some());
    assert_eq!(archived.source, "anticipatory");
}

#[tokio::test]
async fn strained_satisfaction_suppresses_optional_morning_brief() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let now = now_millis();
    let mut goal = sample_goal_run("goal-strained", Some("thread-strained"));
    goal.title = "Resume degraded flow".to_string();
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now;
    goal.current_step_title = Some("retry the command".to_string());
    engine.goal_runs.lock().await.push_back(goal);
    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.operator_satisfaction.score = 0.22;
        model.operator_satisfaction.label = "strained".to_string();
    }
    engine.mark_operator_present("test").await;

    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .items
            .iter()
            .all(|item| item.kind != "morning_brief"),
        "strained satisfaction should suppress optional morning briefs to reduce proactive noise"
    );
}

#[tokio::test]
async fn tool_hesitation_suppresses_optional_morning_brief() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let now = now_millis();
    let mut goal = sample_goal_run("goal-hesitation-brief", Some("thread-hesitation-brief"));
    goal.title = "Resume degraded flow".to_string();
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now;
    goal.current_step_title = Some("retry the command".to_string());
    engine.goal_runs.lock().await.push_back(goal);
    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.implicit_feedback.tool_hesitation_count = 1;
        refresh_operator_satisfaction(&mut model);
    }
    engine.mark_operator_present("test").await;

    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .items
            .iter()
            .all(|item| item.kind != "morning_brief"),
        "tool hesitation should suppress optional morning briefs to reduce proactive noise"
    );
}

#[tokio::test]
async fn historical_tool_hesitation_rate_does_not_suppress_optional_morning_brief() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let now = now_millis();
    let mut goal = sample_goal_run("goal-recovered-brief", Some("thread-recovered-brief"));
    goal.title = "Resume recovered flow".to_string();
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now;
    goal.current_step_title = Some("continue the active work".to_string());
    engine.goal_runs.lock().await.push_back(goal);
    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 35;
        model.attention_topology.focus_event_count = 555;
        model.attention_topology.rapid_switch_count = 294;
        model.implicit_feedback.tool_hesitation_count = 47;
        model.implicit_feedback.revision_message_count = 5;
        model.implicit_feedback.rapid_revert_count = 13;
        refresh_operator_satisfaction(&mut model);
    }
    engine.mark_operator_present("test").await;

    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .items
            .iter()
            .any(|item| item.kind == "morning_brief"),
        "old friction should not permanently suppress optional morning briefs"
    );
}

#[tokio::test]
async fn slow_approval_latency_surfaces_proactive_suppression_transparency() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.stuck_detection = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut task = sample_task("task-latency-intent", Some("thread-latency-intent"), None);
    task.title = "Need approval".to_string();
    task.status = TaskStatus::AwaitingApproval;
    engine.tasks.lock().await.push_back(task);
    engine
        .record_operator_attention("conversation:chat", Some("thread-latency-intent"), None)
        .await
        .unwrap();
    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.risk_fingerprint.approval_requests = 4;
        model.risk_fingerprint.approvals = 2;
        model.risk_fingerprint.denials = 2;
        model.risk_fingerprint.avg_response_time_secs = 45.0;
        refresh_risk_metrics(&mut model.risk_fingerprint);
        refresh_operator_satisfaction(&mut model);
    }

    engine.run_anticipatory_tick().await;

    let item = engine
        .anticipatory
        .read()
        .await
        .items
        .iter()
        .find(|item| item.kind == "proactive_suppression")
        .cloned()
        .expect("expected a proactive suppression transparency item");
    assert_eq!(item.thread_id.as_deref(), Some("thread-latency-intent"));
    assert!(item.summary.contains("suppressed") || item.summary.contains("tightened"));
    assert!(item
        .bullets
        .iter()
        .any(|bullet| bullet.contains("approval latency")
            || bullet.contains("reduce proactive noise")));
}

#[tokio::test]
async fn slow_approval_latency_suppresses_optional_intent_prediction() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.stuck_detection = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut task = sample_task("task-latency-intent", Some("thread-latency-intent"), None);
    task.title = "Need approval".to_string();
    task.status = TaskStatus::AwaitingApproval;
    engine.tasks.lock().await.push_back(task);
    engine
        .record_operator_attention("conversation:chat", Some("thread-latency-intent"), None)
        .await
        .unwrap();
    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.risk_fingerprint.approval_requests = 4;
        model.risk_fingerprint.approvals = 2;
        model.risk_fingerprint.denials = 2;
        model.risk_fingerprint.avg_response_time_secs = 45.0;
        refresh_risk_metrics(&mut model.risk_fingerprint);
        refresh_operator_satisfaction(&mut model);
    }

    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .items
            .iter()
            .all(|item| item.kind != "intent_prediction"),
        "slow approval latency should suppress optional intent prediction to reduce proactive noise"
    );
}

#[tokio::test]
async fn strained_satisfaction_skips_predictive_hydration() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let now = now_millis();
    let mut goal = sample_goal_run("goal-hydration", Some("thread-hydration"));
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now;
    engine.goal_runs.lock().await.push_back(goal);
    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.operator_satisfaction.score = 0.24;
        model.operator_satisfaction.label = "strained".to_string();
    }

    engine.run_anticipatory_tick().await;

    assert!(
        !engine
            .anticipatory
            .read()
            .await
            .hydration_by_thread
            .contains_key("thread-hydration"),
        "strained satisfaction should skip predictive hydration so the daemon reduces background churn"
    );
}

#[tokio::test]
async fn anticipatory_tick_surfaces_intent_prediction_for_pending_approval() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.stuck_detection = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut task = sample_task("task-approval", Some("thread-intent"), None);
    task.title = "Need approval".to_string();
    task.status = TaskStatus::AwaitingApproval;
    engine.tasks.lock().await.push_back(task);
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();
    engine
        .record_operator_attention("conversation:chat", Some("thread-intent"), None)
        .await
        .unwrap();

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "intent_prediction")
        .expect("expected an intent prediction item");
    let payload = item
        .intent_prediction
        .as_ref()
        .expect("intent prediction payload should be present");
    assert_eq!(payload.primary_action, "review pending approval");
    assert_eq!(
        payload.ranked_actions.len(),
        3,
        "intent prediction should surface ranked next actions"
    );
    assert_eq!(payload.ranked_actions[0].rank, 1);
    assert_eq!(payload.ranked_actions[0].action, "review pending approval");
    assert!(payload.ranked_actions[0].confidence >= 0.86);
    assert_eq!(item.thread_id.as_deref(), Some("thread-intent"));
    assert!(item.summary.contains("review pending approval"));
    assert!(item.confidence >= 0.86);
}

#[tokio::test]
async fn anticipatory_tick_surfaces_intent_prediction_for_repo_change_context() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = false;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-repo-intent"), None)
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-repo-intent".to_string(),
        ThreadWorkContext {
            thread_id: "thread-repo-intent".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/main.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some("/tmp/repo".to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "intent_prediction")
        .expect("expected an intent prediction item");
    let payload = item
        .intent_prediction
        .as_ref()
        .expect("intent prediction payload should be present");
    assert_eq!(
        payload.primary_action,
        "inspect or test recent repo changes"
    );
    assert_eq!(
        payload.ranked_actions.len(),
        3,
        "intent prediction should surface ranked next actions"
    );
    assert_eq!(payload.ranked_actions[0].rank, 1);
    assert_eq!(
        payload.ranked_actions[0].action,
        "inspect or test recent repo changes"
    );
    assert_eq!(item.thread_id.as_deref(), Some("thread-repo-intent"));
    assert!(item.summary.contains("inspect or test recent repo changes"));
    assert!(item
        .bullets
        .iter()
        .any(|bullet| bullet.contains("repo-linked")));
}

#[tokio::test]
async fn strained_satisfaction_suppresses_intent_prediction() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-strained-intent"), None)
        .await
        .unwrap();
    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.operator_satisfaction.score = 0.20;
        model.operator_satisfaction.label = "strained".to_string();
    }

    engine.run_anticipatory_tick().await;

    assert!(engine
        .anticipatory
        .read()
        .await
        .items
        .iter()
        .all(|item| item.kind != "intent_prediction"));
}

#[tokio::test]
async fn tool_hesitation_tightens_predictive_hydration_to_active_attention_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-focus"), None)
        .await
        .unwrap();

    let now = now_millis();
    let mut focused_goal = sample_goal_run("goal-focus", Some("thread-focus"));
    focused_goal.status = GoalRunStatus::Running;
    focused_goal.updated_at = now;
    engine.goal_runs.lock().await.push_back(focused_goal);

    let mut other_goal = sample_goal_run("goal-other", Some("thread-other"));
    other_goal.status = GoalRunStatus::Running;
    other_goal.updated_at = now.saturating_sub(1_000);
    engine.goal_runs.lock().await.push_back(other_goal);

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.implicit_feedback.tool_hesitation_count = 1;
        refresh_operator_satisfaction(&mut model);
    }

    engine.run_anticipatory_tick().await;

    let hydration = engine.anticipatory.read().await.hydration_by_thread.clone();
    assert!(
        hydration.contains_key("thread-focus"),
        "active attention thread should still be hydrated"
    );
    assert!(
        !hydration.contains_key("thread-other"),
        "tool hesitation should tighten predictive hydration to the active attention thread"
    );
}

#[tokio::test]
async fn slow_approval_latency_tightens_predictive_hydration_to_active_attention_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-focus"), None)
        .await
        .unwrap();

    let now = now_millis();
    let mut focused_goal = sample_goal_run("goal-focus", Some("thread-focus"));
    focused_goal.status = GoalRunStatus::Running;
    focused_goal.updated_at = now;
    engine.goal_runs.lock().await.push_back(focused_goal);

    let mut other_goal = sample_goal_run("goal-other", Some("thread-other"));
    other_goal.status = GoalRunStatus::Running;
    other_goal.updated_at = now.saturating_sub(1_000);
    engine.goal_runs.lock().await.push_back(other_goal);

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.risk_fingerprint.approval_requests = 4;
        model.risk_fingerprint.approvals = 2;
        model.risk_fingerprint.denials = 2;
        model.risk_fingerprint.avg_response_time_secs = 45.0;
        refresh_risk_metrics(&mut model.risk_fingerprint);
        refresh_operator_satisfaction(&mut model);
    }

    engine.run_anticipatory_tick().await;

    let hydration = engine.anticipatory.read().await.hydration_by_thread.clone();
    assert!(
        hydration.contains_key("thread-focus"),
        "active attention thread should still be hydrated"
    );
    assert!(
        !hydration.contains_key("thread-other"),
        "slow approval latency should tighten predictive hydration to the active attention thread"
    );
}

#[tokio::test]
async fn awaiting_approval_thread_overrides_tightened_predictive_hydration_target() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-focus"), None)
        .await
        .unwrap();

    let now = now_millis();
    let mut focus_goal = sample_goal_run("goal-focus", Some("thread-focus"));
    focus_goal.status = GoalRunStatus::Running;
    focus_goal.updated_at = now;
    engine.goal_runs.lock().await.push_back(focus_goal);

    let mut approval_goal = sample_goal_run("goal-approval", Some("thread-approval"));
    approval_goal.status = GoalRunStatus::Running;
    approval_goal.updated_at = now.saturating_sub(1_000);
    engine.goal_runs.lock().await.push_back(approval_goal);

    let mut approval_task = sample_task("task-awaiting", Some("thread-approval"), None);
    approval_task.status = TaskStatus::AwaitingApproval;
    engine.tasks.lock().await.push_back(approval_task);
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.implicit_feedback.tool_hesitation_count = 1;
        refresh_operator_satisfaction(&mut model);
    }

    engine.run_anticipatory_tick().await;

    let hydration = engine.anticipatory.read().await.hydration_by_thread.clone();
    assert!(
        hydration.contains_key("thread-approval"),
        "awaiting approval thread should be hydrated even when tightening is active"
    );
    assert!(
        !hydration.contains_key("thread-focus"),
        "approval-gated thread should override ordinary attention-target tightening"
    );
}

#[tokio::test]
async fn awaiting_approval_persisted_goal_overrides_tightened_predictive_hydration_target_after_live_queue_clear(
) {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-focus"), None)
        .await
        .unwrap();

    let now = now_millis();
    let mut focus_goal = sample_goal_run("goal-focus-persisted-gate", Some("thread-focus"));
    focus_goal.status = GoalRunStatus::Running;
    focus_goal.updated_at = now;
    engine.goal_runs.lock().await.push_back(focus_goal);

    let mut approval_goal =
        sample_goal_run("goal-approval-persisted-gate", Some("thread-approval"));
    approval_goal.status = GoalRunStatus::AwaitingApproval;
    approval_goal.updated_at = now.saturating_sub(1_000);
    engine.goal_runs.lock().await.push_back(approval_goal);
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.implicit_feedback.tool_hesitation_count = 1;
        refresh_operator_satisfaction(&mut model);
    }

    engine.run_anticipatory_tick().await;

    let hydration = engine.anticipatory.read().await.hydration_by_thread.clone();
    assert!(
        hydration.contains_key("thread-approval"),
        "persisted awaiting-approval goal thread should override tightened active attention"
    );
    assert!(
        !hydration.contains_key("thread-focus"),
        "approval-gated persisted goal thread should override ordinary attention-target tightening"
    );
}

#[tokio::test]
async fn predictive_hydration_populates_prewarm_cache_for_hydrated_thread() {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-predictive-cache");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    enable_repo_monitor_for_workspace_root(&engine, root.path()).await;

    let mut goal = sample_goal_run("goal-cache", Some("thread-cache"));
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now_millis();
    engine.goal_runs.lock().await.push_back(goal);
    engine.thread_work_contexts.write().await.insert(
        "thread-cache".to_string(),
        ThreadWorkContext {
            thread_id: "thread-cache".to_string(),
            entries: vec![WorkContextEntry {
                path: "Cargo.toml".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    engine.run_anticipatory_tick().await;

    let runtime = engine.anticipatory.read().await;
    let snapshot = runtime
        .prewarm_cache_by_thread
        .get("thread-cache")
        .expect("prewarm cache snapshot for hydrated thread");
    assert!(snapshot.summary.contains("branch"));
    assert!(snapshot.summary.contains("context entries 1"));
}

#[tokio::test]
async fn predictive_hydration_skips_prewarm_cache_when_repo_monitor_disabled() {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-predictive-cache-disabled");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut goal = sample_goal_run("goal-cache-disabled", Some("thread-cache-disabled"));
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now_millis();
    engine.goal_runs.lock().await.push_back(goal);
    engine.thread_work_contexts.write().await.insert(
        "thread-cache-disabled".to_string(),
        ThreadWorkContext {
            thread_id: "thread-cache-disabled".to_string(),
            entries: vec![WorkContextEntry {
                path: "Cargo.toml".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    engine.run_anticipatory_tick().await;

    let runtime = engine.anticipatory.read().await;
    assert!(
        !runtime
            .prewarm_cache_by_thread
            .contains_key("thread-cache-disabled"),
        "predictive hydration should skip git-backed prewarm work when repo monitor is disabled"
    );
}

#[tokio::test]
async fn predictive_hydration_uses_persisted_active_goal_thread_after_live_queue_clear() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut goal = sample_goal_run("goal-predictive-persisted", Some("thread-predictive-goal"));
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now_millis();
    engine.goal_runs.lock().await.push_back(goal);
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();

    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .hydration_by_thread
            .contains_key("thread-predictive-goal"),
        "predictive hydration should target active goal threads from persisted goal rows"
    );
}

#[tokio::test]
async fn predictive_hydration_records_causal_trace_family() {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-predictive-trace");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    enable_repo_monitor_for_workspace_root(&engine, root.path()).await;

    let mut goal = sample_goal_run("goal-trace", Some("thread-trace"));
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now_millis();
    engine.goal_runs.lock().await.push_back(goal);
    engine.thread_work_contexts.write().await.insert(
        "thread-trace".to_string(),
        ThreadWorkContext {
            thread_id: "thread-trace".to_string(),
            entries: vec![WorkContextEntry {
                path: "Cargo.toml".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    engine.run_anticipatory_tick().await;

    let traces = engine
        .history
        .list_recent_causal_trace_records("predictive_hydration", 1)
        .await
        .expect("list predictive hydration traces");
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].trace_family, "predictive_hydration");
}

#[tokio::test]
async fn resolved_intent_predictions_raise_future_prediction_confidence() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-confidence"), None)
        .await
        .unwrap();

    let mut task = sample_task("task-confidence", Some("thread-confidence"), None);
    task.title = "Need approval".to_string();
    task.status = TaskStatus::AwaitingApproval;
    task.awaiting_approval_id = Some("approval-confidence".to_string());
    engine
        .history
        .upsert_agent_task(&task)
        .await
        .expect("persist task");
    engine.tasks.lock().await.push_back(task);

    for (id, was_correct, created_at_ms) in [
        ("intent-hist-1", true, 100),
        ("intent-hist-2", true, 200),
        ("intent-hist-3", false, 300),
    ] {
        engine
            .history
            .insert_intent_prediction(&IntentPredictionRow {
                id: id.to_string(),
                session_id: "thread-other".to_string(),
                context_state_hash: format!("ctx-{id}"),
                predicted_action: "review pending approval".to_string(),
                confidence: 0.80,
                actual_action: Some("review pending approval".to_string()),
                was_correct: Some(was_correct),
                created_at_ms,
            })
            .await
            .expect("seed resolved prediction history");
    }

    engine.run_anticipatory_tick().await;

    let item = engine
        .anticipatory
        .read()
        .await
        .items
        .clone()
        .into_iter()
        .find(|candidate| candidate.kind == "intent_prediction")
        .expect("expected intent prediction item");
    let payload = item
        .intent_prediction
        .as_ref()
        .expect("intent prediction payload should be present");

    assert!(
        payload.confidence > 0.86,
        "persisted success-rate priors should raise confidence above the raw heuristic baseline"
    );
    assert_eq!(payload.primary_action, "review pending approval");
}

#[tokio::test]
async fn intent_prediction_persists_and_resolves_when_operator_action_matches() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut task = sample_task("task-approval-persist", Some("thread-intent-persist"), None);
    task.title = "Need approval".to_string();
    task.status = TaskStatus::AwaitingApproval;
    task.awaiting_approval_id = Some("approval-intent-persist".to_string());
    engine
        .history
        .upsert_agent_task(&task)
        .await
        .expect("persist task");
    engine.tasks.lock().await.push_back(task);
    engine
        .record_operator_attention("conversation:chat", Some("thread-intent-persist"), None)
        .await
        .unwrap();

    engine.run_anticipatory_tick().await;

    let before = engine
        .history
        .list_intent_predictions("thread-intent-persist", 10)
        .await
        .expect("list persisted intent predictions before resolution");
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].predicted_action, "review pending approval");
    assert_eq!(before[0].was_correct, None);

    engine
        .record_operator_message(
            "thread-intent-persist",
            "please review the approval first",
            false,
        )
        .await
        .expect("record operator message");

    let after = engine
        .history
        .list_intent_predictions("thread-intent-persist", 10)
        .await
        .expect("list persisted intent predictions after resolution");
    assert_eq!(after.len(), 1);
    assert_eq!(
        after[0].actual_action.as_deref(),
        Some("review pending approval")
    );
    assert_eq!(after[0].was_correct, Some(true));
}

#[tokio::test]
async fn intent_prediction_resolution_treats_inspect_changes_as_repo_verification_action() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    config.anticipatory.predictive_hydration = false;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention(
            "conversation:chat",
            Some("thread-intent-repo-resolve"),
            None,
        )
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-intent-repo-resolve".to_string(),
        ThreadWorkContext {
            thread_id: "thread-intent-repo-resolve".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some("/tmp/repo".to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    engine.run_anticipatory_tick().await;

    let before = engine
        .history
        .list_intent_predictions("thread-intent-repo-resolve", 10)
        .await
        .expect("list persisted intent predictions before inspect resolution");
    assert_eq!(before.len(), 1);
    assert_eq!(
        before[0].predicted_action,
        "inspect or test recent repo changes"
    );
    assert_eq!(before[0].was_correct, None);

    engine
        .record_operator_message(
            "thread-intent-repo-resolve",
            "please inspect the recent changes first",
            false,
        )
        .await
        .expect("record operator inspect message");

    let after = engine
        .history
        .list_intent_predictions("thread-intent-repo-resolve", 10)
        .await
        .expect("list persisted intent predictions after inspect resolution");
    assert_eq!(after.len(), 1);
    assert_eq!(
        after[0].actual_action.as_deref(),
        Some("inspect or test recent repo changes")
    );
    assert_eq!(after[0].was_correct, Some(true));
}

#[tokio::test]
async fn intent_prediction_updates_active_prediction_instead_of_duplicating_when_confidence_changes(
) {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-intent-update"), None)
        .await
        .unwrap();

    let mut task = sample_task("task-intent-update", Some("thread-intent-update"), None);
    task.title = "Need approval".to_string();
    task.status = TaskStatus::AwaitingApproval;
    task.awaiting_approval_id = Some("approval-intent-update".to_string());
    engine
        .history
        .upsert_agent_task(&task)
        .await
        .expect("persist task");
    engine.tasks.lock().await.push_back(task);

    engine.run_anticipatory_tick().await;

    let before = engine
        .history
        .list_intent_predictions("thread-intent-update", 10)
        .await
        .expect("list initial intent predictions");
    assert_eq!(before.len(), 1);
    let initial_confidence = before[0].confidence;

    engine
        .history
        .insert_intent_prediction(&IntentPredictionRow {
            id: "intent-prior-1".to_string(),
            session_id: "thread-prior".to_string(),
            context_state_hash: "ctx-prior-1".to_string(),
            predicted_action: "review pending approval".to_string(),
            confidence: 0.90,
            actual_action: Some("review pending approval".to_string()),
            was_correct: Some(true),
            created_at_ms: now_millis(),
        })
        .await
        .expect("seed first resolved intent prior");
    engine
        .history
        .insert_intent_prediction(&IntentPredictionRow {
            id: "intent-prior-2".to_string(),
            session_id: "thread-prior".to_string(),
            context_state_hash: "ctx-prior-2".to_string(),
            predicted_action: "review pending approval".to_string(),
            confidence: 0.90,
            actual_action: Some("review pending approval".to_string()),
            was_correct: Some(true),
            created_at_ms: now_millis().saturating_add(1),
        })
        .await
        .expect("seed second resolved intent prior");

    {
        let mut runtime = engine.anticipatory.write().await;
        runtime.last_surface_at = None;
    }

    engine.run_anticipatory_tick().await;

    let predictions = engine
        .history
        .list_intent_predictions("thread-intent-update", 10)
        .await
        .expect("list updated intent predictions");
    let unresolved = predictions
        .iter()
        .filter(|row| row.was_correct.is_none() && row.actual_action.is_none())
        .collect::<Vec<_>>();

    assert_eq!(
        unresolved.len(),
        1,
        "confidence changes for the same unresolved intent prediction should update the active prediction instead of duplicating it"
    );
    assert!(
        unresolved[0].confidence >= initial_confidence,
        "updated unresolved intent prediction should retain or improve confidence after stronger evidence"
    );
}

#[tokio::test]
async fn system_outcome_foresight_does_not_persist_duplicate_unresolved_predictions() {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-foresight-dedupe");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join("src/lib.rs"), "pub fn broken() {}\n").unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    enable_repo_monitor_for_workspace_root(&engine, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-foresight-dedupe"), None)
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-foresight-dedupe".to_string(),
        ThreadWorkContext {
            thread_id: "thread-foresight-dedupe".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );
    engine
        .history
        .insert_health_log(
            "health-foresight-dedupe-degraded",
            "task",
            "cargo-test",
            "degraded",
            Some("{\"tool\":\"cargo test\",\"error\":\"Command failed\"}"),
            Some("recent cargo test failed in this repo"),
            now_millis(),
        )
        .await
        .expect("save degraded health log");

    engine.run_anticipatory_tick().await;
    engine.run_anticipatory_tick().await;

    let predictions = engine
        .history
        .list_system_outcome_predictions("thread-foresight-dedupe", 10)
        .await
        .expect("list persisted system outcome predictions after duplicate ticks");
    let unresolved = predictions
        .iter()
        .filter(|row| row.was_correct.is_none() && row.actual_outcome.is_none())
        .collect::<Vec<_>>();

    assert_eq!(
        unresolved.len(),
        1,
        "repeated identical foresight ticks should not persist duplicate unresolved predictions"
    );
}

#[tokio::test]
async fn system_outcome_foresight_updates_active_prediction_instead_of_duplicating_when_confidence_changes(
) {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-foresight-update");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join("src/lib.rs"), "pub fn broken() {}\n").unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    enable_repo_monitor_for_workspace_root(&engine, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-foresight-update"), None)
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-foresight-update".to_string(),
        ThreadWorkContext {
            thread_id: "thread-foresight-update".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );
    engine
        .history
        .insert_health_log(
            "health-foresight-update-1",
            "task",
            "cargo-test",
            "degraded",
            Some("{\"tool\":\"cargo test\",\"error\":\"Command failed\"}"),
            Some("recent cargo test failed in this repo"),
            now_millis() - 2_000,
        )
        .await
        .expect("save first degraded health log");

    engine.run_anticipatory_tick().await;

    let before = engine
        .history
        .list_system_outcome_predictions("thread-foresight-update", 10)
        .await
        .expect("list initial system outcome predictions");
    assert_eq!(before.len(), 1);
    let initial_confidence = before[0].confidence;

    engine
        .history
        .insert_health_log(
            "health-foresight-update-2",
            "task",
            "cargo-test",
            "degraded",
            Some("{\"tool\":\"cargo test\",\"error\":\"Command failed\"}"),
            Some("recent cargo test failed in this repo"),
            now_millis() - 1_000,
        )
        .await
        .expect("save second degraded health log");

    engine.run_anticipatory_tick().await;

    let predictions = engine
        .history
        .list_system_outcome_predictions("thread-foresight-update", 10)
        .await
        .expect("list updated system outcome predictions");
    let unresolved = predictions
        .iter()
        .filter(|row| row.was_correct.is_none() && row.actual_outcome.is_none())
        .collect::<Vec<_>>();

    assert_eq!(
        unresolved.len(),
        1,
        "confidence changes for the same unresolved foresight should update the active prediction instead of duplicating it"
    );
    assert!(
        unresolved[0].confidence >= initial_confidence,
        "updated unresolved prediction should retain or improve confidence after stronger evidence"
    );
}

#[tokio::test]
async fn system_outcome_foresight_persists_and_resolves_when_health_feedback_arrives() {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-foresight-persist");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join("src/lib.rs"), "pub fn broken() {}\n").unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    enable_repo_monitor_for_workspace_root(&engine, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-foresight-persist"), None)
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-foresight-persist".to_string(),
        ThreadWorkContext {
            thread_id: "thread-foresight-persist".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );
    engine
        .history
        .insert_health_log(
            "health-foresight-persist-degraded",
            "task",
            "cargo-test",
            "degraded",
            Some("{\"tool\":\"cargo test\",\"error\":\"Command failed\"}"),
            Some("recent cargo test failed in this repo"),
            now_millis(),
        )
        .await
        .expect("save degraded health log");

    engine.run_anticipatory_tick().await;

    let before = engine
        .history
        .list_system_outcome_predictions("thread-foresight-persist", 10)
        .await
        .expect("list persisted system outcome predictions before resolution");
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].prediction_type, "build_test_risk");
    assert_eq!(before[0].predicted_outcome, "build/test failure");
    assert!(before[0].confidence >= 0.7);
    assert_eq!(before[0].was_correct, None);

    engine
        .resolve_system_outcome_prediction_feedback("thread-foresight-persist", "healthy")
        .await;

    let after = engine
        .history
        .list_system_outcome_predictions("thread-foresight-persist", 10)
        .await
        .expect("list persisted system outcome predictions after resolution");
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].actual_outcome.as_deref(), Some("healthy"));
    assert_eq!(after[0].was_correct, Some(false));
}

#[tokio::test]
async fn intent_prediction_includes_cached_prewarm_summary_when_available() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = false;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-cache-bullets"), None)
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-cache-bullets".to_string(),
        ThreadWorkContext {
            thread_id: "thread-cache-bullets".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/main.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some("/tmp/repo".to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );
    engine.anticipatory.write().await.prewarm_cache_by_thread.insert(
        "thread-cache-bullets".to_string(),
        AnticipatoryPrewarmSnapshot {
            summary: "branch main; dirty=true; modified 1; staged 0; untracked 0; ahead 0; behind 0; context entries 1".to_string(),
            precomputation_id: None,
        },
    );

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "intent_prediction")
        .expect("expected an intent prediction item");
    let payload = item
        .intent_prediction
        .as_ref()
        .expect("intent prediction payload should be present");
    assert!(payload
        .ranked_actions
        .iter()
        .any(|candidate| candidate.rationale.contains("Cached prewarm")));
    assert!(item
        .bullets
        .iter()
        .any(|bullet| bullet.contains("Cached prewarm:")));
}

#[tokio::test]
async fn anticipatory_tick_surfaces_persisted_system_outcome_foresight_for_build_risk() {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-build-risk");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo_root.join("src")).unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    enable_repo_monitor_for_workspace_root(&engine, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-build-risk"), None)
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-build-risk".to_string(),
        ThreadWorkContext {
            thread_id: "thread-build-risk".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );
    std::fs::write(repo_root.join("src/lib.rs"), "pub fn broken() {}\n").unwrap();
    engine
        .history
        .insert_health_log(
            "health-build-risk",
            "task",
            "cargo-test",
            "degraded",
            Some("{\"tool\":\"cargo test\",\"error\":\"Command failed\"}"),
            Some("recent cargo test failed in this repo"),
            now_millis(),
        )
        .await
        .expect("save health log");

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "system_outcome_foresight")
        .expect("expected a system-outcome foresight item");
    assert_eq!(item.thread_id.as_deref(), Some("thread-build-risk"));
    assert!(item.summary.contains("build/test failure risk"));
    assert!(item.confidence >= 0.7);
    assert!(item
        .bullets
        .iter()
        .any(|bullet| bullet.contains("prediction_type=build_test_risk")));
    assert!(item
        .bullets
        .iter()
        .any(|bullet| bullet.contains("recent cargo test failed")));
    assert!(item
        .bullets
        .iter()
        .any(|bullet| bullet.contains("dirty repo state")));
}

#[tokio::test]
async fn build_test_risk_confidence_increases_with_repeated_degraded_health_entries() {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-build-confidence");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join("src/lib.rs"), "pub fn broken() {}\n").unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    enable_repo_monitor_for_workspace_root(&engine, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-build-confidence"), None)
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-build-confidence".to_string(),
        ThreadWorkContext {
            thread_id: "thread-build-confidence".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    engine
        .history
        .insert_health_log(
            "health-build-confidence-1",
            "task",
            "cargo-test",
            "degraded",
            Some("{\"tool\":\"cargo test\",\"error\":\"Command failed\"}"),
            Some("recent cargo test failed in this repo"),
            now_millis() - 2_000,
        )
        .await
        .expect("save first health log");
    let settings = engine.config.read().await.anticipatory.clone();
    let single_confidence = engine
        .compute_system_outcome_foresight(&settings)
        .await
        .map(|item| item.confidence)
        .expect("expected foresight item after one degraded health entry");

    engine
        .history
        .insert_health_log(
            "health-build-confidence-2",
            "task",
            "cargo-test",
            "degraded",
            Some("{\"tool\":\"cargo test\",\"error\":\"Command failed\"}"),
            Some("recent cargo test failed in this repo"),
            now_millis() - 1_000,
        )
        .await
        .expect("save second health log");
    let repeated_confidence = engine
        .compute_system_outcome_foresight(&settings)
        .await
        .map(|item| item.confidence)
        .expect("expected foresight item after repeated degraded health entries");

    assert!((0.0..=1.0).contains(&single_confidence));
    assert!((0.0..=1.0).contains(&repeated_confidence));
    assert!(
        repeated_confidence > single_confidence,
        "expected repeated degraded build/test health evidence to raise foresight confidence"
    );
}

#[tokio::test]
async fn system_outcome_foresight_skips_build_risk_when_repo_monitor_disabled() {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-build-risk-disabled");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join("src/lib.rs"), "pub fn broken() {}\n").unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention(
            "conversation:chat",
            Some("thread-build-risk-disabled"),
            None,
        )
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-build-risk-disabled".to_string(),
        ThreadWorkContext {
            thread_id: "thread-build-risk-disabled".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );
    engine
        .history
        .insert_health_log(
            "health-build-risk-disabled",
            "task",
            "cargo-test",
            "degraded",
            Some("{\"tool\":\"cargo test\",\"error\":\"Command failed\"}"),
            Some("recent cargo test failed in this repo"),
            now_millis(),
        )
        .await
        .expect("save health log");

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    assert!(
        items.into_iter()
            .all(|candidate| candidate.kind != "system_outcome_foresight"),
        "system outcome foresight should skip git-backed build-risk analysis when repo monitor is disabled"
    );
}

#[tokio::test]
async fn anticipatory_tick_surfaces_stale_context_foresight_when_hydration_lags_session_rhythm() {
    let root = tempdir().unwrap();
    let repo_root = root.path().join("repo-stale-context");
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::write(repo_root.join("src/lib.rs"), "pub fn stale() {}\n").unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = false;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    enable_repo_monitor_for_workspace_root(&engine, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-stale-context"), None)
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-stale-context".to_string(),
        ThreadWorkContext {
            thread_id: "thread-stale-context".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );
    {
        let mut runtime = engine.anticipatory.write().await;
        runtime.hydration_by_thread.insert(
            "thread-stale-context".to_string(),
            now_millis() - 16 * 60 * 1000,
        );
    }
    {
        let mut model = engine.operator_model.write().await;
        model.session_rhythm.session_count = 6;
        model.session_rhythm.session_duration_avg_minutes = 10.0;
        model.session_rhythm.typical_start_hour_utc = Some(9);
    }

    engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: "thread-stale-context".to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Stale context thread".to_string(),
            created_at: 1,
            updated_at: now_millis() as i64,
            message_count: 2,
            total_tokens: 0,
            last_preview: "off-topic drift".to_string(),
            metadata_json: None,
        })
        .await
        .expect("seed thread row");
    engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            reasoning_tokens: None,
            thinking_ms: None,
            id: "stale-user-1".to_string(),
            thread_id: "thread-stale-context".to_string(),
            created_at: (now_millis() - 2_000) as i64,
            role: "user".to_string(),
            content: "Let's switch topics completely and talk about vacation photos.".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed user message");
    engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            reasoning_tokens: None,
            thinking_ms: None,
            id: "stale-assistant-1".to_string(),
            thread_id: "thread-stale-context".to_string(),
            created_at: (now_millis() - 1_000) as i64,
            role: "assistant".to_string(),
            content: "The recent repo context may be stale relative to the current conversation."
                .to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed assistant message");

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "system_outcome_foresight")
        .expect("expected a system-outcome foresight item");
    assert_eq!(item.thread_id.as_deref(), Some("thread-stale-context"));
    assert!(item.summary.contains("stale context"));
    assert!(item.confidence >= 0.7);
    assert!(item
        .bullets
        .iter()
        .any(|bullet| bullet.contains("prediction_type=stale_context")));
    assert!(item
        .bullets
        .iter()
        .any(|bullet| bullet.contains("hydration age")));
    assert!(item
        .bullets
        .iter()
        .any(|bullet| bullet.contains("semantic alignment degraded")));
}

#[tokio::test]
async fn event_trigger_defaults_can_be_seeded_and_listed() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let seeded = engine
        .ensure_default_event_triggers()
        .await
        .expect("default event triggers should seed");
    assert!(seeded >= 4);

    let payload = engine
        .list_event_triggers_json()
        .await
        .expect("list_event_triggers should succeed");
    let rows = payload.as_array().expect("payload should be an array");
    assert!(rows.iter().any(|row| row["event_kind"] == "weles_health"));
    assert!(rows
        .iter()
        .any(|row| row["event_kind"] == "subagent_health"));
    assert!(rows.iter().any(|row| row["event_kind"] == "file_changed"));
    assert!(rows.iter().any(|row| row["event_kind"] == "disk_pressure"));
    assert!(rows
        .iter()
        .any(|row| { row["event_kind"] == "file_changed" && row["source"] == "packaged_default" }));
    assert!(rows.iter().any(|row| {
        row["event_kind"] == "disk_pressure" && row["source"] == "packaged_default"
    }));
}

#[tokio::test]
async fn webhook_ingest_foundation_fires_seeded_default_file_changed_trigger() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .ensure_default_event_triggers()
        .await
        .expect("default event triggers should seed");

    let payload = engine
        .ingest_webhook_event_json(&serde_json::json!({
            "event_family": "filesystem",
            "event_kind": "file_changed",
            "state": "detected",
            "thread_id": "thread-webhook-fs-1",
            "payload": {
                "path": "src/lib.rs"
            }
        }))
        .await
        .expect("webhook ingest should succeed");
    assert_eq!(payload["status"], "accepted");
    assert_eq!(payload["fired"], 1);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    let task = tasks.front().expect("expected event task");
    assert_eq!(task.status, TaskStatus::Queued);
    assert_eq!(task.thread_id.as_deref(), Some("thread-webhook-fs-1"));
    assert!(task.description.contains("src/lib.rs"));
    drop(tasks);
}

#[tokio::test]
async fn webhook_ingest_foundation_fires_seeded_default_disk_pressure_trigger() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .ensure_default_event_triggers()
        .await
        .expect("default event triggers should seed");

    let payload = engine
        .ingest_webhook_event_json(&serde_json::json!({
            "event_family": "system",
            "event_kind": "disk_pressure",
            "state": "critical",
            "thread_id": "thread-webhook-disk-1",
            "payload": {
                "mount": "/",
                "usage_pct": 94
            }
        }))
        .await
        .expect("webhook ingest should succeed");
    assert_eq!(payload["status"], "accepted");
    assert_eq!(payload["fired"], 1);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    let task = tasks.front().expect("expected approval-gated event task");
    assert_eq!(task.status, TaskStatus::AwaitingApproval);
    assert_eq!(task.thread_id.as_deref(), Some("thread-webhook-disk-1"));
    assert!(task.awaiting_approval_id.is_some());
    drop(tasks);
}

#[tokio::test]
async fn webhook_ingest_executes_direct_workflow_pack_trigger_without_background_agent_task() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .add_event_trigger_from_args(&serde_json::json!({
            "id": "trigger-workflow-pack-direct",
            "event_family": "filesystem",
            "event_kind": "file_changed",
            "target_state": "detected",
            "notification_kind": "file_changed",
            "title_template": "File changed: {path}",
            "body_template": "Observed file change for {path}",
            "tool_name": "run_workflow_pack",
            "tool_payload": {
                "pack_name": "approval-checkpoint-long-task",
                "task_kind": "task",
                "title": "Triggered governed follow-up",
                "priority": "high",
                "checkpoint_titles": ["review change"],
                "rollback_notes": ["revert triggered follow-up if noisy"]
            }
        }))
        .await
        .expect("trigger creation should succeed");

    let payload = engine
        .ingest_webhook_event_json(&serde_json::json!({
            "event_family": "filesystem",
            "event_kind": "file_changed",
            "state": "detected",
            "thread_id": "thread-trigger-pack-1",
            "payload": {
                "path": "src/lib.rs"
            }
        }))
        .await
        .expect("webhook ingest should execute the workflow-pack trigger");
    assert_eq!(payload["status"], "accepted");
    assert_eq!(payload["fired"], 1);

    let tasks = engine.tasks.lock().await;
    assert_eq!(
        tasks.len(),
        1,
        "direct workflow-pack trigger should only create the workflow-pack-owned task"
    );
    let task = tasks.front().expect("expected workflow-pack-created task");
    assert_eq!(task.source, "workflow_pack");
    assert_eq!(task.status, TaskStatus::AwaitingApproval);
    assert_eq!(task.title, "Triggered governed follow-up");
    assert_eq!(task.thread_id.as_deref(), Some("thread-trigger-pack-1"));
    assert_eq!(task.priority, TaskPriority::High);
    assert!(task.awaiting_approval_id.is_some());
    drop(tasks);

    assert_eq!(engine.pending_operator_approvals.read().await.len(), 1);
    let pending_commands = engine.pending_approval_commands.read().await;
    assert_eq!(pending_commands.len(), 1);
    let command = pending_commands
        .values()
        .next()
        .cloned()
        .expect("pending approval command should be recorded");
    assert!(command.contains("run_workflow_pack approval-checkpoint-long-task"));
}

#[tokio::test]
async fn webhook_ingest_executes_watch_monitor_trigger_and_surfaces_direct_tool_approval() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .add_event_trigger_from_args(&serde_json::json!({
            "id": "trigger-watch-monitor-direct",
            "event_family": "filesystem",
            "event_kind": "file_changed",
            "target_state": "detected",
            "notification_kind": "file_changed",
            "title_template": "File changed: {path}",
            "body_template": "Observed file change for {path}",
            "tool_name": "run_workflow_pack",
            "tool_payload": {
                "pack_name": "watch-monitor",
                "watch_source": "event",
                "source_ref": "filesystem:file_changed",
                "previous_snapshot": "status=clean",
                "remediate": true
            }
        }))
        .await
        .expect("watch-monitor trigger creation should succeed");

    let payload = engine
        .ingest_webhook_event_json(&serde_json::json!({
            "event_family": "filesystem",
            "event_kind": "file_changed",
            "state": "detected",
            "thread_id": "thread-watch-trigger-1",
            "payload": {
                "path": "src/lib.rs",
                "status": "changed"
            }
        }))
        .await
        .expect("webhook ingest should execute direct watch-monitor trigger");
    assert_eq!(payload["status"], "accepted");
    assert_eq!(payload["fired"], 1);

    let tasks = engine.tasks.lock().await;
    assert!(
        tasks.is_empty(),
        "direct watch-monitor trigger should not enqueue a background agent task"
    );
    drop(tasks);

    assert_eq!(engine.pending_operator_approvals.read().await.len(), 1);
    let pending_commands = engine.pending_approval_commands.read().await;
    assert_eq!(pending_commands.len(), 1);
    let command = pending_commands
        .values()
        .next()
        .cloned()
        .expect("watch-monitor approval command should be recorded");
    assert!(command.contains("run_workflow_pack watch-monitor"));
}

async fn post_webhook_event(
    addr: &str,
    body: &str,
    timestamp_ms: Option<u64>,
    signature: Option<&str>,
) -> String {
    let mut stream = TcpStream::connect(addr)
        .await
        .expect("connect webhook listener");
    let mut request = format!(
        "POST /webhook/event HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(timestamp_ms) = timestamp_ms {
        request.push_str(&format!("x-zorai-timestamp-ms: {timestamp_ms}\r\n"));
    }
    if let Some(signature) = signature {
        request.push_str(&format!("x-zorai-signature-256: {signature}\r\n"));
    }
    request.push_str("Connection: close\r\n\r\n");
    request.push_str(body);
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write webhook request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .await
        .expect("read webhook response");
    response
}

#[tokio::test]
async fn webhook_listener_accepts_valid_signed_requests_and_fires_trigger() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.extra.insert(
        "webhook_listener_enabled".to_string(),
        serde_json::Value::Bool(true),
    );
    config.extra.insert(
        "webhook_listener_bind".to_string(),
        serde_json::Value::String("127.0.0.1:0".to_string()),
    );
    config.extra.insert(
        "webhook_listener_secret".to_string(),
        serde_json::Value::String("test-secret".to_string()),
    );
    config.extra.insert(
        "webhook_listener_max_age_secs".to_string(),
        serde_json::Value::Number(serde_json::Number::from(300u64)),
    );
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runner = tokio::spawn(engine.clone().run_loop(shutdown_rx));

    let addr = timeout(Duration::from_secs(1), async {
        loop {
            if let Some(addr) = engine.webhook_listener_addr().await {
                break addr;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("webhook listener should become ready");

    let body = serde_json::json!({
        "event_family": "filesystem",
        "event_kind": "file_changed",
        "state": "detected",
        "thread_id": "thread-http-webhook-1",
        "payload": { "path": "src/lib.rs" }
    })
    .to_string();
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_millis() as u64;
    let signature = crate::agent::webhook_listener::compute_webhook_signature(
        "test-secret",
        timestamp_ms,
        body.as_bytes(),
    );

    let response = post_webhook_event(&addr, &body, Some(timestamp_ms), Some(&signature)).await;
    assert!(response.starts_with("HTTP/1.1 202 Accepted"));

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    let task = tasks.front().expect("expected event task");
    assert_eq!(task.status, TaskStatus::Queued);
    assert_eq!(task.thread_id.as_deref(), Some("thread-http-webhook-1"));
    assert!(task.description.contains("src/lib.rs"));
    drop(tasks);

    shutdown_tx.send(true).expect("shutdown signal should send");
    runner.await.expect("run_loop should exit");
}

#[tokio::test]
async fn webhook_listener_rejects_stale_timestamp() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.extra.insert(
        "webhook_listener_enabled".to_string(),
        serde_json::Value::Bool(true),
    );
    config.extra.insert(
        "webhook_listener_bind".to_string(),
        serde_json::Value::String("127.0.0.1:0".to_string()),
    );
    config.extra.insert(
        "webhook_listener_secret".to_string(),
        serde_json::Value::String("test-secret".to_string()),
    );
    config.extra.insert(
        "webhook_listener_max_age_secs".to_string(),
        serde_json::Value::Number(serde_json::Number::from(60u64)),
    );
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runner = tokio::spawn(engine.clone().run_loop(shutdown_rx));

    let addr = timeout(Duration::from_secs(1), async {
        loop {
            if let Some(addr) = engine.webhook_listener_addr().await {
                break addr;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("webhook listener should become ready");

    let body = serde_json::json!({
        "event_family": "filesystem",
        "event_kind": "file_changed",
        "state": "detected",
        "thread_id": "thread-http-webhook-stale-1",
        "payload": { "path": "src/lib.rs" }
    })
    .to_string();
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_millis() as u64
        - 120_000;
    let signature = crate::agent::webhook_listener::compute_webhook_signature(
        "test-secret",
        timestamp_ms,
        body.as_bytes(),
    );

    let response = post_webhook_event(&addr, &body, Some(timestamp_ms), Some(&signature)).await;
    assert!(response.starts_with("HTTP/1.1 401 Unauthorized"));
    assert!(engine.tasks.lock().await.is_empty());

    shutdown_tx.send(true).expect("shutdown signal should send");
    runner.await.expect("run_loop should exit");
}

#[tokio::test]
async fn webhook_listener_rejects_invalid_signature() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.extra.insert(
        "webhook_listener_enabled".to_string(),
        serde_json::Value::Bool(true),
    );
    config.extra.insert(
        "webhook_listener_bind".to_string(),
        serde_json::Value::String("127.0.0.1:0".to_string()),
    );
    config.extra.insert(
        "webhook_listener_secret".to_string(),
        serde_json::Value::String("test-secret".to_string()),
    );
    config.extra.insert(
        "webhook_listener_max_age_secs".to_string(),
        serde_json::Value::Number(serde_json::Number::from(300u64)),
    );
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runner = tokio::spawn(engine.clone().run_loop(shutdown_rx));

    let addr = timeout(Duration::from_secs(1), async {
        loop {
            if let Some(addr) = engine.webhook_listener_addr().await {
                break addr;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("webhook listener should become ready");

    let body = serde_json::json!({
        "event_family": "filesystem",
        "event_kind": "file_changed",
        "state": "detected",
        "thread_id": "thread-http-webhook-bad-1",
        "payload": { "path": "src/lib.rs" }
    })
    .to_string();
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_millis() as u64;

    let response =
        post_webhook_event(&addr, &body, Some(timestamp_ms), Some("sha256=badbadbad")).await;
    assert!(response.starts_with("HTTP/1.1 401 Unauthorized"));
    assert!(engine.tasks.lock().await.is_empty());

    shutdown_tx.send(true).expect("shutdown signal should send");
    runner.await.expect("run_loop should exit");
}

#[tokio::test]
async fn seeded_default_file_changed_trigger_enqueues_task_without_manual_registration() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .ensure_default_event_triggers()
        .await
        .expect("default event triggers should seed");

    let fired = engine
        .maybe_fire_event_trigger(
            "filesystem",
            "file_changed",
            Some("detected"),
            Some("thread-fs-default-1"),
            serde_json::json!({
                "path": "src/lib.rs"
            }),
        )
        .await
        .expect("default file_changed trigger should fire");
    assert_eq!(fired, 1);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    let task = tasks.front().expect("expected event task");
    assert_eq!(task.source, "event_trigger");
    assert_eq!(task.status, TaskStatus::Queued);
    assert_eq!(task.thread_id.as_deref(), Some("thread-fs-default-1"));
    assert_eq!(task.sub_agent_def_id.as_deref(), Some("weles_builtin"));
    assert!(task.description.contains("src/lib.rs"));
    drop(tasks);

    let event_rows = engine
        .history
        .list_event_log(Some("filesystem"), Some("file_changed"), 4)
        .await
        .expect("event log query should succeed");
    assert_eq!(event_rows.len(), 1);
    assert_eq!(
        event_rows[0].thread_id.as_deref(),
        Some("thread-fs-default-1")
    );
    assert!(event_rows[0].payload_json.contains("src/lib.rs"));
}

#[tokio::test]
async fn seeded_default_disk_pressure_trigger_queues_approval_without_manual_registration() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .ensure_default_event_triggers()
        .await
        .expect("default event triggers should seed");

    let fired = engine
        .maybe_fire_event_trigger(
            "system",
            "disk_pressure",
            Some("critical"),
            Some("thread-disk-default-1"),
            serde_json::json!({
                "mount": "/",
                "usage_pct": 94
            }),
        )
        .await
        .expect("default disk_pressure trigger should fire");
    assert_eq!(fired, 1);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    let approval_task = tasks.front().expect("expected approval-gated event task");
    assert_eq!(approval_task.source, "event_trigger");
    assert_eq!(approval_task.status, TaskStatus::AwaitingApproval);
    assert_eq!(
        approval_task.thread_id.as_deref(),
        Some("thread-disk-default-1")
    );
    assert_eq!(
        approval_task.sub_agent_def_id.as_deref(),
        Some("weles_builtin")
    );
    let approval_id = approval_task
        .awaiting_approval_id
        .clone()
        .expect("approval id should be present");
    drop(tasks);

    assert_eq!(engine.pending_operator_approvals.read().await.len(), 1);

    let handled = engine
        .handle_task_approval_resolution(
            &approval_id,
            zorai_protocol::ApprovalDecision::ApproveOnce,
        )
        .await;
    assert!(
        handled,
        "approval resolution should resume the seeded default event task"
    );

    let tasks = engine.tasks.lock().await;
    let resumed = tasks
        .front()
        .expect("task should still exist after approval");
    assert_eq!(resumed.status, TaskStatus::Queued);
    assert!(resumed.awaiting_approval_id.is_none());
}

#[tokio::test]
async fn run_loop_seeds_default_event_triggers_on_startup() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runner = tokio::spawn(engine.clone().run_loop(shutdown_rx));

    timeout(Duration::from_secs(1), async {
        loop {
            let payload = engine
                .list_event_triggers_json()
                .await
                .expect("list_event_triggers should succeed");
            let rows = payload.as_array().expect("payload should be an array");
            if rows.iter().any(|row| row["event_kind"] == "weles_health")
                && rows
                    .iter()
                    .any(|row| row["event_kind"] == "subagent_health")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("run_loop should seed default event triggers");

    shutdown_tx.send(true).expect("shutdown signal should send");
    runner.await.expect("run_loop should exit");
}

#[tokio::test]
async fn event_trigger_fire_respects_cooldown_and_emits_notice() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let mut events = engine.event_tx.subscribe();

    let payload = engine
        .add_event_trigger_from_args(&serde_json::json!({
            "id": "trigger-health-test",
            "event_family": "health",
            "event_kind": "subagent_health",
            "target_state": "stuck",
            "notification_kind": "subagent_health_stuck",
            "title_template": "Subagent {task_id} stuck",
            "body_template": "Task {task_id} entered {state} because {reason}",
            "cooldown_secs": 3600,
            "risk_label": "medium"
        }))
        .await
        .expect("add trigger should succeed");
    assert_eq!(payload["status"], "created");

    let fired = engine
        .maybe_fire_event_trigger(
            "health",
            "subagent_health",
            Some("stuck"),
            Some("thread-health-trigger"),
            serde_json::json!({
                "task_id": "task-77",
                "reason": "timeout"
            }),
        )
        .await
        .expect("trigger firing should succeed");
    assert_eq!(fired, 1);

    let notice = timeout(Duration::from_millis(250), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::WorkflowNotice {
                    kind,
                    thread_id,
                    message,
                    details,
                }) => break (kind, thread_id, message, details),
                Ok(_) => continue,
                Err(error) => panic!("expected workflow notice, got event error: {error}"),
            }
        }
    })
    .await
    .expect("trigger notice should be emitted");

    assert_eq!(notice.0, "subagent_health_stuck");
    assert_eq!(notice.1, "thread-health-trigger");
    assert!(notice.2.contains("task-77"));
    assert!(notice
        .3
        .as_deref()
        .is_some_and(|details| details.contains("timeout")));

    let second = engine
        .maybe_fire_event_trigger(
            "health",
            "subagent_health",
            Some("stuck"),
            Some("thread-health-trigger"),
            serde_json::json!({
                "task_id": "task-77",
                "reason": "timeout"
            }),
        )
        .await
        .expect("cooldown evaluation should succeed");
    assert_eq!(second, 0, "cooldown should suppress immediate refire");
}

#[tokio::test]
async fn low_risk_event_trigger_enqueues_weles_background_task_and_logs_event() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .add_event_trigger_from_args(&serde_json::json!({
            "id": "trigger-fs-change",
            "event_family": "filesystem",
            "event_kind": "file_changed",
            "notification_kind": "file_changed",
            "title_template": "File changed: {path}",
            "body_template": "Observed file change for {path}",
            "prompt_template": "The file at {path} changed. Review whether the operator likely needs follow-up.",
            "agent_id": "weles",
            "cooldown_secs": 10,
            "risk_label": "low"
        }))
        .await
        .expect("trigger creation should succeed");

    let fired = engine
        .maybe_fire_event_trigger(
            "filesystem",
            "file_changed",
            Some("detected"),
            Some("thread-fs-1"),
            serde_json::json!({
                "path": "src/lib.rs"
            }),
        )
        .await
        .expect("trigger should fire");
    assert_eq!(fired, 1);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    let task = tasks.front().expect("expected event task");
    assert_eq!(task.source, "event_trigger");
    assert_eq!(task.status, TaskStatus::Queued);
    assert_eq!(task.thread_id.as_deref(), Some("thread-fs-1"));
    assert_eq!(task.sub_agent_def_id.as_deref(), Some("weles_builtin"));
    assert!(task.description.contains("src/lib.rs"));
    drop(tasks);

    let event_rows = engine
        .history
        .list_event_log(Some("filesystem"), Some("file_changed"), 4)
        .await
        .expect("event log query should succeed");
    assert_eq!(event_rows.len(), 1);
    assert_eq!(event_rows[0].thread_id.as_deref(), Some("thread-fs-1"));
    assert!(event_rows[0].payload_json.contains("src/lib.rs"));
}

#[tokio::test]
async fn high_risk_event_trigger_queues_approval_before_weles_dispatch() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .add_event_trigger_from_args(&serde_json::json!({
            "id": "trigger-disk-pressure",
            "event_family": "system",
            "event_kind": "disk_pressure",
            "notification_kind": "disk_pressure",
            "title_template": "Disk pressure on {mount}",
            "body_template": "Disk usage on {mount} is {usage_pct}",
            "prompt_template": "Disk pressure detected on {mount} at {usage_pct}. Investigate and suggest cleanup actions.",
            "agent_id": "weles",
            "cooldown_secs": 10,
            "risk_label": "high"
        }))
        .await
        .expect("trigger creation should succeed");

    let fired = engine
        .maybe_fire_event_trigger(
            "system",
            "disk_pressure",
            Some("critical"),
            Some("thread-disk-1"),
            serde_json::json!({
                "mount": "/",
                "usage_pct": 94
            }),
        )
        .await
        .expect("trigger should fire");
    assert_eq!(fired, 1);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    let approval_task = tasks.front().expect("expected approval-gated event task");
    assert_eq!(approval_task.source, "event_trigger");
    assert_eq!(approval_task.status, TaskStatus::AwaitingApproval);
    assert_eq!(
        approval_task.sub_agent_def_id.as_deref(),
        Some("weles_builtin")
    );
    let approval_id = approval_task
        .awaiting_approval_id
        .clone()
        .expect("approval id should be present");
    drop(tasks);

    assert_eq!(engine.pending_operator_approvals.read().await.len(), 1);

    let handled = engine
        .handle_task_approval_resolution(
            &approval_id,
            zorai_protocol::ApprovalDecision::ApproveOnce,
        )
        .await;
    assert!(handled, "approval resolution should resume the event task");

    let tasks = engine.tasks.lock().await;
    let resumed = tasks
        .front()
        .expect("task should still exist after approval");
    assert_eq!(resumed.status, TaskStatus::Queued);
    assert!(resumed.awaiting_approval_id.is_none());
}

#[tokio::test]
async fn run_anticipatory_tick_persists_temporal_patterns_and_intent_model() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.predictive_hydration = false;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-temporal-persist";

    engine
        .record_operator_attention("conversation:chat", Some(thread_id), None)
        .await
        .expect("attention should record");
    engine.thread_work_contexts.write().await.insert(
        thread_id.to_string(),
        ThreadWorkContext {
            thread_id: thread_id.to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(root.path().display().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    engine.run_anticipatory_tick().await;

    let patterns = engine
        .history
        .list_temporal_patterns("task_sequence", 8)
        .await
        .expect("temporal patterns should persist");
    assert!(
        !patterns.is_empty(),
        "intent prediction should persist temporal task-sequence patterns"
    );
    let intent_model = engine
        .history
        .get_intent_model(crate::agent::agent_identity::WELES_AGENT_ID)
        .await
        .expect("intent model lookup should succeed");
    assert!(
        intent_model.is_some(),
        "intent model snapshot should persist"
    );
}

#[tokio::test]
async fn anticipatory_prompt_context_marks_cached_precomputation_used() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-temporal-context";
    let now = now_millis();

    let pattern_id = engine
        .history
        .insert_temporal_pattern(&crate::history::TemporalPatternRow {
            id: None,
            pattern_type: "task_sequence".to_string(),
            timescale: "minutes".to_string(),
            pattern_description: "Likely next step is repo verification".to_string(),
            context_filter: Some(format!("thread={thread_id}")),
            frequency: 1,
            last_observed_ms: now,
            first_observed_ms: now,
            confidence: 0.8,
            decay_rate: 0.01,
            created_at_ms: now,
        })
        .await
        .expect("pattern insert should succeed");
    let prediction_id = engine
        .history
        .insert_temporal_prediction(&crate::history::TemporalPredictionRow {
            id: None,
            pattern_id,
            predicted_action: "inspect or test recent repo changes".to_string(),
            predicted_at_ms: now,
            confidence: 0.8,
            actual_action: None,
            was_accepted: None,
            accuracy_score: None,
        })
        .await
        .expect("prediction insert should succeed");
    let precomputation_id = engine
        .history
        .insert_precomputation_log(&crate::history::PrecomputationLogRow {
            id: None,
            prediction_id,
            precomputation_type: "context_prefetch".to_string(),
            precomputation_details: "branch main; dirty=true".to_string(),
            started_at_ms: now,
            completed_at_ms: Some(now),
            was_used: None,
        })
        .await
        .expect("precomputation insert should succeed");

    engine.anticipatory.write().await.items = vec![AnticipatoryItem {
        id: "intent_prediction_thread-temporal-context".to_string(),
        kind: "intent_prediction".to_string(),
        title: "Likely Next Action".to_string(),
        summary: "Predicted next step: inspect or test recent repo changes".to_string(),
        bullets: vec!["Recent repo changes usually lead to verification.".to_string()],
        intent_prediction: Some(IntentPredictionPayload {
            primary_action: "inspect or test recent repo changes".to_string(),
            confidence: 0.8,
            ranked_actions: vec![IntentPredictionCandidate {
                rank: 1,
                action: "inspect or test recent repo changes".to_string(),
                confidence: 0.8,
                rationale: "repo context is active".to_string(),
            }],
        }),
        confidence: 0.8,
        goal_run_id: None,
        thread_id: Some(thread_id.to_string()),
        preferred_client_surface: None,
        preferred_attention_surface: None,
        created_at: now,
        updated_at: now,
    }];
    engine
        .anticipatory
        .write()
        .await
        .prewarm_cache_by_thread
        .insert(
            thread_id.to_string(),
            AnticipatoryPrewarmSnapshot {
                summary: "branch main; dirty=true".to_string(),
                precomputation_id: Some(precomputation_id),
            },
        );

    let context = engine
        .build_anticipatory_prompt_context(thread_id)
        .await
        .expect("prompt context should exist");
    assert!(context.contains("Temporal Foresight"));
    assert!(context.contains("Cached precomputation"));

    let precomputations = engine
        .history
        .list_precomputation_log(prediction_id)
        .await
        .expect("precomputation log should load");
    assert_eq!(precomputations[0].was_used, Some(true));
}

#[tokio::test]
async fn anticipatory_prompt_context_records_proactive_cache_provenance() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.enabled = true;
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    enable_repo_monitor_for_workspace_root(&engine, root.path()).await;

    let repo_root = root.path().join("repo-proactive-provenance");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::fs::write(repo_root.join("tracked.txt"), "hello\n").unwrap();
    std::fs::write(repo_root.join("dirty.txt"), "dirty\n").unwrap();
    std::process::Command::new("git")
        .arg("init")
        .arg(&repo_root)
        .output()
        .expect("git init should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["config", "user.email", "spec@example.com"])
        .output()
        .expect("git config email should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["config", "user.name", "Spec Test"])
        .output()
        .expect("git config name should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["add", "tracked.txt"])
        .output()
        .expect("git add should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["commit", "-m", "init"])
        .output()
        .expect("git commit should succeed");

    engine.thread_work_contexts.write().await.insert(
        "thread-proactive-provenance".to_string(),
        ThreadWorkContext {
            thread_id: "thread-proactive-provenance".to_string(),
            entries: vec![WorkContextEntry {
                path: repo_root.join("dirty.txt").to_string_lossy().to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "test".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    engine
        .refresh_anticipatory_prewarm_cache("thread-proactive-provenance")
        .await;

    let context = engine
        .build_anticipatory_prompt_context("thread-proactive-provenance")
        .await
        .expect("prompt context should exist");
    assert!(context.contains("Cached precomputation"));

    let report = engine
        .history
        .provenance_report(16)
        .expect("provenance report");
    assert_eq!(
        report
            .summary_by_event
            .get(crate::agent::provenance::PROVENANCE_EVENT_PROACTIVE_CACHE_PREPARED)
            .copied(),
        Some(1)
    );
    assert_eq!(
        report
            .summary_by_event
            .get(crate::agent::provenance::PROVENANCE_EVENT_PROACTIVE_CACHE_USED)
            .copied(),
        Some(1)
    );
    assert!(report.entries.iter().any(|entry| {
        entry.event_type == crate::agent::provenance::PROVENANCE_EVENT_PROACTIVE_CACHE_PREPARED
            && entry.thread_id.as_deref() == Some("thread-proactive-provenance")
    }));
    assert!(report.entries.iter().any(|entry| {
        entry.event_type == crate::agent::provenance::PROVENANCE_EVENT_PROACTIVE_CACHE_USED
            && entry.thread_id.as_deref() == Some("thread-proactive-provenance")
    }));
}

#[tokio::test]
async fn speculative_queue_admits_high_confidence_items_and_dedupes_by_thread_and_action() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let now = now_millis();
    let item = AnticipatoryItem {
        id: "intent-spec-1".to_string(),
        kind: "intent_prediction".to_string(),
        title: "Likely Next Action".to_string(),
        summary: "Predicted next step: inspect repo context".to_string(),
        bullets: vec![],
        intent_prediction: None,
        confidence: 0.91,
        goal_run_id: None,
        thread_id: Some("thread-spec".to_string()),
        preferred_client_surface: None,
        preferred_attention_surface: None,
        created_at: now,
        updated_at: now,
    };

    engine
        .build_speculative_execution_queue(&[item.clone(), item])
        .await;

    let runtime = engine.anticipatory.read().await;
    assert_eq!(runtime.opportunity_queue.len(), 1);
    let queued = runtime
        .opportunity_queue
        .front()
        .expect("speculative opportunity should be queued");
    assert_eq!(queued.thread_id.as_deref(), Some("thread-spec"));
    assert_eq!(queued.action_kind, "repo_context_refresh");
}

#[tokio::test]
async fn speculative_queue_rejects_unsafe_or_low_confidence_items() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let now = now_millis();
    let low_confidence = AnticipatoryItem {
        id: "intent-spec-low".to_string(),
        kind: "intent_prediction".to_string(),
        title: "Low confidence".to_string(),
        summary: "maybe do something".to_string(),
        bullets: vec![],
        intent_prediction: None,
        confidence: 0.4,
        goal_run_id: None,
        thread_id: Some("thread-low".to_string()),
        preferred_client_surface: None,
        preferred_attention_surface: None,
        created_at: now,
        updated_at: now,
    };
    let unsupported = AnticipatoryItem {
        id: "collab-spec".to_string(),
        kind: "collaboration_disagreement".to_string(),
        title: "Collaboration".to_string(),
        summary: "unsafe for speculative queue".to_string(),
        bullets: vec![],
        intent_prediction: None,
        confidence: 0.99,
        goal_run_id: None,
        thread_id: Some("thread-low".to_string()),
        preferred_client_surface: None,
        preferred_attention_surface: None,
        created_at: now,
        updated_at: now,
    };

    engine
        .build_speculative_execution_queue(&[low_confidence, unsupported])
        .await;

    assert!(engine
        .anticipatory
        .read()
        .await
        .opportunity_queue
        .is_empty());
}

#[tokio::test]
async fn speculative_queue_execution_persists_result_and_retrieval_marks_usage() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let repo_root = root.path().join("repo-speculative");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::fs::write(repo_root.join("tracked.txt"), "hello\n").unwrap();
    std::fs::write(repo_root.join("dirty.txt"), "dirty\n").unwrap();
    std::process::Command::new("git")
        .arg("init")
        .arg(&repo_root)
        .output()
        .expect("git init should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["config", "user.email", "spec@example.com"])
        .output()
        .expect("git config email should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["config", "user.name", "Spec Test"])
        .output()
        .expect("git config name should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["add", "tracked.txt"])
        .output()
        .expect("git add should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["commit", "-m", "init"])
        .output()
        .expect("git commit should succeed");

    enable_repo_monitor_for_workspace_root(&engine, &repo_root).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-spec-run"), None)
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-spec-run".to_string(),
        ThreadWorkContext {
            thread_id: "thread-spec-run".to_string(),
            entries: vec![WorkContextEntry {
                path: repo_root.join("dirty.txt").to_string_lossy().to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "test".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    let item = AnticipatoryItem {
        id: "intent-spec-run".to_string(),
        kind: "intent_prediction".to_string(),
        title: "Likely Next Action".to_string(),
        summary: "Predicted next step: inspect repo context".to_string(),
        bullets: vec![],
        intent_prediction: None,
        confidence: 0.92,
        goal_run_id: None,
        thread_id: Some("thread-spec-run".to_string()),
        preferred_client_surface: None,
        preferred_attention_surface: None,
        created_at: now_millis(),
        updated_at: now_millis(),
    };

    engine.build_speculative_execution_queue(&[item]).await;
    engine.run_speculative_execution_queue().await;

    let result = engine
        .take_matching_speculative_result("thread-spec-run", "repo_context_refresh")
        .await
        .expect("speculative result should be retrievable");
    assert!(result.summary.contains("branch"));
    assert!(result.precomputation_id.is_some());

    let second = engine
        .take_matching_speculative_result("thread-spec-run", "repo_context_refresh")
        .await;
    assert!(
        second.is_none(),
        "used speculative results should not be reused"
    );

    let pattern_id = engine
        .history
        .list_temporal_patterns("file_access", 8)
        .await
        .expect("patterns should load")
        .into_iter()
        .filter_map(|row| row.id)
        .max()
        .expect("pattern id should exist");
    let prediction_id = engine
        .history
        .list_temporal_predictions(pattern_id, 8)
        .await
        .expect("predictions should load")
        .into_iter()
        .filter_map(|row| row.id)
        .max()
        .expect("prediction id should exist");
    let logs = engine
        .history
        .list_precomputation_log(prediction_id)
        .await
        .expect("precomputation log should load");
    assert!(logs.iter().any(|row| row.was_used == Some(true)));
}

#[tokio::test]
async fn speculative_queue_records_speculative_provenance_for_prepare_and_use() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.enabled = true;
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let repo_root = root.path().join("repo-speculative-provenance");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::fs::write(repo_root.join("tracked.txt"), "hello\n").unwrap();
    std::fs::write(repo_root.join("dirty.txt"), "dirty\n").unwrap();
    std::process::Command::new("git")
        .arg("init")
        .arg(&repo_root)
        .output()
        .expect("git init should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["config", "user.email", "spec@example.com"])
        .output()
        .expect("git config email should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["config", "user.name", "Spec Test"])
        .output()
        .expect("git config name should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["add", "tracked.txt"])
        .output()
        .expect("git add should succeed");
    std::process::Command::new("git")
        .current_dir(&repo_root)
        .args(["commit", "-m", "init"])
        .output()
        .expect("git commit should succeed");

    enable_repo_monitor_for_workspace_root(&engine, &repo_root).await;

    engine
        .record_operator_attention(
            "conversation:chat",
            Some("thread-speculative-provenance"),
            None,
        )
        .await
        .unwrap();
    engine.thread_work_contexts.write().await.insert(
        "thread-speculative-provenance".to_string(),
        ThreadWorkContext {
            thread_id: "thread-speculative-provenance".to_string(),
            entries: vec![WorkContextEntry {
                path: repo_root.join("dirty.txt").to_string_lossy().to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "test".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );

    let item = AnticipatoryItem {
        id: "intent-spec-provenance".to_string(),
        kind: "intent_prediction".to_string(),
        title: "Likely Next Action".to_string(),
        summary: "Predicted next step: inspect repo context".to_string(),
        bullets: vec![],
        intent_prediction: None,
        confidence: 0.93,
        goal_run_id: None,
        thread_id: Some("thread-speculative-provenance".to_string()),
        preferred_client_surface: None,
        preferred_attention_surface: None,
        created_at: now_millis(),
        updated_at: now_millis(),
    };

    engine.build_speculative_execution_queue(&[item]).await;
    engine.run_speculative_execution_queue().await;
    let result = engine
        .take_matching_speculative_result(
            "thread-speculative-provenance",
            SPECULATIVE_ACTION_REPO_CONTEXT_REFRESH,
        )
        .await
        .expect("speculative result should be retrievable");
    assert!(result.summary.contains("branch"));

    let report = engine
        .history
        .provenance_report(20)
        .expect("provenance report");
    assert_eq!(
        report
            .summary_by_event
            .get(crate::agent::provenance::PROVENANCE_EVENT_SPECULATIVE_RESULT_PREPARED)
            .copied(),
        Some(1)
    );
    assert_eq!(
        report
            .summary_by_event
            .get(crate::agent::provenance::PROVENANCE_EVENT_SPECULATIVE_RESULT_USED)
            .copied(),
        Some(1)
    );
    assert!(report.entries.iter().any(|entry| {
        entry.event_type == crate::agent::provenance::PROVENANCE_EVENT_SPECULATIVE_RESULT_PREPARED
            && entry.thread_id.as_deref() == Some("thread-speculative-provenance")
    }));
    assert!(report.entries.iter().any(|entry| {
        entry.event_type == crate::agent::provenance::PROVENANCE_EVENT_SPECULATIVE_RESULT_USED
            && entry.thread_id.as_deref() == Some("thread-speculative-provenance")
    }));
}

#[tokio::test]
async fn speculative_queue_expiry_cleans_stale_entries() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let expired_at = now_millis().saturating_sub(1);
    {
        let mut runtime = engine.anticipatory.write().await;
        runtime.opportunity_queue.push_back(SpeculativeOpportunity {
            id: "expired-opportunity".to_string(),
            thread_id: Some("thread-expired".to_string()),
            source_kind: "intent_prediction".to_string(),
            action_kind: "repo_context_refresh".to_string(),
            confidence: 0.9,
            created_at_ms: expired_at,
            expires_at_ms: expired_at,
            status: SpeculativeOpportunityStatus::Queued,
            summary: "expired".to_string(),
        });
        runtime.speculative_results_by_thread.insert(
            "thread-expired".to_string(),
            vec![SpeculativeResult {
                opportunity_id: "expired-opportunity".to_string(),
                action_kind: "repo_context_refresh".to_string(),
                thread_id: Some("thread-expired".to_string()),
                summary: "expired".to_string(),
                artifact: serde_json::json!({"summary": "expired"}),
                completed_at_ms: expired_at,
                expires_at_ms: expired_at,
                used_at_ms: None,
                precomputation_id: None,
            }],
        );
    }

    engine.build_speculative_execution_queue(&[]).await;
    let runtime = engine.anticipatory.read().await;
    assert!(runtime.opportunity_queue.is_empty());
    assert!(runtime.speculative_results_by_thread.is_empty());
}

#[tokio::test]
async fn speculative_prompt_context_surfaces_cached_speculative_result() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .anticipatory
        .write()
        .await
        .speculative_results_by_thread
        .insert(
            "thread-spec-context".to_string(),
            vec![SpeculativeResult {
                opportunity_id: "spec-ctx-1".to_string(),
                action_kind: "repo_context_refresh".to_string(),
                thread_id: Some("thread-spec-context".to_string()),
                summary: "branch main; dirty=true; speculative".to_string(),
                artifact: serde_json::json!({"summary": "branch main; dirty=true; speculative"}),
                completed_at_ms: now_millis(),
                expires_at_ms: now_millis().saturating_add(60_000),
                used_at_ms: None,
                precomputation_id: None,
            }],
        );

    let context = engine
        .build_anticipatory_prompt_context("thread-spec-context")
        .await
        .expect("prompt context should exist");
    assert!(context.contains("Speculative result"));
}

#[tokio::test]
async fn speculative_queue_drops_unknown_action_kind_during_execution() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    {
        let mut runtime = engine.anticipatory.write().await;
        runtime.opportunity_queue.push_back(SpeculativeOpportunity {
            id: "bad-action".to_string(),
            thread_id: Some("thread-bad".to_string()),
            source_kind: "intent_prediction".to_string(),
            action_kind: "unsafe_write".to_string(),
            confidence: 0.95,
            created_at_ms: now_millis(),
            expires_at_ms: now_millis().saturating_add(60_000),
            status: SpeculativeOpportunityStatus::Queued,
            summary: "unsafe".to_string(),
        });
    }

    engine.run_speculative_execution_queue().await;
    let runtime = engine.anticipatory.read().await;
    assert!(runtime.opportunity_queue.is_empty());
    assert!(runtime.speculative_results_by_thread.is_empty());
}

#[tokio::test]
async fn cognitive_resonance_sampling_persists_samples_and_adjustment_logs() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .history
        .insert_cognitive_resonance_sample(&crate::history::CognitiveResonanceSampleRow {
            id: None,
            sampled_at_ms: now_millis().saturating_sub(60_000),
            revision_velocity_ms: Some(120_000),
            session_entropy: Some(0.1),
            approval_latency_ms: Some(1_000),
            tool_hesitation_count: 0,
            cognitive_state: "flow".to_string(),
            state_confidence: 0.8,
            resonance_score: 0.8,
            verbosity_adjustment: 0.9,
            risk_adjustment: 0.85,
            proactiveness_adjustment: 0.8,
            memory_urgency_adjustment: 0.3,
        })
        .await
        .expect("seed resonance sample should persist");

    {
        let mut model = engine.operator_model.write().await;
        model.operator_satisfaction.label = "strained".to_string();
        model.operator_satisfaction.score = 0.18;
        model.implicit_feedback.tool_hesitation_count = 3;
        model.attention_topology.rapid_switch_count = 4;
        model.attention_topology.focus_event_count = 4;
    }

    engine.sample_cognitive_resonance_runtime().await;

    let samples = engine
        .history
        .list_cognitive_resonance_samples(2)
        .await
        .expect("resonance samples should load");
    assert_eq!(samples[0].cognitive_state, "frustrated");
    let adjustments = engine
        .history
        .list_behavior_adjustment_log(8)
        .await
        .expect("behavior adjustments should load");
    assert!(
        !adjustments.is_empty(),
        "substantial resonance shifts should log behavior adjustments"
    );
}

#[tokio::test]
async fn cognitive_resonance_sampling_finds_revision_signal_beyond_recent_unrelated_window() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-revision-velocity-sql";
    let now = now_millis();

    engine
        .history
        .insert_implicit_signal(&crate::history::ImplicitSignalRow {
            id: "older-operator-correction".to_string(),
            session_id: thread_id.to_string(),
            signal_type: "operator_correction".to_string(),
            weight: -0.2,
            timestamp_ms: now.saturating_sub(60_000),
            context_snapshot_json: None,
        })
        .await
        .expect("insert correction signal");
    for index in 0..12u64 {
        engine
            .history
            .insert_implicit_signal(&crate::history::ImplicitSignalRow {
                id: format!("newer-tool-signal-{index}"),
                session_id: thread_id.to_string(),
                signal_type: "tool_fallback".to_string(),
                weight: -0.01,
                timestamp_ms: now.saturating_sub(10_000).saturating_add(index),
                context_snapshot_json: None,
            })
            .await
            .expect("insert unrelated signal");
    }
    engine.anticipatory.write().await.active_attention_thread_id = Some(thread_id.to_string());

    engine.sample_cognitive_resonance_runtime().await;

    let samples = engine
        .history
        .list_cognitive_resonance_samples(1)
        .await
        .expect("resonance samples should load");
    assert!(
        samples[0].revision_velocity_ms.unwrap_or_default() >= 60_000,
        "revision velocity should be based on the latest matching signal type, not the latest mixed signal window"
    );
}

#[tokio::test]
async fn low_risk_event_trigger_records_succeeded_fire_history_with_created_task_id() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .add_event_trigger_from_args(&serde_json::json!({
            "id": "trigger-fs-history-success",
            "event_family": "filesystem",
            "event_kind": "file_changed",
            "notification_kind": "file_changed",
            "title_template": "File changed: {path}",
            "body_template": "Observed file change for {path}",
            "prompt_template": "The file at {path} changed. Review whether the operator likely needs follow-up.",
            "agent_id": "weles",
            "cooldown_secs": 10,
            "risk_label": "low"
        }))
        .await
        .expect("trigger creation should succeed");

    let fired = engine
        .maybe_fire_event_trigger(
            "filesystem",
            "file_changed",
            Some("detected"),
            Some("thread-fs-history-success"),
            serde_json::json!({
                "path": "src/lib.rs"
            }),
        )
        .await
        .expect("trigger should fire");
    assert_eq!(fired, 1);

    let task_id = {
        let tasks = engine.tasks.lock().await;
        tasks.front().expect("expected event task").id.clone()
    };

    let history_rows = engine
        .history
        .list_trigger_fire_history(Some("trigger-fs-history-success"), Some("succeeded"), 4)
        .await
        .expect("trigger fire history query should succeed");
    assert_eq!(history_rows.len(), 1);
    let row = &history_rows[0];
    assert_eq!(row.status, "succeeded");
    assert_eq!(row.created_task_id.as_deref(), Some(task_id.as_str()));
    assert!(row.completed_at_ms.is_some());
    assert!(row.error_message.is_none());
}

#[tokio::test]
async fn unsupported_direct_tool_trigger_records_failed_fire_history_and_emits_failure_notice() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let mut events = engine.event_tx.subscribe();

    engine
        .add_event_trigger_from_args(&serde_json::json!({
            "id": "trigger-unsupported-tool-history-failure",
            "event_family": "health",
            "event_kind": "subagent_health",
            "target_state": "stuck",
            "notification_kind": "subagent_health_stuck",
            "title_template": "Subagent {task_id} stuck",
            "body_template": "Task {task_id} entered {state} because {reason}",
            "tool_name": "read_file",
            "tool_payload": { "path": "/tmp/never-used" },
            "cooldown_secs": 10,
            "risk_label": "medium"
        }))
        .await
        .expect("trigger creation should succeed");

    let error = engine
        .maybe_fire_event_trigger(
            "health",
            "subagent_health",
            Some("stuck"),
            Some("thread-trigger-history-failure"),
            serde_json::json!({
                "task_id": "task-77",
                "reason": "timeout"
            }),
        )
        .await
        .expect_err("unsupported direct tool trigger should fail");
    assert!(error.to_string().contains("not supported"));

    let failure_notice = timeout(Duration::from_millis(250), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::WorkflowNotice {
                    kind,
                    message,
                    details,
                    ..
                }) if kind == "event_trigger_failed" => break (message, details),
                Ok(_) => continue,
                Err(error) => panic!("expected failure workflow notice, got event error: {error}"),
            }
        }
    })
    .await
    .expect("failure workflow notice should be emitted");
    assert!(failure_notice
        .0
        .contains("trigger-unsupported-tool-history-failure"));
    assert!(failure_notice
        .1
        .as_deref()
        .is_some_and(|details| details.contains("not supported")));

    let history_rows = engine
        .history
        .list_trigger_fire_history(Some("trigger-unsupported-tool-history-failure"), None, 4)
        .await
        .expect("trigger fire history query should succeed");
    assert_eq!(history_rows.len(), 1);
    let row = &history_rows[0];
    assert_eq!(row.status, "failed");
    assert!(row.completed_at_ms.is_some());
    assert!(row.created_task_id.is_none());
    assert!(row
        .error_message
        .as_deref()
        .is_some_and(|message| message.contains("not supported")));
}
