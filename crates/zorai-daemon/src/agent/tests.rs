use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn sample_goal_run() -> GoalRun {
    GoalRun {
        id: "goal_test".to_string(),
        title: "Test goal".to_string(),
        goal: "Ship something".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Failed,
        priority: TaskPriority::Normal,
        created_at: 10,
        updated_at: 30,
        started_at: Some(20),
        completed_at: Some(80),
        thread_id: None,
        session_id: Some("session-1".to_string()),
        current_step_index: 1,
        current_step_title: None,
        current_step_kind: None,
        planner_owner_profile: None,
        current_step_owner_profile: None,
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 1,
        max_replans: 2,
        plan_summary: Some("Plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: Some("/tmp/skill.md".to_string()),
        last_error: Some("child task failed".to_string()),
        failure_cause: None,
        step_failure_history: Vec::new(),
        stopped_reason: None,
        child_task_ids: vec!["task-a".to_string(), "task-b".to_string()],
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
        steps: vec![
            GoalRunStep {
                id: "step-0".to_string(),
                position: 0,
                title: "Inspect".to_string(),
                instructions: "Inspect state".to_string(),
                kind: GoalRunStepKind::Research,
                success_criteria: "Know what failed".to_string(),
                session_id: None,
                status: GoalRunStepStatus::Completed,
                task_id: Some("task-a".to_string()),
                summary: Some("done".to_string()),
                error: None,
                started_at: Some(21),
                completed_at: Some(30),
            },
            GoalRunStep {
                id: "step-1".to_string(),
                position: 1,
                title: "Fix".to_string(),
                instructions: "Fix it".to_string(),
                kind: GoalRunStepKind::Command,
                success_criteria: "Green".to_string(),
                session_id: Some("session-1".to_string()),
                status: GoalRunStepStatus::Failed,
                task_id: Some("task-b".to_string()),
                summary: None,
                error: Some("step failure".to_string()),
                started_at: Some(31),
                completed_at: Some(50),
            },
        ],
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

fn sample_task(id: &str, goal_run_id: &str) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: id.to_string(),
        description: id.to_string(),
        status: TaskStatus::Failed,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: 1,
        started_at: Some(2),
        completed_at: Some(3),
        error: Some("task error".to_string()),
        result: None,
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: Some("session-1".to_string()),
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("Test goal".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("Fix".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: Some("apr-1".to_string()),
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: Some("task error".to_string()),
        logs: vec![AgentTaskLogEntry {
            id: format!("log-{id}"),
            timestamp: 4,
            level: TaskLogLevel::Warn,
            phase: "approval".to_string(),
            message: "managed command paused for operator approval".to_string(),
            details: None,
            attempt: 0,
        }],
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

fn sample_subagent(id: &str, parent_task_id: &str, status: TaskStatus) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: format!("Subagent {id}"),
        description: "Child work".to_string(),
        status,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: 1,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: None,
        source: "subagent".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: Some(parent_task_id.to_string()),
        parent_thread_id: Some("thread-parent".to_string()),
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
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

fn sample_session(session_id: &str, workspace_id: &str) -> zorai_protocol::SessionInfo {
    zorai_protocol::SessionInfo {
        id: uuid::Uuid::parse_str(session_id).expect("valid uuid"),
        title: Some("Agent lane".to_string()),
        cwd: Some("/tmp/repo".to_string()),
        cols: 120,
        rows: 40,
        created_at: 1,
        workspace_id: Some(workspace_id.to_string()),
        exit_code: None,
        is_alive: true,
        active_command: Some("cargo test".to_string()),
    }
}

pub(crate) async fn spawn_goal_recording_server(
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
    assistant_content: String,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind goal recording server");
    let addr = listener.local_addr().expect("goal recording server addr");
    let response_json =
        serde_json::to_string(&assistant_content).expect("assistant content should serialize");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            let response_json = response_json.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let read = socket
                    .read(&mut buffer)
                    .await
                    .expect("read goal recording request");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let body = request
                    .split("\r\n\r\n")
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                recorded_bodies
                    .lock()
                    .expect("lock goal request log")
                    .push_back(body);

                let response = format!(
                    concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\n",
                        "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\n",
                        "data: [DONE]\n\n"
                    ),
                    response_json
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write goal recording response");
            });
        }
    });

    format!("http://{addr}/v1")
}

#[test]
fn collect_plan_issues_catches_empty_summary() {
    let plan = GoalPlanResponse {
        title: Some("Goal".to_string()),
        summary: String::new(),
        steps: vec![GoalPlanStepResponse {
            title: "Do it".to_string(),
            instructions: "Do it".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "Done".to_string(),
            execution_binding: None,
            verification_binding: None,
            proof_checks: Vec::new(),
            session_id: None,
            llm_confidence: None,
            llm_confidence_rationale: None,
        }],
        rejected_alternatives: Vec::new(),
    };

    assert!(!collect_plan_issues(&plan).is_empty());
}

#[cfg(feature = "lancedb-vector")]
#[path = "tests/skill_mesh_compiler.rs"]
mod skill_mesh_compiler_tests;
#[cfg(feature = "lancedb-vector")]
#[path = "tests/skill_mesh.rs"]
mod skill_mesh_tests;

#[test]
fn project_goal_run_snapshot_derives_metrics() {
    let goal_run = sample_goal_run();
    let tasks = vec![sample_task("task-b", "goal_test")];

    let projected = project_goal_run_snapshot(goal_run, &tasks, 100);

    assert_eq!(projected.current_step_title.as_deref(), Some("Fix"));
    assert_eq!(projected.child_task_count, 2);
    assert_eq!(projected.approval_count, 1);
    assert_eq!(projected.awaiting_approval_id.as_deref(), Some("apr-1"));
    assert_eq!(
        projected.failure_cause.as_deref(),
        Some("child task failed")
    );
    assert_eq!(projected.duration_ms, Some(60));
}

#[test]
fn project_goal_run_snapshot_clears_stale_awaiting_approval_without_approval_id() {
    let mut goal_run = sample_goal_run();
    goal_run.status = GoalRunStatus::AwaitingApproval;
    goal_run.awaiting_approval_id = Some("stale-approval".to_string());
    goal_run.completed_at = None;
    goal_run.last_error = None;
    goal_run.failure_cause = None;

    let mut task = sample_task("task-b", "goal_test");
    task.status = TaskStatus::InProgress;
    task.awaiting_approval_id = None;
    task.completed_at = None;
    task.error = None;
    task.last_error = None;
    task.logs.clear();

    let projected = project_goal_run_snapshot(goal_run, &[task], 100);

    assert_eq!(projected.status, GoalRunStatus::Running);
    assert!(projected.awaiting_approval_id.is_none());
}

#[test]
fn project_task_runs_exposes_parent_runtime_workspace_and_classification() {
    let mut parent = sample_task("parent-task", "goal_test");
    parent.title = "Implement rust file patching".to_string();
    parent.description = "Update repo files and tests".to_string();
    parent.status = TaskStatus::InProgress;
    parent.source = "user".to_string();
    parent.session_id = Some("11111111-1111-1111-1111-111111111111".to_string());

    let mut child = sample_subagent("child-task", "parent-task", TaskStatus::Queued);
    child.session_id = Some("22222222-2222-2222-2222-222222222222".to_string());
    child.runtime = "hermes".to_string();

    let runs = project_task_runs_with_runtime(
        &[parent.clone(), child.clone()],
        &[
            sample_session("11111111-1111-1111-1111-111111111111", "workspace-parent"),
            sample_session("22222222-2222-2222-2222-222222222222", "workspace-child"),
        ],
        &std::collections::HashMap::new(),
        &[],
    );

    let parent_run = runs
        .iter()
        .find(|run| run.id == parent.id)
        .expect("parent run projected");
    assert_eq!(parent_run.kind, AgentRunKind::Task);
    assert_eq!(parent_run.classification, "coding");
    assert_eq!(parent_run.workspace_id.as_deref(), Some("workspace-parent"));

    let child_run = runs
        .iter()
        .find(|run| run.id == child.id)
        .expect("child run projected");
    assert_eq!(child_run.kind, AgentRunKind::Subagent);
    assert_eq!(child_run.runtime, "hermes");
    assert_eq!(child_run.parent_run_id.as_deref(), Some("parent-task"));
    assert_eq!(
        child_run.parent_title.as_deref(),
        Some("Implement rust file patching")
    );
    assert_eq!(child_run.workspace_id.as_deref(), Some("workspace-child"));
}

#[test]
fn project_task_runs_prefers_thread_profile_then_task_override_then_subagent() {
    let mut profiled = sample_subagent("profiled", "parent-task", TaskStatus::Queued);
    profiled.thread_id = Some("thread-profiled".to_string());
    profiled.sub_agent_def_id = Some("mokosh".to_string());
    profiled.override_provider = Some("override-provider".to_string());
    profiled.override_model = Some("override-model".to_string());

    let mut fallback = sample_subagent("fallback", "parent-task", TaskStatus::Queued);
    fallback.sub_agent_def_id = Some("mokosh".to_string());
    fallback.override_provider = Some("task-provider".to_string());
    fallback.override_model = Some("task-model".to_string());

    let definition: SubAgentDefinition = serde_json::from_value(serde_json::json!({
        "id": "mokosh",
        "name": "Mokosh",
        "provider": "openrouter",
        "model": "definition-model",
        "reasoning_effort": "medium",
    }))
    .expect("subagent definition");
    let mut profiles = std::collections::HashMap::new();
    profiles.insert(
        "thread-profiled".to_string(),
        ThreadExecutionProfile {
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.5".to_string()),
            reasoning_effort: Some("high".to_string()),
            context_window_tokens: None,
        },
    );

    let runs =
        project_task_runs_with_runtime(&[profiled, fallback], &[], &profiles, &[definition]);
    let profiled_run = runs
        .iter()
        .find(|run| run.id == "profiled")
        .expect("profiled run");
    assert_eq!(profiled_run.provider.as_deref(), Some("github-copilot"));
    assert_eq!(profiled_run.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(profiled_run.reasoning_effort.as_deref(), Some("high"));

    let fallback_run = runs
        .iter()
        .find(|run| run.id == "fallback")
        .expect("fallback run");
    assert_eq!(fallback_run.provider.as_deref(), Some("task-provider"));
    assert_eq!(fallback_run.model.as_deref(), Some("task-model"));
    assert_eq!(fallback_run.reasoning_effort.as_deref(), Some("medium"));
}

#[test]
fn make_goal_run_event_with_todos_preserves_snapshot() {
    let event = make_goal_run_event_with_todos(
        "todo",
        "goal todo updated",
        None,
        Some(1),
        vec![TodoItem {
            id: "todo-1".to_string(),
            content: "Inspect failing test".to_string(),
            status: TodoStatus::InProgress,
            position: 0,
            step_index: Some(1),
            created_at: 10,
            updated_at: 20,
        }],
    );

    assert_eq!(event.phase, "todo");
    assert_eq!(event.step_index, Some(1));
    assert_eq!(event.todo_snapshot.len(), 1);
    assert_eq!(event.todo_snapshot[0].content, "Inspect failing test");
}

#[tokio::test]
async fn replace_thread_todos_binds_authoritative_goal_items_to_current_step() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut goal_run = sample_goal_run();
    goal_run.id = "goal-1".to_string();
    goal_run.current_step_index = 0;
    goal_run.updated_at = 100;
    goal_run.events.clear();
    engine.goal_runs.lock().await.push_back(goal_run);

    let mut task = sample_task("task-root", "goal-1");
    task.thread_id = Some("thread-main".to_string());
    task.goal_step_id = Some("step-0".to_string());
    task.goal_step_title = Some("Inspect".to_string());
    engine.tasks.lock().await.push_back(task.clone());
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    engine
        .replace_thread_todos(
            "thread-main",
            vec![
                TodoItem {
                    id: "todo-1".to_string(),
                    content: "Inspect current state".to_string(),
                    status: TodoStatus::InProgress,
                    position: 0,
                    step_index: Some(1),
                    created_at: 1,
                    updated_at: 1,
                },
                TodoItem {
                    id: "todo-2".to_string(),
                    content: "Capture failing evidence".to_string(),
                    status: TodoStatus::Pending,
                    position: 1,
                    step_index: Some(2),
                    created_at: 1,
                    updated_at: 1,
                },
            ],
            Some(task.id.as_str()),
        )
        .await;

    let goal_run = engine
        .get_goal_run("goal-1")
        .await
        .expect("goal run should still exist");
    let event = goal_run
        .events
        .last()
        .expect("authoritative goal todo update should record a goal event");
    assert_eq!(event.step_index, Some(0));
    assert_eq!(event.todo_snapshot.len(), 2);
    assert_eq!(event.todo_snapshot[0].step_index, Some(0));
    assert_eq!(event.todo_snapshot[1].step_index, Some(0));
}

#[tokio::test]
async fn replace_thread_todos_from_subagent_stays_thread_scoped_for_goal_runs() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut goal_run = sample_goal_run();
    goal_run.id = "goal-1".to_string();
    goal_run.current_step_index = 0;
    goal_run.updated_at = 100;
    goal_run.events.clear();
    engine.goal_runs.lock().await.push_back(goal_run);

    let mut parent = sample_task("task-root", "goal-1");
    parent.thread_id = Some("thread-main".to_string());
    engine.tasks.lock().await.push_back(parent.clone());

    let mut child = sample_subagent("task-child", &parent.id, TaskStatus::InProgress);
    child.goal_run_id = Some("goal-1".to_string());
    child.thread_id = Some("thread-child".to_string());
    engine.tasks.lock().await.push_back(child.clone());

    engine
        .replace_thread_todos(
            "thread-child",
            vec![TodoItem {
                id: "todo-child".to_string(),
                content: "Local child todo".to_string(),
                status: TodoStatus::Pending,
                position: 0,
                step_index: Some(0),
                created_at: 1,
                updated_at: 1,
            }],
            Some(child.id.as_str()),
        )
        .await;

    let goal_run = engine
        .get_goal_run("goal-1")
        .await
        .expect("goal run should still exist");
    assert!(
        goal_run.events.is_empty(),
        "subagent todo updates must not overwrite authoritative goal-step todos"
    );
    let child_todos = engine.thread_todos.read().await;
    assert_eq!(
        child_todos
            .get("thread-child")
            .expect("child thread todos should still be stored locally")
            .len(),
        1
    );
}

#[test]
fn planner_required_for_message_detects_multi_step_requests() {
    assert!(planner_required_for_message(
        "Investigate the failing tests, then update the parser, and finally rerun the suite."
    ));
    assert!(planner_required_for_message(
        "1. Inspect logs\n2. Find the bad config\n3. Patch it"
    ));
}

#[test]
fn planner_required_for_message_skips_simple_requests() {
    assert!(!planner_required_for_message(
        "What port is the daemon listening on?"
    ));
    assert!(!planner_required_for_message("Show me the last error."));
}

#[test]
fn refresh_task_queue_state_blocks_parent_while_subagents_are_active() {
    let mut tasks = VecDeque::from(vec![
        AgentTask {
            id: "parent".to_string(),
            title: "Parent".to_string(),
            description: "Parent".to_string(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 10,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: "agent".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
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
        },
        sample_subagent("sub-1", "parent", TaskStatus::InProgress),
    ]);

    let changed = refresh_task_queue_state(&mut tasks, 100, &[], &AgentConfig::default());
    let parent = tasks.iter().find(|task| task.id == "parent").unwrap();

    assert_eq!(parent.status, TaskStatus::Blocked);
    assert!(parent
        .blocked_reason
        .as_deref()
        .unwrap_or_default()
        .contains("waiting for subagents"));
    assert_eq!(changed.len(), 1);
}

#[test]
fn refresh_task_queue_state_requeues_parent_after_subagents_finish() {
    let mut tasks = VecDeque::from(vec![
        AgentTask {
            id: "parent".to_string(),
            title: "Parent".to_string(),
            description: "Parent".to_string(),
            status: TaskStatus::Blocked,
            priority: TaskPriority::Normal,
            progress: 90,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: "agent".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: Some("waiting for subagents: sub-1".to_string()),
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
        },
        sample_subagent("sub-1", "parent", TaskStatus::Completed),
    ]);

    let changed = refresh_task_queue_state(&mut tasks, 100, &[], &AgentConfig::default());
    let parent = tasks.iter().find(|task| task.id == "parent").unwrap();

    assert_eq!(parent.status, TaskStatus::Queued);
    assert!(parent.blocked_reason.is_none());
    assert_eq!(changed.len(), 1);
}

#[test]
fn refresh_task_queue_state_requeues_stale_awaiting_approval_without_id() {
    let mut tasks = VecDeque::from(vec![AgentTask {
        id: "stale-approval".to_string(),
        title: "Stale approval".to_string(),
        description: "stuck without a live approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 35,
        created_at: 1,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some("thread-1".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some("goal-1".to_string()),
        goal_run_title: Some("Investigate ingenix.ai".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("Review findings".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("awaiting approval".to_string()),
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
    }]);

    let changed = refresh_task_queue_state(&mut tasks, 100, &[], &AgentConfig::default());
    let task = tasks.front().expect("task should remain present");

    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.blocked_reason.is_none());
    assert_eq!(changed.len(), 1);
}
