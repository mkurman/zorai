use super::*;
use crate::agent::types::{
    GoalAgentAssignment, GoalDeliveryUnit, GoalProjectionState, GoalResumeAction,
    GoalResumeDecision, GoalRoleBinding, GoalRunDossier, GoalRunModelUsage,
    GoalRuntimeOwnerProfile,
};
use crate::history::schema_helpers::table_has_column_sync;

fn sample_assignment(
    role_id: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> GoalAgentAssignment {
    GoalAgentAssignment {
        role_id: role_id.to_string(),
        enabled: true,
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
        inherit_from_main: false,
    }
}

fn sample_goal_run_record(id: &str, updated_at: u64) -> GoalRun {
    GoalRun {
        id: id.to_string(),
        title: format!("Goal {id}"),
        goal: "Do the thing".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
        priority: TaskPriority::Normal,
        created_at: updated_at.saturating_sub(1),
        updated_at,
        started_at: Some(updated_at.saturating_sub(1)),
        completed_at: None,
        thread_id: Some(format!("thread-{id}")),
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("Inspect".to_string()),
        current_step_kind: Some(GoalRunStepKind::Research),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("Plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        step_failure_history: Vec::new(),
        dossier: None,
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
        steps: vec![GoalRunStep {
            id: format!("step-{id}"),
            position: 0,
            title: "Inspect".to_string(),
            instructions: "Inspect state".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "Know state".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: None,
            summary: None,
            error: None,
            started_at: Some(updated_at.saturating_sub(1)),
            completed_at: None,
        }],
        events: vec![GoalRunEvent {
            id: format!("event-{id}"),
            timestamp: updated_at,
            phase: "todo".to_string(),
            message: "goal todo updated".to_string(),
            details: None,
            step_index: Some(0),
            todo_snapshot: Vec::new(),
        }],
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

fn sample_agent_task_record(id: &str, status: TaskStatus, created_at: u64) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: format!("Task {id}"),
        description: "Do the task".to_string(),
        status,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(format!("thread-{id}")),
        source: "user".to_string(),
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

#[tokio::test]
async fn init_schema_migrates_legacy_agent_tasks_before_goal_run_index() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.conn.call(|conn| {
        conn.execute_batch("DROP TABLE IF EXISTS agent_tasks")?;
        conn.execute_batch(
        "
        CREATE TABLE agent_tasks (
            id                   TEXT PRIMARY KEY,
            title                TEXT NOT NULL,
            description          TEXT NOT NULL,
            status               TEXT NOT NULL,
            priority             TEXT NOT NULL,
            progress             INTEGER NOT NULL DEFAULT 0,
            created_at           INTEGER NOT NULL,
            started_at           INTEGER,
            completed_at         INTEGER,
            error                TEXT,
            result               TEXT,
            thread_id            TEXT,
            source               TEXT NOT NULL DEFAULT 'user',
            notify_on_complete   INTEGER NOT NULL DEFAULT 0,
            notify_channels_json TEXT NOT NULL DEFAULT '[]',
            command              TEXT,
            retry_count          INTEGER NOT NULL DEFAULT 0,
            max_retries          INTEGER NOT NULL DEFAULT 3,
            next_retry_at        INTEGER,
            blocked_reason       TEXT,
            awaiting_approval_id TEXT,
            lane_id              TEXT,
            last_error           TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_status ON agent_tasks(status, priority, created_at DESC);
        ",
    )?;
        Ok(())
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    store.init_schema().await?;

    let has_cols = store.conn.call(|conn| {
        let has_session = table_has_column_sync(conn, "agent_tasks", "session_id")?;
        let has_scheduled = table_has_column_sync(conn, "agent_tasks", "scheduled_at")?;
        let has_goal_run = table_has_column_sync(conn, "agent_tasks", "goal_run_id")?;
        let has_override_provider = table_has_column_sync(conn, "agent_tasks", "override_provider")?;
        let has_override_prompt = table_has_column_sync(conn, "agent_tasks", "override_system_prompt")?;
        let has_sub_agent_def = table_has_column_sync(conn, "agent_tasks", "sub_agent_def_id")?;
        let has_tool_whitelist = table_has_column_sync(conn, "agent_tasks", "tool_whitelist_json")?;
        let has_tool_blacklist = table_has_column_sync(conn, "agent_tasks", "tool_blacklist_json")?;
        let has_context_budget = table_has_column_sync(conn, "agent_tasks", "context_budget_tokens")?;
        let has_context_overflow = table_has_column_sync(conn, "agent_tasks", "context_overflow_action")?;
        let has_termination_conditions = table_has_column_sync(conn, "agent_tasks", "termination_conditions")?;
        let has_success_criteria = table_has_column_sync(conn, "agent_tasks", "success_criteria")?;
        let has_max_duration = table_has_column_sync(conn, "agent_tasks", "max_duration_secs")?;
        let has_supervisor_config = table_has_column_sync(conn, "agent_tasks", "supervisor_config_json")?;
        let has_completion_contract =
            table_has_column_sync(conn, "agent_tasks", "completion_contract_json")?;
        let index_name: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_agent_tasks_goal_run'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let parent_thread_subagent_index_name: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_agent_tasks_parent_thread_subagent_status'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let quiet_recovery_index_name: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_agent_tasks_goal_run_status_quiet'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok((
            has_session,
            has_scheduled,
            has_goal_run,
            has_override_provider,
            has_override_prompt,
            has_sub_agent_def,
            has_tool_whitelist,
            has_tool_blacklist,
            has_context_budget,
            has_context_overflow,
            has_termination_conditions,
            has_success_criteria,
            has_max_duration,
            has_supervisor_config,
            has_completion_contract,
            index_name,
            parent_thread_subagent_index_name,
            quiet_recovery_index_name,
        ))
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_cols.0);
    assert!(has_cols.1);
    assert!(has_cols.2);
    assert!(has_cols.3);
    assert!(has_cols.4);
    assert!(has_cols.5);
    assert!(has_cols.6);
    assert!(has_cols.7);
    assert!(has_cols.8);
    assert!(has_cols.9);
    assert!(has_cols.10);
    assert!(has_cols.11);
    assert!(has_cols.12);
    assert!(has_cols.13);
    assert!(has_cols.14);
    assert_eq!(has_cols.15.as_deref(), Some("idx_agent_tasks_goal_run"));
    assert_eq!(
        has_cols.16.as_deref(),
        Some("idx_agent_tasks_parent_thread_subagent_status")
    );
    assert_eq!(
        has_cols.17.as_deref(),
        Some("idx_agent_tasks_goal_run_status_quiet")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_task_subagent_metadata_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let task = AgentTask {
        id: "task-subagent-meta".to_string(),
        title: "Persist subagent metadata".to_string(),
        description: "Ensure task persistence keeps self-orchestration config".to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::High,
        progress: 66,
        created_at: 100,
        started_at: Some(101),
        completed_at: None,
        error: None,
        result: Some("partial".to_string()),
        thread_id: Some("thread-subagent".to_string()),
        source: "agent".to_string(),
        notify_on_complete: true,
        notify_channels: vec!["slack".to_string(), "discord".to_string()],
        dependencies: vec!["dep-1".to_string(), "dep-2".to_string()],
        command: Some("cargo test -p zorai-daemon".to_string()),
        session_id: Some("session-subagent".to_string()),
        goal_run_id: Some("goal-77".to_string()),
        goal_run_title: Some("Goal title".to_string()),
        goal_step_id: Some("step-9".to_string()),
        goal_step_title: Some("Investigate failure".to_string()),
        parent_task_id: Some("parent-task".to_string()),
        parent_thread_id: Some("parent-thread".to_string()),
        runtime: "daemon".to_string(),
        retry_count: 1,
        max_retries: 4,
        next_retry_at: Some(150),
        scheduled_at: Some(140),
        blocked_reason: Some("waiting for sibling evidence".to_string()),
        awaiting_approval_id: Some("approval-9".to_string()),
        policy_fingerprint: Some("policy-9".to_string()),
        approval_expires_at: Some(999),
        containment_scope: Some("workspace".to_string()),
        compensation_status: Some("required".to_string()),
        compensation_summary: Some("rollback staged".to_string()),
        lane_id: Some("lane-7".to_string()),
        last_error: Some("previous retry timed out".to_string()),
        logs: vec![AgentTaskLogEntry {
            id: "log-1".to_string(),
            timestamp: 111,
            level: TaskLogLevel::Warn,
            phase: "supervisor".to_string(),
            message: "stuck pattern detected".to_string(),
            details: Some("tool loop A→B→A→B".to_string()),
            attempt: 1,
        }],
        completion_contract: None,
        tool_whitelist: Some(vec!["read_file".to_string(), "search_files".to_string()]),
        tool_blacklist: Some(vec!["bash_command".to_string()]),
        context_budget_tokens: Some(20_000),
        context_overflow_action: Some(crate::agent::types::ContextOverflowAction::Truncate),
        termination_conditions: Some("timeout(300) OR error_count(3)".to_string()),
        success_criteria: Some("All targeted tests pass".to_string()),
        max_duration_secs: Some(600),
        supervisor_config: Some(crate::agent::types::SupervisorConfig {
            check_interval_secs: 45,
            stuck_timeout_secs: 240,
            max_retries: 5,
            intervention_level: crate::agent::types::InterventionLevel::Aggressive,
        }),
        override_provider: Some("github-copilot".to_string()),
        override_model: Some("gpt-5.4".to_string()),
        override_api_transport: None,
        override_system_prompt: Some("You are a focused subagent".to_string()),
        sub_agent_def_id: Some("weles_builtin".to_string()),
    };

    store.upsert_agent_task(&task).await?;
    let loaded = store
        .list_agent_tasks()
        .await?
        .into_iter()
        .find(|entry| entry.id == task.id)
        .expect("persisted task should be present");

    assert_eq!(loaded.dependencies, vec!["dep-1", "dep-2"]);
    assert_eq!(loaded.notify_channels, vec!["slack", "discord"]);
    assert_eq!(loaded.logs.len(), 1);
    assert_eq!(loaded.logs[0].phase, "supervisor");
    assert_eq!(
        loaded.tool_whitelist,
        Some(vec!["read_file".to_string(), "search_files".to_string()])
    );
    assert_eq!(
        loaded.tool_blacklist,
        Some(vec!["bash_command".to_string()])
    );
    assert_eq!(loaded.context_budget_tokens, Some(20_000));
    assert_eq!(
        loaded.context_overflow_action,
        Some(crate::agent::types::ContextOverflowAction::Truncate)
    );
    assert_eq!(
        loaded.termination_conditions.as_deref(),
        Some("timeout(300) OR error_count(3)")
    );
    assert_eq!(
        loaded.success_criteria.as_deref(),
        Some("All targeted tests pass")
    );
    assert_eq!(loaded.max_duration_secs, Some(600));
    let supervisor = loaded
        .supervisor_config
        .expect("supervisor config should round-trip");
    assert_eq!(supervisor.check_interval_secs, 45);
    assert_eq!(supervisor.stuck_timeout_secs, 240);
    assert_eq!(supervisor.max_retries, 5);
    assert_eq!(
        supervisor.intervention_level,
        crate::agent::types::InterventionLevel::Aggressive
    );
    assert_eq!(loaded.override_provider.as_deref(), Some("github-copilot"));
    assert_eq!(loaded.override_model.as_deref(), Some("gpt-5.4"));
    assert_eq!(loaded.sub_agent_def_id.as_deref(), Some("weles_builtin"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_tasks_filtered_applies_status_and_limit_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut newest_running =
        sample_agent_task_record("task-newest-running", TaskStatus::InProgress, 30);
    newest_running.dependencies = vec!["dep-newest".to_string()];
    newest_running.logs = vec![AgentTaskLogEntry {
        id: "log-newest".to_string(),
        timestamp: 31,
        level: TaskLogLevel::Info,
        phase: "run".to_string(),
        message: "running".to_string(),
        details: None,
        attempt: 0,
    }];
    let mut older_running =
        sample_agent_task_record("task-older-running", TaskStatus::InProgress, 20);
    older_running.dependencies = vec!["dep-older".to_string()];
    older_running.logs = vec![AgentTaskLogEntry {
        id: "log-older".to_string(),
        timestamp: 21,
        level: TaskLogLevel::Warn,
        phase: "run".to_string(),
        message: "older".to_string(),
        details: None,
        attempt: 0,
    }];
    let completed = sample_agent_task_record("task-completed", TaskStatus::Completed, 40);

    store.upsert_agent_task(&newest_running).await?;
    store.upsert_agent_task(&older_running).await?;
    store.upsert_agent_task(&completed).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-completed"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let tasks = store
        .list_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: Some("in_progress".to_string()),
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: Some(1),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "task-newest-running");
    assert_eq!(tasks[0].dependencies, vec!["dep-newest"]);
    assert_eq!(tasks[0].logs.len(), 1);
    assert_eq!(tasks[0].logs[0].id, "log-newest");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn count_agent_tasks_filtered_counts_rows_without_hydrating_task_payloads() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let running_one = sample_agent_task_record("task-running-one", TaskStatus::InProgress, 30);
    let running_two = sample_agent_task_record("task-running-two", TaskStatus::InProgress, 40);
    let awaiting = sample_agent_task_record("task-awaiting", TaskStatus::AwaitingApproval, 50);
    let completed = sample_agent_task_record("task-completed-count", TaskStatus::Completed, 60);

    store.upsert_agent_task(&running_one).await?;
    store.upsert_agent_task(&running_two).await?;
    store.upsert_agent_task(&awaiting).await?;
    store.upsert_agent_task(&completed).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-running-two"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let active = store
        .count_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: vec!["in_progress".to_string(), "awaiting_approval".to_string()],
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;
    assert_eq!(active, 3);

    let awaiting_count = store
        .count_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: Some("awaiting_approval".to_string()),
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;
    assert_eq!(awaiting_count, 1);

    let running_by_id = store
        .count_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some("task-running-two".to_string()),
            status: None,
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: true,
            order_by_recent_activity_desc: false,
            limit: Some(1),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;
    assert_eq!(running_by_id, 1);

    let completed_by_id = store
        .count_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some("task-completed-count".to_string()),
            status: None,
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: true,
            order_by_recent_activity_desc: false,
            limit: Some(1),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;
    assert_eq!(completed_by_id, 0);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_agent_task_approval_id_for_thread_selects_pending_id_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut older_approval =
        sample_agent_task_record("task-approval-older", TaskStatus::AwaitingApproval, 30);
    older_approval.thread_id = Some("thread-approval".to_string());
    older_approval.awaiting_approval_id = Some("approval-older".to_string());
    let mut newer_without_approval =
        sample_agent_task_record("task-approval-newer-empty", TaskStatus::InProgress, 60);
    newer_without_approval.thread_id = Some("thread-approval".to_string());
    let mut unrelated_approval =
        sample_agent_task_record("task-approval-unrelated", TaskStatus::AwaitingApproval, 70);
    unrelated_approval.thread_id = Some("thread-other".to_string());
    unrelated_approval.awaiting_approval_id = Some("approval-other".to_string());

    store.upsert_agent_task(&older_approval).await?;
    store.upsert_agent_task(&newer_without_approval).await?;
    store.upsert_agent_task(&unrelated_approval).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-approval-older"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let approval_id = store
        .latest_agent_task_approval_id_for_thread("thread-approval")
        .await?;

    assert_eq!(approval_id.as_deref(), Some("approval-older"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn has_agent_task_for_thread_checks_existence_without_hydrating_task() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut task = sample_agent_task_record("task-thread-exists", TaskStatus::Queued, 30);
    task.thread_id = Some("thread-conflict".to_string());
    store.upsert_agent_task(&task).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-thread-exists"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(store.has_agent_task_for_thread("thread-conflict").await?);
    assert!(!store.has_agent_task_for_thread("thread-missing").await?);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn has_agent_task_id_checks_existence_without_hydrating_task() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let task = sample_agent_task_record("task-id-exists", TaskStatus::Queued, 30);
    store.upsert_agent_task(&task).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-id-exists"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(store.has_agent_task_id("task-id-exists").await?);
    assert!(!store.has_agent_task_id("task-id-missing").await?);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_task_refs_filtered_selects_ids_without_hydrating_task_payloads() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut parent = sample_agent_task_record("task-ref-parent", TaskStatus::Queued, 20);
    parent.goal_run_id = Some("goal-ref".to_string());
    parent.thread_id = Some("thread-ref-parent".to_string());
    let mut child = sample_agent_task_record("task-ref-child", TaskStatus::Queued, 30);
    child.parent_task_id = Some("task-ref-parent".to_string());
    child.thread_id = Some("thread-ref-child".to_string());
    child.parent_thread_id = Some("thread-ref-parent".to_string());
    let unrelated = sample_agent_task_record("task-ref-unrelated", TaskStatus::Queued, 40);

    store.upsert_agent_task(&parent).await?;
    store.upsert_agent_task(&child).await?;
    store.upsert_agent_task(&unrelated).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id IN (?1, ?2)",
                params!["task-ref-parent", "task-ref-child"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let goal_refs = store
        .list_agent_task_refs_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: Some("goal-ref".to_string()),
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;
    assert_eq!(
        goal_refs
            .iter()
            .map(|(task_id, thread_id, parent_thread_id)| {
                (
                    task_id.as_str(),
                    thread_id.as_deref(),
                    parent_thread_id.as_deref(),
                )
            })
            .collect::<Vec<_>>(),
        vec![("task-ref-parent", Some("thread-ref-parent"), None)]
    );

    let child_refs = store
        .list_agent_task_refs_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: Some("task-ref-parent".to_string()),
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;
    assert_eq!(
        child_refs
            .iter()
            .map(|(task_id, thread_id, parent_thread_id)| {
                (
                    task_id.as_str(),
                    thread_id.as_deref(),
                    parent_thread_id.as_deref(),
                )
            })
            .collect::<Vec<_>>(),
        vec![(
            "task-ref-child",
            Some("thread-ref-child"),
            Some("thread-ref-parent")
        )]
    );

    let awaiting_refs = store
        .list_agent_task_refs_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: Some("queued".to_string()),
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;
    assert_eq!(
        awaiting_refs
            .iter()
            .map(|(task_id, _, _)| task_id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-ref-child", "task-ref-parent", "task-ref-unrelated"]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_tasks_for_parent_thread_subagents_hydrates_only_matching_rows() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut matching =
        sample_agent_task_record("task-parent-thread-match", TaskStatus::InProgress, 30);
    matching.source = "subagent".to_string();
    matching.parent_task_id = Some("task-parent-root".to_string());
    matching.parent_thread_id = Some("thread-parent-scope".to_string());
    matching.dependencies = vec!["task-blocker".to_string()];
    matching.logs = vec![AgentTaskLogEntry {
        id: "log-parent-thread-match".to_string(),
        timestamp: 31,
        level: TaskLogLevel::Info,
        phase: "analysis".to_string(),
        message: "matching log".to_string(),
        details: None,
        attempt: 0,
    }];

    let mut unrelated =
        sample_agent_task_record("task-parent-thread-other", TaskStatus::InProgress, 40);
    unrelated.source = "subagent".to_string();
    unrelated.parent_task_id = Some("task-other-root".to_string());
    unrelated.parent_thread_id = Some("thread-other-scope".to_string());
    unrelated.dependencies = vec!["task-unrelated-blocker".to_string()];
    unrelated.logs = vec![AgentTaskLogEntry {
        id: "log-parent-thread-other".to_string(),
        timestamp: 41,
        level: TaskLogLevel::Warn,
        phase: "analysis".to_string(),
        message: "unrelated log".to_string(),
        details: None,
        attempt: 0,
    }];

    store.upsert_agent_task(&matching).await?;
    store.upsert_agent_task(&unrelated).await?;

    let tasks = store
        .list_agent_tasks_for_parent_thread_subagents("thread-parent-scope", None, None, true)
        .await?;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, matching.id);
    assert_eq!(tasks[0].dependencies, vec!["task-blocker".to_string()]);
    assert_eq!(tasks[0].logs.len(), 1);
    assert_eq!(tasks[0].logs[0].message, "matching log");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn parent_thread_subagents_filter_status_before_hydrating_tasks() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut matching = sample_agent_task_record(
        "task-parent-thread-status-match",
        TaskStatus::InProgress,
        30,
    );
    matching.source = "subagent".to_string();
    matching.parent_task_id = Some("task-parent-root".to_string());
    matching.parent_thread_id = Some("thread-parent-status-scope".to_string());

    let mut wrong_status =
        sample_agent_task_record("task-parent-thread-status-other", TaskStatus::Queued, 40);
    wrong_status.source = "subagent".to_string();
    wrong_status.parent_task_id = Some("task-parent-root".to_string());
    wrong_status.parent_thread_id = Some("thread-parent-status-scope".to_string());

    store.upsert_agent_task(&matching).await?;
    store.upsert_agent_task(&wrong_status).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-parent-thread-status-other"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let tasks = store
        .list_agent_tasks_for_parent_thread_subagents(
            "thread-parent-status-scope",
            Some("in_progress"),
            None,
            true,
        )
        .await?;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, matching.id);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_task_ids_filtered_selects_ids_without_hydrating_task_payloads() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut active_child =
        sample_agent_task_record("task-id-active-child", TaskStatus::InProgress, 30);
    active_child.source = "subagent".to_string();
    active_child.parent_task_id = Some("task-id-parent".to_string());
    let mut queued_child = sample_agent_task_record("task-id-queued-child", TaskStatus::Queued, 20);
    queued_child.source = "subagent".to_string();
    queued_child.parent_task_id = Some("task-id-parent".to_string());
    let mut terminal_child =
        sample_agent_task_record("task-id-terminal-child", TaskStatus::Completed, 40);
    terminal_child.source = "subagent".to_string();
    terminal_child.parent_task_id = Some("task-id-parent".to_string());
    let mut unrelated = sample_agent_task_record("task-id-unrelated-child", TaskStatus::Queued, 50);
    unrelated.source = "subagent".to_string();
    unrelated.parent_task_id = Some("task-id-other-parent".to_string());

    store.upsert_agent_task(&active_child).await?;
    store.upsert_agent_task(&queued_child).await?;
    store.upsert_agent_task(&terminal_child).await?;
    store.upsert_agent_task(&unrelated).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id IN (?1, ?2)",
                params!["task-id-active-child", "task-id-queued-child"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let task_ids = store
        .list_agent_task_ids_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: Some("subagent".to_string()),
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: Some("task-id-parent".to_string()),
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: true,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;

    assert_eq!(
        task_ids,
        vec![
            "task-id-active-child".to_string(),
            "task-id-queued-child".to_string()
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_task_goal_context_selects_goal_columns_without_hydrating_task() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut task = sample_agent_task_record("task-goal-context", TaskStatus::InProgress, 30);
    task.goal_run_id = Some("goal-context".to_string());
    task.goal_step_id = Some("step-context".to_string());
    task.session_id = Some("session-context".to_string());
    task.source = "goal_run".to_string();
    task.parent_task_id = Some("task-parent-context".to_string());

    store.upsert_agent_task(&task).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-goal-context"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let context = store
        .agent_task_goal_context("task-goal-context")
        .await?
        .expect("task goal context should exist");

    assert_eq!(context.goal_run_id.as_deref(), Some("goal-context"));
    assert_eq!(context.goal_step_id.as_deref(), Some("step-context"));
    assert_eq!(context.session_id.as_deref(), Some("session-context"));
    assert_eq!(context.source, "goal_run");
    assert_eq!(
        context.parent_task_id.as_deref(),
        Some("task-parent-context")
    );
    assert!(store
        .agent_task_goal_context("task-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_task_titles_filtered_selects_titles_without_hydrating_task_payloads(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_agent_task_record("task-title-running", TaskStatus::InProgress, 20);
    running.source = "handoff".to_string();
    running.title = "[frontend] Build the panel".to_string();
    let mut awaiting =
        sample_agent_task_record("task-title-awaiting", TaskStatus::AwaitingApproval, 30);
    awaiting.source = "handoff".to_string();
    awaiting.title = "[backend] Wire the endpoint".to_string();
    let mut unrelated =
        sample_agent_task_record("task-title-unrelated", TaskStatus::InProgress, 40);
    unrelated.source = "user".to_string();
    unrelated.title = "[frontend] User task".to_string();

    store.upsert_agent_task(&running).await?;
    store.upsert_agent_task(&awaiting).await?;
    store.upsert_agent_task(&unrelated).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-title-awaiting"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let titles = store
        .list_agent_task_titles_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: vec!["in_progress".to_string(), "awaiting_approval".to_string()],
            source: Some("handoff".to_string()),
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;

    assert_eq!(
        titles,
        vec![
            (
                "task-title-awaiting".to_string(),
                "[backend] Wire the endpoint".to_string(),
            ),
            (
                "task-title-running".to_string(),
                "[frontend] Build the panel".to_string(),
            ),
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_task_provider_override_selects_override_columns_without_hydrating_task_payload(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut task = sample_agent_task_record("task-provider-override", TaskStatus::Queued, 20);
    task.override_provider = Some("openai".to_string());
    task.override_model = Some("gpt-5.4-mini".to_string());
    let mut no_override =
        sample_agent_task_record("task-provider-no-override", TaskStatus::Queued, 30);
    no_override.override_model = Some("ignored-model".to_string());

    store.upsert_agent_task(&task).await?;
    store.upsert_agent_task(&no_override).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-provider-override"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store
            .agent_task_provider_override("task-provider-override")
            .await?,
        Some(("openai".to_string(), Some("gpt-5.4-mini".to_string())))
    );
    assert!(store
        .agent_task_provider_override("task-provider-no-override")
        .await?
        .is_none());
    assert!(store
        .agent_task_provider_override("task-provider-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_task_override_system_prompt_selects_prompt_without_hydrating_task_payload(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut task = sample_agent_task_record("task-system-prompt", TaskStatus::Queued, 20);
    task.override_system_prompt =
        Some("You are Review Bot (review-bot) operating as a spawned zorai agent.".to_string());
    let no_prompt = sample_agent_task_record("task-system-prompt-missing", TaskStatus::Queued, 30);

    store.upsert_agent_task(&task).await?;
    store.upsert_agent_task(&no_prompt).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-system-prompt"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store
            .agent_task_override_system_prompt("task-system-prompt")
            .await?,
        Some(Some(
            "You are Review Bot (review-bot) operating as a spawned zorai agent.".to_string()
        ))
    );
    assert_eq!(
        store
            .agent_task_override_system_prompt("task-system-prompt-missing")
            .await?,
        Some(None)
    );
    assert!(store
        .agent_task_override_system_prompt("task-system-prompt-unknown")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_task_operational_refs_selects_prompt_fields_without_hydrating_task_payloads(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut active =
        sample_agent_task_record("task-operational-active", TaskStatus::InProgress, 20);
    active.title = "Keep the prompt small".to_string();
    active.progress = 42;
    active.awaiting_approval_id = Some("approval-operational".to_string());
    let terminal =
        sample_agent_task_record("task-operational-completed", TaskStatus::Completed, 30);

    store.upsert_agent_task(&active).await?;
    store.upsert_agent_task(&terminal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-operational-active"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_agent_task_operational_refs_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: true,
            order_by_recent_activity_desc: false,
            limit: Some(4),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, "task-operational-active");
    assert_eq!(refs[0].title, "Keep the prompt small");
    assert_eq!(refs[0].status, TaskStatus::InProgress);
    assert_eq!(refs[0].progress, 42);
    assert_eq!(
        refs[0].awaiting_approval_id.as_deref(),
        Some("approval-operational")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_task_summary_refs_selects_summary_fields_without_hydrating_task_payloads(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut high = sample_agent_task_record("task-summary-high", TaskStatus::InProgress, 20);
    high.title = "Summarize only these columns".to_string();
    high.priority = TaskPriority::High;
    let terminal = sample_agent_task_record("task-summary-completed", TaskStatus::Completed, 30);

    store.upsert_agent_task(&high).await?;
    store.upsert_agent_task(&terminal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["task-summary-high"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_agent_task_summary_refs_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: true,
            order_by_recent_activity_desc: false,
            limit: Some(4),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, "task-summary-high");
    assert_eq!(refs[0].title, "Summarize only these columns");
    assert_eq!(refs[0].status, TaskStatus::InProgress);
    assert_eq!(refs[0].priority, TaskPriority::High);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_task_quiet_recovery_refs_selects_activity_fields_without_hydrating_task_payloads(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut active = sample_agent_task_record("task-quiet-active", TaskStatus::InProgress, 20);
    active.source = "goal_run".to_string();
    active.progress = 64;
    active.goal_run_id = Some("goal-quiet".to_string());
    active.thread_id = Some("thread-quiet-active".to_string());
    let mut child = sample_agent_task_record("task-quiet-child", TaskStatus::Queued, 30);
    child.goal_run_id = Some("goal-quiet".to_string());
    child.parent_task_id = Some("task-quiet-active".to_string());
    child.thread_id = Some("thread-quiet-child".to_string());
    let terminal = sample_agent_task_record("task-quiet-completed", TaskStatus::Completed, 40);

    store.upsert_agent_task(&active).await?;
    store.upsert_agent_task(&child).await?;
    store.upsert_agent_task(&terminal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET retry_count = 'not-an-integer' WHERE id IN (?1, ?2)",
                params!["task-quiet-active", "task-quiet-child"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_agent_task_quiet_recovery_refs_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: vec![
                "in_progress".to_string(),
                "blocked".to_string(),
                "queued".to_string(),
            ],
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;

    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].id, "task-quiet-active");
    assert_eq!(refs[0].status, TaskStatus::InProgress);
    assert_eq!(refs[0].source, "goal_run");
    assert_eq!(refs[0].progress, 64);
    assert_eq!(refs[0].goal_run_id.as_deref(), Some("goal-quiet"));
    assert_eq!(refs[0].thread_id.as_deref(), Some("thread-quiet-active"));
    assert_eq!(refs[1].id, "task-quiet-child");
    assert_eq!(refs[1].parent_task_id.as_deref(), Some("task-quiet-active"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn quiet_recovery_refs_for_goal_runs_statuses_filters_goal_run_ids_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut matching =
        sample_agent_task_record("task-quiet-goal-match", TaskStatus::InProgress, 30);
    matching.goal_run_id = Some("goal-quiet-target".to_string());

    let mut wrong_goal =
        sample_agent_task_record("task-quiet-goal-other", TaskStatus::InProgress, 40);
    wrong_goal.goal_run_id = Some("goal-quiet-other".to_string());

    let mut wrong_status =
        sample_agent_task_record("task-quiet-goal-status", TaskStatus::Completed, 50);
    wrong_status.goal_run_id = Some("goal-quiet-target".to_string());

    store.upsert_agent_task(&matching).await?;
    store.upsert_agent_task(&wrong_goal).await?;
    store.upsert_agent_task(&wrong_status).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id IN (?1, ?2)",
                params!["task-quiet-goal-other", "task-quiet-goal-status"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_agent_task_quiet_recovery_refs_for_goal_runs_statuses(
            &["goal-quiet-target".to_string()],
            &["in_progress".to_string()],
        )
        .await?;

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, matching.id);
    assert_eq!(refs[0].goal_run_id.as_deref(), Some("goal-quiet-target"));
    assert_eq!(refs[0].status, TaskStatus::InProgress);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_task_subagent_hierarchy_refs_selects_depth_fields_without_hydrating_payloads(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut parent = sample_agent_task_record("task-subagent-parent", TaskStatus::InProgress, 20);
    parent.source = "subagent".to_string();
    parent.containment_scope = Some("subagent-depth:1/3".to_string());
    let mut child = sample_agent_task_record("task-subagent-child", TaskStatus::Queued, 30);
    child.source = "subagent".to_string();
    child.parent_task_id = Some("task-subagent-parent".to_string());
    let mut user_task = sample_agent_task_record("task-user", TaskStatus::Queued, 40);
    user_task.source = "user".to_string();

    store.upsert_agent_task(&parent).await?;
    store.upsert_agent_task(&child).await?;
    store.upsert_agent_task(&user_task).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET retry_count = 'not-an-integer' WHERE id IN (?1, ?2)",
                params!["task-subagent-parent", "task-subagent-child"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_agent_task_subagent_hierarchy_refs_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: Some("subagent".to_string()),
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;

    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].id, "task-subagent-child");
    assert_eq!(
        refs[0].parent_task_id.as_deref(),
        Some("task-subagent-parent")
    );
    assert_eq!(refs[1].id, "task-subagent-parent");
    assert_eq!(
        refs[1].containment_scope.as_deref(),
        Some("subagent-depth:1/3")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_operational_refs_selects_prompt_fields_without_hydrating_events(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_goal_run_record("goal-operational-running", 40);
    running.title = "Run the focused projection".to_string();
    running.current_step_index = 1;
    running.steps.push(GoalRunStep {
        id: "step-goal-operational-running-2".to_string(),
        position: 1,
        title: "Verify".to_string(),
        instructions: "Verify projection".to_string(),
        kind: GoalRunStepKind::Command,
        success_criteria: "Projection verified".to_string(),
        session_id: None,
        status: GoalRunStepStatus::Pending,
        task_id: None,
        summary: None,
        error: None,
        started_at: None,
        completed_at: None,
    });
    let mut completed = sample_goal_run_record("goal-operational-completed", 30);
    completed.status = GoalRunStatus::Completed;

    store.upsert_goal_run(&running).await?;
    store.upsert_goal_run(&completed).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_events SET timestamp = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-operational-running"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_goal_run_operational_refs_for_statuses_limited(&[GoalRunStatus::Running], Some(3))
        .await?;

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, "goal-operational-running");
    assert_eq!(refs[0].title, "Run the focused projection");
    assert_eq!(refs[0].status, GoalRunStatus::Running);
    assert_eq!(refs[0].current_step_index, 1);
    assert_eq!(refs[0].step_count, 2);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_brief_refs_selects_morning_brief_fields_without_hydrating_events(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_goal_run_record("goal-brief-running", 40);
    running.title = "Pick up the useful work".to_string();
    running.thread_id = Some("thread-brief".to_string());
    running.current_step_index = 0;
    let mut paused = sample_goal_run_record("goal-brief-paused", 30);
    paused.status = GoalRunStatus::Paused;
    paused.title = "Review paused work".to_string();
    let mut completed = sample_goal_run_record("goal-brief-completed", 20);
    completed.status = GoalRunStatus::Completed;

    store.upsert_goal_run(&running).await?;
    store.upsert_goal_run(&paused).await?;
    store.upsert_goal_run(&completed).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_events SET timestamp = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-brief-running"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_goal_run_brief_refs_for_statuses(&[GoalRunStatus::Running, GoalRunStatus::Paused])
        .await?;

    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].id, "goal-brief-running");
    assert_eq!(refs[0].title, "Pick up the useful work");
    assert_eq!(refs[0].status, GoalRunStatus::Running);
    assert_eq!(refs[0].thread_id.as_deref(), Some("thread-brief"));
    assert_eq!(refs[0].current_step_title.as_deref(), Some("Inspect"));
    assert_eq!(refs[1].id, "goal-brief-paused");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_stuck_check_refs_selects_check_fields_without_hydrating_events() -> Result<()>
{
    let (store, root) = make_test_store().await?;

    let mut old_running = sample_goal_run_record("goal-stuck-running", 10);
    old_running.status = GoalRunStatus::Running;
    old_running.title = "Old running goal".to_string();
    old_running.last_error = Some("provider stalled".to_string());
    let mut old_awaiting = sample_goal_run_record("goal-stuck-awaiting", 20);
    old_awaiting.status = GoalRunStatus::AwaitingApproval;
    old_awaiting.title = "Old approval goal".to_string();
    let mut fresh_running = sample_goal_run_record("goal-stuck-fresh", 100);
    fresh_running.status = GoalRunStatus::Running;
    let mut completed = sample_goal_run_record("goal-stuck-completed", 5);
    completed.status = GoalRunStatus::Completed;

    store.upsert_goal_run(&old_running).await?;
    store.upsert_goal_run(&old_awaiting).await?;
    store.upsert_goal_run(&fresh_running).await?;
    store.upsert_goal_run(&completed).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_events SET timestamp = 'not-an-integer' WHERE goal_run_id IN (?1, ?2)",
                params!["goal-stuck-running", "goal-stuck-awaiting"],
            )?;
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id IN (?1, ?2)",
                params!["goal-stuck-running", "goal-stuck-awaiting"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_goal_run_stuck_check_refs_updated_before(
            &[
                GoalRunStatus::Running,
                GoalRunStatus::Planning,
                GoalRunStatus::AwaitingApproval,
            ],
            50,
        )
        .await?;

    assert_eq!(
        refs.iter()
            .map(|goal| {
                (
                    goal.id.as_str(),
                    goal.status,
                    goal.title.as_str(),
                    goal.updated_at,
                    goal.last_error.as_deref(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                "goal-stuck-running",
                GoalRunStatus::Running,
                "Old running goal",
                10,
                Some("provider stalled"),
            ),
            (
                "goal-stuck-awaiting",
                GoalRunStatus::AwaitingApproval,
                "Old approval goal",
                20,
                None,
            ),
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_quiet_recovery_refs_selects_recovery_fields_without_hydrating_events(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_goal_run_record("goal-quiet-ref-running", 40);
    running.status = GoalRunStatus::Running;
    running.thread_id = Some("thread-quiet-root".to_string());
    running.root_thread_id = Some("thread-quiet-root".to_string());
    running.execution_thread_ids = vec!["thread-quiet-worker".to_string()];
    running.active_task_id = Some("task-quiet-main".to_string());
    running.current_step_index = 0;
    running.current_step_title = None;
    let mut completed = sample_goal_run_record("goal-quiet-ref-completed", 50);
    completed.status = GoalRunStatus::Completed;

    store.upsert_goal_run(&running).await?;
    store.upsert_goal_run(&completed).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_events SET timestamp = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-quiet-ref-running"],
            )?;
            conn.execute(
                "UPDATE goal_run_steps SET status = 'not-a-step-status' WHERE goal_run_id = ?1",
                params!["goal-quiet-ref-running"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_goal_run_quiet_recovery_refs_for_statuses(&[GoalRunStatus::Running])
        .await?;

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, "goal-quiet-ref-running");
    assert_eq!(refs[0].status, GoalRunStatus::Running);
    assert_eq!(refs[0].thread_id.as_deref(), Some("thread-quiet-root"));
    assert_eq!(refs[0].root_thread_id.as_deref(), Some("thread-quiet-root"));
    assert_eq!(refs[0].execution_thread_ids, vec!["thread-quiet-worker"]);
    assert_eq!(refs[0].active_task_id.as_deref(), Some("task-quiet-main"));
    assert_eq!(
        refs[0].current_step_id.as_deref(),
        Some("step-goal-quiet-ref-running")
    );
    assert_eq!(refs[0].current_step_title.as_deref(), Some("Inspect"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_agent_tasks_filtered_finds_active_subagent_children_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut active_child =
        sample_agent_task_record("task-active-child", TaskStatus::InProgress, 30);
    active_child.source = "subagent".to_string();
    active_child.parent_task_id = Some("task-parent".to_string());
    let mut completed_child =
        sample_agent_task_record("task-completed-child", TaskStatus::Completed, 40);
    completed_child.source = "subagent".to_string();
    completed_child.parent_task_id = Some("task-parent".to_string());
    let mut unrelated_child =
        sample_agent_task_record("task-unrelated-child", TaskStatus::InProgress, 50);
    unrelated_child.source = "subagent".to_string();
    unrelated_child.parent_task_id = Some("task-other-parent".to_string());

    store.upsert_agent_task(&active_child).await?;
    store.upsert_agent_task(&completed_child).await?;
    store.upsert_agent_task(&unrelated_child).await?;

    let tasks = store
        .list_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: Some("subagent".to_string()),
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: Some("task-parent".to_string()),
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: true,
            order_by_recent_activity_desc: false,
            limit: Some(1),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await?;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "task-active-child");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_agent_task_session_for_thread_filters_thread_and_session_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut older = sample_agent_task_record("task-session-older", TaskStatus::Queued, 20);
    older.thread_id = Some("thread-session-sql".to_string());
    older.session_id = Some("session-older".to_string());
    let mut newest = sample_agent_task_record("task-session-newest", TaskStatus::Queued, 30);
    newest.thread_id = Some("thread-session-sql".to_string());
    newest.session_id = Some("session-newest".to_string());
    let mut unrelated = sample_agent_task_record("task-session-other", TaskStatus::Queued, 40);
    unrelated.thread_id = Some("thread-other".to_string());
    unrelated.session_id = Some("session-other".to_string());
    let mut missing_session =
        sample_agent_task_record("task-session-missing", TaskStatus::Queued, 50);
    missing_session.thread_id = Some("thread-session-sql".to_string());
    missing_session.session_id = None;

    store.upsert_agent_task(&older).await?;
    store.upsert_agent_task(&newest).await?;
    store.upsert_agent_task(&unrelated).await?;
    store.upsert_agent_task(&missing_session).await?;

    assert_eq!(
        store
            .latest_agent_task_session_for_thread("thread-session-sql")
            .await?
            .as_deref(),
        Some("session-newest")
    );
    assert!(store
        .latest_agent_task_session_for_thread("thread-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_event_todo_snapshot_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let goal_run = GoalRun {
        id: "goal-1".to_string(),
        title: "Goal".to_string(),
        goal: "Do the thing".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
        priority: TaskPriority::Normal,
        created_at: 1,
        updated_at: 2,
        started_at: Some(1),
        completed_at: None,
        thread_id: Some("thread-1".to_string()),
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("Inspect".to_string()),
        current_step_kind: Some(GoalRunStepKind::Research),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("Plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        step_failure_history: Vec::new(),
        dossier: None,
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
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "Inspect".to_string(),
            instructions: "Inspect state".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "Know state".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: None,
            summary: None,
            error: None,
            started_at: Some(1),
            completed_at: None,
        }],
        events: vec![GoalRunEvent {
            id: "event-1".to_string(),
            timestamp: 3,
            phase: "todo".to_string(),
            message: "goal todo updated".to_string(),
            details: None,
            step_index: Some(0),
            todo_snapshot: vec![crate::agent::types::TodoItem {
                id: "todo-1".to_string(),
                content: "Inspect state".to_string(),
                status: crate::agent::types::TodoStatus::InProgress,
                position: 0,
                step_index: Some(0),
                created_at: 3,
                updated_at: 3,
            }],
        }],
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
    };

    store.upsert_goal_run(&goal_run).await?;
    let loaded = store
        .get_goal_run("goal-1")
        .await?
        .expect("goal run should exist after upsert");

    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.events[0].step_index, Some(0));
    assert_eq!(loaded.events[0].todo_snapshot.len(), 1);
    assert_eq!(loaded.events[0].todo_snapshot[0].content, "Inspect state");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn get_goal_run_ignores_unrelated_malformed_rows() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let target = sample_goal_run_record("goal-target", 20);
    let unrelated = sample_goal_run_record("goal-unrelated", 30);
    store.upsert_goal_run(&target).await?;
    store.upsert_goal_run(&unrelated).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_runs SET updated_at = 'not-an-integer' WHERE id = ?1",
                params!["goal-unrelated"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let loaded = store
        .get_goal_run("goal-target")
        .await?
        .expect("target goal run should load");

    assert_eq!(loaded.id, "goal-target");
    assert_eq!(loaded.steps.len(), 1);
    assert_eq!(loaded.events.len(), 1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn concierge_goal_context_loads_latest_goal_and_counts_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut old_running = sample_goal_run_record("goal-old-running", 10);
    old_running.status = GoalRunStatus::Running;
    let mut latest_paused = sample_goal_run_record("goal-latest-paused", 30);
    latest_paused.title = "Latest paused goal".to_string();
    latest_paused.status = GoalRunStatus::Paused;
    let mut old_paused = sample_goal_run_record("goal-old-paused", 20);
    old_paused.status = GoalRunStatus::Paused;

    store.upsert_goal_run(&old_running).await?;
    store.upsert_goal_run(&latest_paused).await?;
    store.upsert_goal_run(&old_paused).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_runs SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["goal-old-paused"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let context = store.concierge_goal_context().await?;

    assert_eq!(
        context
            .latest_goal_run
            .as_ref()
            .map(|goal_run| goal_run.id.as_str()),
        Some("goal-latest-paused")
    );
    assert_eq!(context.running_goal_total, 1);
    assert_eq!(context.paused_goal_total, 2);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_runs_for_statuses_limited_bounds_id_selection_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut old_running = sample_goal_run_record("goal-old-running", 10);
    old_running.status = GoalRunStatus::Running;
    let mut newest_paused = sample_goal_run_record("goal-newest-paused", 40);
    newest_paused.status = GoalRunStatus::Paused;
    let mut middle_running = sample_goal_run_record("goal-middle-running", 30);
    middle_running.status = GoalRunStatus::Running;
    let mut completed = sample_goal_run_record("goal-completed", 50);
    completed.status = GoalRunStatus::Completed;

    store.upsert_goal_run(&old_running).await?;
    store.upsert_goal_run(&newest_paused).await?;
    store.upsert_goal_run(&middle_running).await?;
    store.upsert_goal_run(&completed).await?;

    let rows = store
        .list_goal_runs_for_statuses_limited(
            &[GoalRunStatus::Running, GoalRunStatus::Paused],
            Some(2),
        )
        .await?;

    assert_eq!(
        rows.iter().map(|goal| goal.id.as_str()).collect::<Vec<_>>(),
        vec!["goal-newest-paused", "goal-middle-running"]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_ids_for_statuses_selects_only_matching_ids_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_goal_run_record("goal-id-running", 10);
    running.status = GoalRunStatus::Running;
    let mut paused = sample_goal_run_record("goal-id-paused", 30);
    paused.status = GoalRunStatus::Paused;
    let mut completed = sample_goal_run_record("goal-id-completed", 40);
    completed.status = GoalRunStatus::Completed;

    store.upsert_goal_run(&running).await?;
    store.upsert_goal_run(&paused).await?;
    store.upsert_goal_run(&completed).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-id-running"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let ids = store
        .list_goal_run_ids_for_statuses(&[GoalRunStatus::Running, GoalRunStatus::Paused])
        .await?;

    assert_eq!(ids, vec!["goal-id-paused", "goal-id-running"]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn pause_interrupted_goal_runs_on_restart_updates_status_and_events_without_hydrating(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_goal_run_record("goal-restart-running", 10);
    running.status = GoalRunStatus::Running;
    let mut planning = sample_goal_run_record("goal-restart-planning", 20);
    planning.status = GoalRunStatus::Planning;
    let mut queued = sample_goal_run_record("goal-restart-queued", 30);
    queued.status = GoalRunStatus::Queued;
    let mut paused = sample_goal_run_record("goal-restart-paused", 40);
    paused.status = GoalRunStatus::Paused;

    store.upsert_goal_run(&running).await?;
    store.upsert_goal_run(&planning).await?;
    store.upsert_goal_run(&queued).await?;
    store.upsert_goal_run(&paused).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id IN (?1, ?2)",
                params!["goal-restart-running", "goal-restart-planning"],
            )?;
            conn.execute(
                "UPDATE goal_run_events SET timestamp = 'not-an-integer' WHERE goal_run_id IN (?1, ?2)",
                params!["goal-restart-running", "goal-restart-planning"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let paused_count = store.pause_interrupted_goal_runs_on_restart(500).await?;

    assert_eq!(paused_count, 2);
    let rows = store
        .conn
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, status FROM goal_runs \
                 WHERE id LIKE 'goal-restart-%' \
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(
        rows,
        vec![
            ("goal-restart-paused".to_string(), "paused".to_string()),
            ("goal-restart-planning".to_string(), "paused".to_string()),
            ("goal-restart-queued".to_string(), "queued".to_string()),
            ("goal-restart-running".to_string(), "paused".to_string()),
        ]
    );

    let restart_events = store
        .conn
        .call(|conn| {
            conn.query_row(
                "SELECT COUNT(1) FROM goal_run_events \
                 WHERE goal_run_id IN (?1, ?2) \
                   AND phase = 'restart' \
                   AND timestamp = ?3 \
                   AND message = ?4 \
                   AND deleted_at IS NULL",
                params![
                    "goal-restart-running",
                    "goal-restart-planning",
                    500_i64,
                    "Daemon restarted; goal run paused for operator review.",
                ],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(restart_events, 2);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn has_goal_run_id_checks_existence_without_hydrating_steps() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let running = sample_goal_run_record("goal-id-exists-fast", 10);
    store.upsert_goal_run(&running).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-id-exists-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(store.has_goal_run_id("goal-id-exists-fast").await?);
    assert!(!store.has_goal_run_id("goal-id-missing-fast").await?);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_replan_count_selects_count_without_hydrating_steps() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut goal = sample_goal_run_record("goal-replan-count-fast", 10);
    goal.replan_count = 7;
    store.upsert_goal_run(&goal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-replan-count-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store
            .goal_run_replan_count("goal-replan-count-fast")
            .await?,
        Some(7)
    );
    assert_eq!(
        store.goal_run_replan_count("goal-replan-missing").await?,
        None
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_task_context_selects_context_without_hydrating_steps() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut goal = sample_goal_run_record("goal-task-context-fast", 10);
    goal.current_step_index = 2;
    goal.session_id = Some("session-goal-context".to_string());
    store.upsert_goal_run(&goal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-task-context-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let context = store
        .goal_run_task_context("goal-task-context-fast")
        .await?
        .expect("goal context should exist");

    assert_eq!(context.current_step_index, 2);
    assert_eq!(context.session_id.as_deref(), Some("session-goal-context"));
    assert!(store
        .goal_run_task_context("goal-task-context-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_current_step_title_selects_title_without_hydrating_steps() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut goal = sample_goal_run_record("goal-current-title-fast", 10);
    goal.current_step_title = None;
    goal.steps[0].title = "Projected current step".to_string();
    store.upsert_goal_run(&goal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-current-title-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store
            .goal_run_current_step_title("goal-current-title-fast")
            .await?
            .as_deref(),
        Some("Projected current step")
    );
    assert!(store
        .goal_run_current_step_title("goal-current-title-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_progress_metrics_counts_steps_without_hydrating_steps() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut goal = sample_goal_run_record("goal-progress-metrics-fast", 10);
    goal.steps[0].status = GoalRunStepStatus::Completed;
    let mut pending_step = goal.steps[0].clone();
    pending_step.id = "step-goal-progress-metrics-fast-2".to_string();
    pending_step.position = 1;
    pending_step.title = "Continue".to_string();
    pending_step.status = GoalRunStepStatus::InProgress;
    goal.steps.push(pending_step);
    store.upsert_goal_run(&goal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-progress-metrics-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let progress = store
        .goal_run_progress_metrics("goal-progress-metrics-fast")
        .await?
        .expect("goal progress should exist");

    assert_eq!(progress.steps_completed, 1);
    assert_eq!(progress.steps_total, 2);
    assert!(store
        .goal_run_progress_metrics("goal-progress-metrics-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_policy_context_selects_prompt_fields_without_hydrating_steps() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut goal = sample_goal_run_record("goal-policy-context-fast", 10);
    goal.goal = "Finish the policy context projection".to_string();
    goal.title = "Policy context goal".to_string();
    goal.current_step_title = None;
    goal.steps[0].title = "Projected policy step".to_string();
    goal.steps[0].status = GoalRunStepStatus::Completed;
    let mut pending_step = goal.steps[0].clone();
    pending_step.id = "step-goal-policy-context-fast-2".to_string();
    pending_step.position = 1;
    pending_step.title = "Continue policy context".to_string();
    pending_step.status = GoalRunStepStatus::InProgress;
    goal.steps.push(pending_step);
    store.upsert_goal_run(&goal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-policy-context-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let context = store
        .goal_run_policy_context("goal-policy-context-fast")
        .await?
        .expect("policy context should exist");

    assert_eq!(context.goal, "Finish the policy context projection");
    assert_eq!(context.title, "Policy context goal");
    assert_eq!(
        context.current_step_title.as_deref(),
        Some("Projected policy step")
    );
    assert_eq!(context.steps_completed, 1);
    assert_eq!(context.steps_total, 2);
    assert!(store
        .goal_run_policy_context("goal-policy-context-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_todo_context_selects_current_step_without_hydrating_goal() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut goal = sample_goal_run_record("goal-todo-context-fast", 10);
    goal.current_step_index = 1;
    goal.steps = vec![
        GoalRunStep {
            id: "step-todo-zero".to_string(),
            position: 0,
            title: "Zero".to_string(),
            instructions: "Zero".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "Zero".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Completed,
            task_id: None,
            summary: None,
            error: None,
            started_at: Some(1),
            completed_at: Some(2),
        },
        GoalRunStep {
            id: "step-todo-current".to_string(),
            position: 1,
            title: "Current".to_string(),
            instructions: "Current".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "Current".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: None,
            summary: None,
            error: None,
            started_at: Some(3),
            completed_at: None,
        },
    ];
    store.upsert_goal_run(&goal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-todo-context-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let context = store
        .goal_run_todo_context("goal-todo-context-fast", None)
        .await?
        .expect("goal todo context should exist");

    assert_eq!(context.step_index, 1);
    assert_eq!(context.step_id.as_deref(), Some("step-todo-current"));
    assert_eq!(context.step_status, Some(GoalRunStepStatus::InProgress));
    let requested = store
        .goal_run_todo_context("goal-todo-context-fast", Some("step-todo-zero"))
        .await?
        .expect("requested goal todo context should exist");
    assert_eq!(requested.step_index, 0);
    assert_eq!(requested.step_id.as_deref(), Some("step-todo-zero"));
    assert_eq!(requested.step_status, Some(GoalRunStepStatus::Completed));
    assert!(store
        .goal_run_todo_context("goal-todo-context-missing", None)
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_workspace_runtime_ref_selects_status_and_summaries_without_hydrating_steps(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut goal = sample_goal_run_record("goal-workspace-runtime-fast", 10);
    goal.status = GoalRunStatus::Completed;
    goal.last_error = Some("runtime failed before retry".to_string());
    goal.reflection_summary = Some("Finished workspace goal".to_string());
    goal.plan_summary = Some("Workspace plan".to_string());
    store.upsert_goal_run(&goal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-workspace-runtime-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let runtime = store
        .goal_run_workspace_runtime_ref("goal-workspace-runtime-fast")
        .await?
        .expect("workspace runtime ref should exist");

    assert_eq!(runtime.id, "goal-workspace-runtime-fast");
    assert_eq!(runtime.status, GoalRunStatus::Completed);
    assert_eq!(
        runtime.last_error.as_deref(),
        Some("runtime failed before retry")
    );
    assert_eq!(
        runtime.reflection_summary.as_deref(),
        Some("Finished workspace goal")
    );
    assert_eq!(runtime.plan_summary.as_deref(), Some("Workspace plan"));
    assert!(store
        .goal_run_workspace_runtime_ref("goal-workspace-runtime-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_compaction_scope_ref_selects_snapshot_without_hydrating_goal() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut goal = sample_goal_run_record("goal-compaction-scope-fast", 20);
    goal.title = "Compaction goal".to_string();
    goal.goal = "Preserve compacted context".to_string();
    goal.status = GoalRunStatus::Running;
    goal.thread_id = Some("thread-compaction-root".to_string());
    goal.root_thread_id = Some("thread-compaction-root".to_string());
    goal.active_thread_id = Some("thread-compaction-active".to_string());
    goal.execution_thread_ids = vec![
        "thread-compaction-root".to_string(),
        "thread-compaction-active".to_string(),
    ];
    goal.active_task_id = Some("task-compaction-active".to_string());
    goal.current_step_index = 0;
    goal.steps[0].title = "Inspect compacted state".to_string();
    goal.steps[0].status = GoalRunStepStatus::InProgress;
    goal.steps[0].summary = Some("Current inspection summary".to_string());
    goal.plan_summary = Some("Compaction plan summary".to_string());
    goal.last_error = Some("latest goal error".to_string());
    goal.events = vec![GoalRunEvent {
        id: "event-compaction-scope".to_string(),
        timestamp: 21,
        phase: "progress".to_string(),
        message: "latest event message".to_string(),
        details: None,
        step_index: Some(0),
        todo_snapshot: Vec::new(),
    }];
    store.upsert_goal_run(&goal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-compaction-scope-fast"],
            )?;
            conn.execute(
                "UPDATE goal_run_events SET timestamp = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-compaction-scope-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let scope = store
        .goal_run_compaction_scope_ref("goal-compaction-scope-fast")
        .await?
        .expect("compaction scope ref should exist");

    assert_eq!(scope.id, "goal-compaction-scope-fast");
    assert_eq!(
        scope.active_task_id.as_deref(),
        Some("task-compaction-active")
    );
    assert_eq!(scope.title, "Compaction goal");
    assert_eq!(scope.goal, "Preserve compacted context");
    assert_eq!(scope.status, GoalRunStatus::Running);
    assert_eq!(
        scope.root_thread_id.as_deref(),
        Some("thread-compaction-root")
    );
    assert_eq!(
        scope.active_thread_id.as_deref(),
        Some("thread-compaction-active")
    );
    assert_eq!(
        scope.execution_thread_ids,
        vec!["thread-compaction-root", "thread-compaction-active"]
    );
    assert_eq!(
        scope.current_step_title.as_deref(),
        Some("Inspect compacted state")
    );
    assert_eq!(
        scope.current_step_status,
        Some(GoalRunStepStatus::InProgress)
    );
    assert_eq!(
        scope.current_step_summary.as_deref(),
        Some("Current inspection summary")
    );
    assert_eq!(
        scope.plan_summary.as_deref(),
        Some("Compaction plan summary")
    );
    assert_eq!(scope.latest_error.as_deref(), Some("latest goal error"));
    assert_eq!(scope.recent_events, vec!["latest event message"]);
    assert!(store
        .goal_run_compaction_scope_ref("goal-compaction-scope-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_thread_id_selects_primary_thread_without_hydrating_steps() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut goal = sample_goal_run_record("goal-thread-id-fast", 10);
    goal.thread_id = Some("thread-attention-target".to_string());
    store.upsert_goal_run(&goal).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-thread-id-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store.goal_run_thread_id("goal-thread-id-fast").await?,
        Some("thread-attention-target".to_string())
    );
    assert_eq!(
        store.goal_run_thread_id("goal-thread-id-missing").await?,
        None
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_goal_run_status_reply_ref_selects_reply_fields_without_hydrating_steps(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut older = sample_goal_run_record("goal-status-reply-older", 10);
    older.thread_id = Some("thread-status-reply".to_string());
    older.title = "Older goal".to_string();
    let mut newest = sample_goal_run_record("goal-status-reply-newest", 40);
    newest.thread_id = Some("thread-other".to_string());
    newest.execution_thread_ids = vec!["thread-status-reply".to_string()];
    newest.title = "Newest gateway reply goal".to_string();
    newest.status = GoalRunStatus::AwaitingApproval;
    newest.current_step_title = None;
    newest.plan_summary = Some("Awaiting approval for the next step".to_string());

    store.upsert_goal_run(&older).await?;
    store.upsert_goal_run(&newest).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-status-reply-newest"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let goal_run = store
        .latest_goal_run_status_reply_ref_for_thread_ids(&["thread-status-reply".to_string()])
        .await?
        .expect("status reply ref should exist");

    assert_eq!(goal_run.id, "goal-status-reply-newest");
    assert_eq!(goal_run.title, "Newest gateway reply goal");
    assert_eq!(goal_run.status, GoalRunStatus::AwaitingApproval);
    assert_eq!(goal_run.updated_at, 40);
    assert_eq!(goal_run.current_step_title, None);
    assert_eq!(
        goal_run.plan_summary.as_deref(),
        Some("Awaiting approval for the next step")
    );
    assert!(store
        .latest_goal_run_status_reply_ref_for_thread_ids(&["thread-missing".to_string()])
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_status_refs_for_statuses_selects_ids_and_status_without_hydrating_steps(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_goal_run_record("goal-status-ref-running", 20);
    running.status = GoalRunStatus::Running;
    let mut planning = sample_goal_run_record("goal-status-ref-planning", 40);
    planning.status = GoalRunStatus::Planning;
    let mut completed = sample_goal_run_record("goal-status-ref-completed", 60);
    completed.status = GoalRunStatus::Completed;

    store.upsert_goal_run(&running).await?;
    store.upsert_goal_run(&planning).await?;
    store.upsert_goal_run(&completed).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-status-ref-planning"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_goal_run_status_refs_for_statuses(&[GoalRunStatus::Running, GoalRunStatus::Planning])
        .await?;

    assert_eq!(
        refs,
        vec![
            (
                "goal-status-ref-planning".to_string(),
                GoalRunStatus::Planning
            ),
            (
                "goal-status-ref-running".to_string(),
                GoalRunStatus::Running
            ),
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_goal_refs_for_statuses_selects_id_and_goal_without_hydrating_steps(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_goal_run_record("goal-text-ref-running", 20);
    running.status = GoalRunStatus::Running;
    running.goal = "Keep the running goal moving".to_string();
    let mut paused = sample_goal_run_record("goal-text-ref-paused", 40);
    paused.status = GoalRunStatus::Paused;
    paused.goal = "Paused goal text".to_string();

    store.upsert_goal_run(&running).await?;
    store.upsert_goal_run(&paused).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-text-ref-running"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_goal_run_goal_refs_for_statuses(&[GoalRunStatus::Running])
        .await?;

    assert_eq!(
        refs,
        vec![(
            "goal-text-ref-running".to_string(),
            "Keep the running goal moving".to_string()
        )]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_goal_run_id_for_thread_ids_selects_newest_match_without_hydrating_steps(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut older_direct = sample_goal_run_record("goal-thread-direct", 10);
    older_direct.thread_id = Some("thread-status".to_string());
    let mut newest_execution = sample_goal_run_record("goal-thread-execution", 40);
    newest_execution.thread_id = Some("thread-other".to_string());
    newest_execution.execution_thread_ids = vec!["thread-status".to_string()];
    let mut unrelated = sample_goal_run_record("goal-thread-unrelated", 50);
    unrelated.thread_id = Some("thread-unrelated".to_string());

    store.upsert_goal_run(&older_direct).await?;
    store.upsert_goal_run(&newest_execution).await?;
    store.upsert_goal_run(&unrelated).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-thread-execution"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let goal_run_id = store
        .latest_goal_run_id_for_thread_ids(&["thread-status".to_string()])
        .await?;

    assert_eq!(goal_run_id.as_deref(), Some("goal-thread-execution"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_goal_run_id_for_thread_ids_and_statuses_filters_status_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_goal_run_record("goal-thread-running", 20);
    running.thread_id = Some("thread-cost".to_string());
    running.status = GoalRunStatus::Running;
    let mut paused_newer = sample_goal_run_record("goal-thread-paused", 50);
    paused_newer.thread_id = Some("thread-cost".to_string());
    paused_newer.status = GoalRunStatus::Paused;
    let mut planning_execution = sample_goal_run_record("goal-thread-planning", 40);
    planning_execution.thread_id = Some("thread-other".to_string());
    planning_execution.execution_thread_ids = vec!["thread-cost".to_string()];
    planning_execution.status = GoalRunStatus::Planning;

    store.upsert_goal_run(&running).await?;
    store.upsert_goal_run(&paused_newer).await?;
    store.upsert_goal_run(&planning_execution).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-thread-planning"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let goal_run_id = store
        .latest_goal_run_id_for_thread_ids_and_statuses(
            &["thread-cost".to_string()],
            &[GoalRunStatus::Running, GoalRunStatus::Planning],
        )
        .await?;

    assert_eq!(goal_run_id.as_deref(), Some("goal-thread-planning"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_thread_refs_for_thread_ids_selects_thread_metadata_without_hydrating_steps(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut direct = sample_goal_run_record("goal-thread-ref-direct", 20);
    direct.thread_id = Some("thread-ref-scan".to_string());
    direct.status = GoalRunStatus::Running;
    let mut execution = sample_goal_run_record("goal-thread-ref-execution", 40);
    execution.thread_id = Some("thread-other".to_string());
    execution.execution_thread_ids = vec!["thread-ref-scan".to_string()];
    execution.status = GoalRunStatus::Completed;
    let mut unrelated = sample_goal_run_record("goal-thread-ref-unrelated", 60);
    unrelated.thread_id = Some("thread-unrelated".to_string());

    store.upsert_goal_run(&direct).await?;
    store.upsert_goal_run(&execution).await?;
    store.upsert_goal_run(&unrelated).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id IN (?1, ?2)",
                params!["goal-thread-ref-direct", "goal-thread-ref-execution"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_goal_run_thread_refs_for_thread_ids(&["thread-ref-scan".to_string()])
        .await?;

    assert_eq!(
        refs.iter()
            .map(|entry| (entry.id.as_str(), entry.status, entry.updated_at))
            .collect::<Vec<_>>(),
        vec![
            ("goal-thread-ref-execution", GoalRunStatus::Completed, 40),
            ("goal-thread-ref-direct", GoalRunStatus::Running, 20),
        ]
    );
    assert_eq!(refs[0].execution_thread_ids, vec!["thread-ref-scan"]);
    assert_eq!(refs[1].thread_id.as_deref(), Some("thread-ref-scan"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_thread_refs_for_thread_ids_chunks_large_in_lists() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut direct = sample_goal_run_record("goal-thread-ref-chunk", 20);
    direct.thread_id = Some("thread-ref-chunk-hit".to_string());
    direct.status = GoalRunStatus::Running;
    store.upsert_goal_run(&direct).await?;

    let mut thread_ids = (0..300)
        .map(|index| format!("thread-ref-chunk-miss-{index}"))
        .collect::<Vec<_>>();
    thread_ids.push("thread-ref-chunk-hit".to_string());

    let refs = store
        .list_goal_run_thread_refs_for_thread_ids(&thread_ids)
        .await?;

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, "goal-thread-ref-chunk");
    assert_eq!(refs[0].thread_id.as_deref(), Some("thread-ref-chunk-hit"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_thread_refs_for_statuses_selects_thread_metadata_without_hydrating_steps(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut running = sample_goal_run_record("goal-status-thread-ref-running", 20);
    running.status = GoalRunStatus::Running;
    running.thread_id = Some("thread-status-ref-running".to_string());
    let mut paused = sample_goal_run_record("goal-status-thread-ref-paused", 40);
    paused.status = GoalRunStatus::Paused;
    paused.thread_id = Some("thread-status-ref-paused".to_string());
    let mut completed = sample_goal_run_record("goal-status-thread-ref-completed", 60);
    completed.status = GoalRunStatus::Completed;

    store.upsert_goal_run(&running).await?;
    store.upsert_goal_run(&paused).await?;
    store.upsert_goal_run(&completed).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET ordinal = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-status-thread-ref-paused"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let refs = store
        .list_goal_run_thread_refs_for_statuses(&[GoalRunStatus::Running, GoalRunStatus::Paused])
        .await?;

    assert_eq!(
        refs.iter()
            .map(|entry| (entry.id.as_str(), entry.status, entry.thread_id.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            (
                "goal-status-thread-ref-paused",
                GoalRunStatus::Paused,
                Some("thread-status-ref-paused"),
            ),
            (
                "goal-status-thread-ref-running",
                GoalRunStatus::Running,
                Some("thread-status-ref-running"),
            ),
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_run_ids_page_applies_order_limit_and_offset_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .upsert_goal_run(&sample_goal_run_record("goal-oldest", 10))
        .await?;
    store
        .upsert_goal_run(&sample_goal_run_record("goal-middle", 20))
        .await?;
    store
        .upsert_goal_run(&sample_goal_run_record("goal-newest", 30))
        .await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_runs SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["goal-oldest"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let (ids, total) = store.list_goal_run_ids_page(1, 1).await?;

    assert_eq!(total, 3);
    assert_eq!(ids, vec!["goal-middle"]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_goal_runs_page_fetches_goal_rows_with_page_bounds_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .upsert_goal_run(&sample_goal_run_record("goal-oldest", 10))
        .await?;
    store
        .upsert_goal_run(&sample_goal_run_record("goal-middle", 20))
        .await?;
    store
        .upsert_goal_run(&sample_goal_run_record("goal-newest", 30))
        .await?;

    let (goal_runs, total) = store.list_goal_runs_page(1, 1).await?;

    assert_eq!(total, 3);
    assert_eq!(
        goal_runs
            .iter()
            .map(|goal_run| goal_run.id.as_str())
            .collect::<Vec<_>>(),
        vec!["goal-middle"],
    );
    assert_eq!(
        goal_runs[0]
            .steps
            .iter()
            .map(|step| step.id.as_str())
            .collect::<Vec<_>>(),
        vec!["step-goal-middle"],
        "paged goal run should include only its own persisted steps"
    );
    assert_eq!(
        goal_runs[0]
            .events
            .iter()
            .map(|event| event.id.as_str())
            .collect::<Vec<_>>(),
        vec!["event-goal-middle"],
        "paged goal run should include only its own persisted events"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn count_goal_runs_counts_non_deleted_rows_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .upsert_goal_run(&sample_goal_run_record("goal-visible-a", 10))
        .await?;
    store
        .upsert_goal_run(&sample_goal_run_record("goal-visible-b", 20))
        .await?;
    store
        .upsert_goal_run(&sample_goal_run_record("goal-deleted", 30))
        .await?;
    store.delete_goal_run("goal-deleted").await?;

    assert_eq!(store.count_goal_runs().await?, 2);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_goal_run_for_thread_filters_thread_and_orders_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut older = sample_goal_run_record("goal-thread-older", 20);
    older.thread_id = Some("thread-goal-sql".to_string());
    let mut newest = sample_goal_run_record("goal-thread-newest", 30);
    newest.thread_id = Some("thread-goal-sql".to_string());
    newest.session_id = Some("session-goal-newest".to_string());
    let mut unrelated = sample_goal_run_record("goal-thread-other", 40);
    unrelated.thread_id = Some("thread-other".to_string());

    store.upsert_goal_run(&older).await?;
    store.upsert_goal_run(&newest).await?;
    store.upsert_goal_run(&unrelated).await?;

    let latest = store
        .latest_goal_run_for_thread("thread-goal-sql")
        .await?
        .expect("latest goal run should exist");
    assert_eq!(latest.id, "goal-thread-newest");
    assert_eq!(latest.session_id.as_deref(), Some("session-goal-newest"));
    assert!(store
        .latest_goal_run_for_thread("thread-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_goal_run_repo_context_for_thread_selects_metadata_without_hydrating_steps(
) -> Result<()> {
    let (store, root) = make_test_store().await?;

    let mut older = sample_goal_run_record("goal-repo-context-older", 10);
    older.thread_id = Some("thread-repo-context-sql".to_string());
    older.session_id = Some("session-older".to_string());
    older.current_step_index = 0;
    let mut newest = sample_goal_run_record("goal-repo-context-newest", 30);
    newest.thread_id = Some("thread-repo-context-sql".to_string());
    newest.session_id = Some("session-newest".to_string());
    newest.current_step_index = 2;
    let mut unrelated = sample_goal_run_record("goal-repo-context-other", 40);
    unrelated.thread_id = Some("thread-other".to_string());
    unrelated.session_id = Some("session-other".to_string());

    store.upsert_goal_run(&older).await?;
    store.upsert_goal_run(&newest).await?;
    store.upsert_goal_run(&unrelated).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                params!["goal-repo-context-newest"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let context = store
        .latest_goal_run_repo_context_for_thread("thread-repo-context-sql")
        .await?
        .expect("repo context should exist");

    assert_eq!(context.id, "goal-repo-context-newest");
    assert_eq!(context.session_id.as_deref(), Some("session-newest"));
    assert_eq!(context.current_step_index, 2);
    assert!(store
        .latest_goal_run_repo_context_for_thread("thread-missing")
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_extended_metadata_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let goal_run = GoalRun {
        id: "goal-meta".to_string(),
        title: "Goal metadata".to_string(),
        goal: "Verify all spec metadata persists".to_string(),
        client_request_id: Some("client-1".to_string()),
        status: GoalRunStatus::AwaitingApproval,
        priority: TaskPriority::High,
        created_at: 10,
        updated_at: 20,
        started_at: Some(11),
        completed_at: Some(99),
        thread_id: Some("thread-2".to_string()),
        session_id: Some("session-2".to_string()),
        current_step_index: 0,
        current_step_title: Some("Execute safely".to_string()),
        current_step_kind: Some(GoalRunStepKind::Command),
        planner_owner_profile: Some(GoalRuntimeOwnerProfile {
            agent_label: "Swarog".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            reasoning_effort: Some("high".to_string()),
        }),
        current_step_owner_profile: Some(GoalRuntimeOwnerProfile {
            agent_label: "Android Verifier".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            reasoning_effort: None,
        }),
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 1,
        max_replans: 3,
        plan_summary: Some("Plan summary".to_string()),
        reflection_summary: Some("Reflection summary".to_string()),
        memory_updates: vec!["remember this".to_string()],
        generated_skill_path: Some("skills/generated/goal-meta.md".to_string()),
        last_error: Some("waiting for approval".to_string()),
        failure_cause: Some("policy gate".to_string()),
        step_failure_history: vec![
            "step-meta attempt 1: build output is incomplete".to_string(),
            "step-meta attempt 2: tests still failing".to_string(),
        ],
        stopped_reason: Some("operator requested stop".to_string()),
        child_task_ids: vec!["task-1".to_string(), "task-2".to_string()],
        child_task_count: 2,
        approval_count: 1,
        awaiting_approval_id: Some("approval-1".to_string()),
        policy_fingerprint: Some("fingerprint-1".to_string()),
        approval_expires_at: Some(12345),
        containment_scope: Some("workspace".to_string()),
        compensation_status: Some("required".to_string()),
        compensation_summary: Some("rollback pending".to_string()),
        active_task_id: Some("task-2".to_string()),
        pending_review_report: None,
        duration_ms: Some(888),
        launch_assignment_snapshot: vec![
            sample_assignment("planner", "openai", "gpt-5.4", Some("high")),
            sample_assignment("executor", "anthropic", "claude-sonnet-4", None),
        ],
        runtime_assignment_list: vec![sample_assignment(
            "executor",
            "anthropic",
            "claude-sonnet-4",
            None,
        )],
        root_thread_id: Some("thread-root".to_string()),
        supervision_thread_id: Some("thread-supervision".to_string()),
        active_thread_id: Some("thread-active".to_string()),
        execution_thread_ids: vec![
            "thread-root".to_string(),
            "thread-active".to_string(),
            "thread-followup".to_string(),
        ],
        steps: vec![GoalRunStep {
            id: "step-meta".to_string(),
            position: 0,
            title: "Execute safely".to_string(),
            instructions: "Run guarded command".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "Command finished".to_string(),
            session_id: Some("session-2".to_string()),
            status: GoalRunStepStatus::InProgress,
            task_id: Some("task-2".to_string()),
            summary: Some("waiting".to_string()),
            error: None,
            started_at: Some(11),
            completed_at: None,
        }],
        events: vec![],
        dossier: Some(GoalRunDossier {
            units: vec![GoalDeliveryUnit {
                id: "unit-1".to_string(),
                title: "Implement guarded command".to_string(),
                status: GoalProjectionState::Completed,
                execution_binding: GoalRoleBinding::Builtin("swarog".to_string()),
                verification_binding: GoalRoleBinding::Subagent("android-verifier".to_string()),
                ..Default::default()
            }],
            latest_resume_decision: Some(GoalResumeDecision {
                action: GoalResumeAction::Stop,
                reason_code: "operator_stop".to_string(),
                projection_state: GoalProjectionState::Completed,
                ..Default::default()
            }),
            summary: Some("One unit completed and verified".to_string()),
            ..Default::default()
        }),
        total_prompt_tokens: 123,
        total_completion_tokens: 456,
        estimated_cost_usd: Some(0.42),
        model_usage: vec![GoalRunModelUsage {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            request_count: 2,
            prompt_tokens: 123,
            completion_tokens: 456,
            estimated_cost_usd: Some(0.42),
            duration_ms: Some(777),
        }],
        autonomy_level: crate::agent::AutonomyLevel::Supervised,
        authorship_tag: Some(crate::agent::AuthorshipTag::Joint),
    };

    store.upsert_goal_run(&goal_run).await?;
    let loaded = store
        .get_goal_run("goal-meta")
        .await?
        .expect("goal run should exist after upsert");

    assert_eq!(loaded.completed_at, Some(99));
    assert_eq!(loaded.current_step_title.as_deref(), Some("Execute safely"));
    assert_eq!(loaded.current_step_kind, Some(GoalRunStepKind::Command));
    assert_eq!(loaded.failure_cause.as_deref(), Some("policy gate"));
    assert_eq!(loaded.child_task_ids, vec!["task-1", "task-2"]);
    assert_eq!(loaded.child_task_count, 2);
    assert_eq!(loaded.approval_count, 1);
    assert_eq!(loaded.awaiting_approval_id.as_deref(), Some("approval-1"));
    assert_eq!(loaded.policy_fingerprint.as_deref(), Some("fingerprint-1"));
    assert_eq!(loaded.approval_expires_at, Some(12345));
    assert_eq!(loaded.containment_scope.as_deref(), Some("workspace"));
    assert_eq!(loaded.compensation_status.as_deref(), Some("required"));
    assert_eq!(
        loaded.compensation_summary.as_deref(),
        Some("rollback pending")
    );
    assert_eq!(loaded.active_task_id.as_deref(), Some("task-2"));
    assert_eq!(loaded.duration_ms, Some(888));
    assert_eq!(
        loaded.stopped_reason.as_deref(),
        Some("operator requested stop")
    );
    let dossier = loaded.dossier.expect("dossier should round-trip");
    assert_eq!(dossier.units.len(), 1);
    assert_eq!(dossier.units[0].id, "unit-1");
    assert_eq!(
        dossier.units[0].verification_binding,
        GoalRoleBinding::Subagent("android-verifier".to_string())
    );
    assert_eq!(
        dossier
            .latest_resume_decision
            .expect("resume decision should round-trip")
            .reason_code,
        "operator_stop"
    );
    assert_eq!(loaded.total_prompt_tokens, 123);
    assert_eq!(loaded.total_completion_tokens, 456);
    assert_eq!(loaded.estimated_cost_usd, Some(0.42));
    assert_eq!(loaded.model_usage.len(), 1);
    assert_eq!(loaded.model_usage[0].provider, "openai");
    assert_eq!(loaded.model_usage[0].model, "gpt-4o");
    assert_eq!(loaded.model_usage[0].request_count, 2);
    assert_eq!(loaded.model_usage[0].prompt_tokens, 123);
    assert_eq!(loaded.model_usage[0].completion_tokens, 456);
    assert_eq!(loaded.model_usage[0].duration_ms, Some(777));
    assert_eq!(
        loaded.autonomy_level,
        crate::agent::AutonomyLevel::Supervised
    );
    assert_eq!(
        loaded.authorship_tag,
        Some(crate::agent::AuthorshipTag::Joint)
    );
    assert_eq!(
        loaded.planner_owner_profile,
        Some(GoalRuntimeOwnerProfile {
            agent_label: "Swarog".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            reasoning_effort: Some("high".to_string()),
        })
    );
    assert_eq!(
        loaded.current_step_owner_profile,
        Some(GoalRuntimeOwnerProfile {
            agent_label: "Android Verifier".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            reasoning_effort: None,
        })
    );
    assert_eq!(
        loaded.launch_assignment_snapshot,
        vec![
            sample_assignment("planner", "openai", "gpt-5.4", Some("high")),
            sample_assignment("executor", "anthropic", "claude-sonnet-4", None),
        ]
    );
    assert_eq!(
        loaded.runtime_assignment_list,
        vec![sample_assignment(
            "executor",
            "anthropic",
            "claude-sonnet-4",
            None,
        )]
    );
    assert_eq!(loaded.root_thread_id.as_deref(), Some("thread-root"));
    assert_eq!(
        loaded.supervision_thread_id.as_deref(),
        Some("thread-supervision")
    );
    assert_eq!(loaded.active_thread_id.as_deref(), Some("thread-active"));
    assert_eq!(
        loaded.execution_thread_ids,
        vec![
            "thread-root".to_string(),
            "thread-active".to_string(),
            "thread-followup".to_string(),
        ]
    );
    assert_eq!(
        loaded.step_failure_history,
        vec![
            "step-meta attempt 1: build output is incomplete".to_string(),
            "step-meta attempt 2: tests still failing".to_string(),
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_step_failure_history_survives_reload() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mut goal_run = sample_goal_run_record("goal-retry-history", 40);
    goal_run.step_failure_history = vec![
        "step-inspect attempt 1: missing cargo target".to_string(),
        "step-inspect attempt 2: retry still incomplete".to_string(),
    ];
    store.upsert_goal_run(&goal_run).await?;

    let loaded = store
        .get_goal_run("goal-retry-history")
        .await?
        .expect("goal run should reload");
    assert_eq!(loaded.step_failure_history, goal_run.step_failure_history);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn init_schema_migrates_legacy_goal_runs_metadata_columns() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store
        .conn
        .call(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS goal_runs")?;
            conn.execute_batch(
                "
            CREATE TABLE goal_runs (
                id                  TEXT PRIMARY KEY,
                title               TEXT NOT NULL,
                goal                TEXT NOT NULL,
                client_request_id   TEXT,
                status              TEXT NOT NULL,
                priority            TEXT NOT NULL,
                created_at          INTEGER NOT NULL,
                updated_at          INTEGER NOT NULL,
                started_at          INTEGER,
                completed_at        INTEGER,
                thread_id           TEXT,
                session_id          TEXT,
                current_step_index  INTEGER NOT NULL DEFAULT 0,
                replan_count        INTEGER NOT NULL DEFAULT 0,
                max_replans         INTEGER NOT NULL DEFAULT 2,
                plan_summary        TEXT,
                reflection_summary  TEXT,
                memory_updates_json TEXT NOT NULL DEFAULT '[]',
                generated_skill_path TEXT,
                last_error          TEXT,
                child_task_ids_json TEXT NOT NULL DEFAULT '[]'
            );
            CREATE INDEX IF NOT EXISTS idx_goal_runs_status ON goal_runs(status, updated_at DESC);
            ",
            )?;
            conn.execute(
                "INSERT INTO goal_runs (
                    id,
                    title,
                    goal,
                    client_request_id,
                    status,
                    priority,
                    created_at,
                    updated_at,
                    started_at,
                    completed_at,
                    thread_id,
                    session_id,
                    current_step_index,
                    replan_count,
                    max_replans,
                    plan_summary,
                    reflection_summary,
                    memory_updates_json,
                    generated_skill_path,
                    last_error,
                    child_task_ids_json
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
                )",
                rusqlite::params![
                    "goal-legacy",
                    "legacy title",
                    "legacy goal",
                    Option::<String>::None,
                    "running",
                    "normal",
                    10_i64,
                    20_i64,
                    Option::<i64>::None,
                    Option::<i64>::None,
                    Some("thread-legacy"),
                    Option::<String>::None,
                    0_i64,
                    0_i64,
                    2_i64,
                    Option::<String>::None,
                    Option::<String>::None,
                    "[]",
                    Option::<String>::None,
                    Option::<String>::None,
                    "[]",
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    store.init_schema().await?;

    let cols = store
        .conn
        .call(|conn| {
            Ok((
                table_has_column_sync(conn, "goal_runs", "failure_cause")?,
                table_has_column_sync(conn, "goal_runs", "stopped_reason")?,
                table_has_column_sync(conn, "goal_runs", "child_task_count")?,
                table_has_column_sync(conn, "goal_runs", "approval_count")?,
                table_has_column_sync(conn, "goal_runs", "awaiting_approval_id")?,
                table_has_column_sync(conn, "goal_runs", "policy_fingerprint")?,
                table_has_column_sync(conn, "goal_runs", "approval_expires_at")?,
                table_has_column_sync(conn, "goal_runs", "containment_scope")?,
                table_has_column_sync(conn, "goal_runs", "compensation_status")?,
                table_has_column_sync(conn, "goal_runs", "compensation_summary")?,
                table_has_column_sync(conn, "goal_runs", "active_task_id")?,
                table_has_column_sync(conn, "goal_runs", "duration_ms")?,
                table_has_column_sync(conn, "goal_runs", "dossier_json")?,
                table_has_column_sync(conn, "goal_runs", "total_prompt_tokens")?,
                table_has_column_sync(conn, "goal_runs", "total_completion_tokens")?,
                table_has_column_sync(conn, "goal_runs", "estimated_cost_usd")?,
                table_has_column_sync(conn, "goal_runs", "model_usage_json")?,
                table_has_column_sync(conn, "goal_runs", "autonomy_level")?,
                table_has_column_sync(conn, "goal_runs", "authorship_tag")?,
                table_has_column_sync(conn, "goal_runs", "planner_owner_profile_json")?,
                table_has_column_sync(conn, "goal_runs", "current_step_owner_profile_json")?,
                table_has_column_sync(conn, "goal_runs", "launch_assignment_snapshot_json")?,
                table_has_column_sync(conn, "goal_runs", "runtime_assignment_list_json")?,
                table_has_column_sync(conn, "goal_runs", "root_thread_id")?,
                table_has_column_sync(conn, "goal_runs", "active_thread_id")?,
                table_has_column_sync(conn, "goal_runs", "execution_thread_ids_json")?,
                table_has_column_sync(conn, "goal_runs", "step_failure_history_json")?,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(cols.0);
    assert!(cols.1);
    assert!(cols.2);
    assert!(cols.3);
    assert!(cols.4);
    assert!(cols.5);
    assert!(cols.6);
    assert!(cols.7);
    assert!(cols.8);
    assert!(cols.9);
    assert!(cols.10);
    assert!(cols.11);
    assert!(cols.12);
    assert!(cols.13);
    assert!(cols.14);
    assert!(cols.15);
    assert!(cols.16);
    assert!(cols.17);
    assert!(cols.18);
    assert!(cols.19);
    assert!(cols.20);
    assert!(cols.21);
    assert!(cols.22);
    assert!(cols.23);
    assert!(cols.24);
    assert!(cols.25);
    assert!(cols.26);

    let legacy_goal = store
        .list_goal_runs()
        .await?
        .into_iter()
        .find(|goal_run| goal_run.id == "goal-legacy")
        .expect("legacy goal should remain readable after migration");
    assert_eq!(legacy_goal.root_thread_id.as_deref(), Some("thread-legacy"));
    assert_eq!(
        legacy_goal.active_thread_id.as_deref(),
        Some("thread-legacy")
    );
    assert_eq!(
        legacy_goal.execution_thread_ids,
        vec!["thread-legacy".to_string()]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn memory_provenance_write_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    let fact_keys = vec!["shell".to_string(), "editor".to_string()];
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "mem-1",
            target: "USER.md",
            mode: "append",
            source_kind: "tool",
            content: "- shell: bash",
            fact_keys: &fact_keys,
            thread_id: Some("thread-1"),
            task_id: Some("task-1"),
            goal_run_id: None,
            created_at: 42,
            sign: true,
        })
        .await?;

    let row = store.conn.call(|conn| {
        conn.query_row(
            "SELECT target, mode, source_kind, content, fact_keys_json, entry_hash, signature_scheme FROM memory_provenance WHERE id = 'mem-1'",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?, row.get::<_, Option<String>>(6)?)),
        ).map_err(Into::into)
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(row.0, "USER.md");
    assert_eq!(row.1, "append");
    assert_eq!(row.2, "tool");
    assert_eq!(row.3, "- shell: bash");
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&row.4)?,
        vec!["shell".to_string(), "editor".to_string()]
    );
    assert!(!row.5.is_empty());
    assert_eq!(row.6.as_deref(), Some("ed25519"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_memory_provenance_created_at_by_fact_keys_filters_keys_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let alpha = vec!["alpha".to_string()];
    let beta = vec!["beta".to_string()];
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "mem-alpha-old",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "test",
            content: "old alpha",
            fact_keys: &alpha,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 100,
            sign: false,
        })
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "mem-beta",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "test",
            content: "beta",
            fact_keys: &beta,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 200,
            sign: false,
        })
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "mem-alpha-new",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "test",
            content: "new alpha",
            fact_keys: &alpha,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 300,
            sign: false,
        })
        .await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "INSERT INTO memory_provenance \
                 (id, target, mode, source_kind, content, fact_keys_json, created_at, entry_hash, signature, signature_scheme) \
                 VALUES ('malformed-facts', 'MEMORY.md', 'append', 'test', 'bad', 'not-json', 999, '', NULL, NULL)",
                [],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let keys = vec![
        "alpha".to_string(),
        "beta".to_string(),
        "missing".to_string(),
    ];
    let timestamps = store
        .latest_memory_provenance_created_at_by_fact_keys("MEMORY.md", &keys)
        .await?;

    assert_eq!(timestamps.get("alpha"), Some(&300));
    assert_eq!(timestamps.get("beta"), Some(&200));
    assert!(!timestamps.contains_key("missing"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn active_memory_provenance_conventions_filter_status_and_tokens_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let rust_keys = vec!["rust".to_string()];
    let cargo_keys = vec!["cargo".to_string()];
    let docs_keys = vec!["docs".to_string()];
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "active-rust-old",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "test",
            content: "rust convention old",
            fact_keys: &rust_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 100,
            sign: false,
        })
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "active-rust-new",
            target: "USER.md",
            mode: "append",
            source_kind: "test",
            content: "newer rust convention",
            fact_keys: &rust_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 200,
            sign: false,
        })
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "removed-rust-newest",
            target: "MEMORY.md",
            mode: "remove",
            source_kind: "test",
            content: "removed rust convention",
            fact_keys: &rust_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 500,
            sign: false,
        })
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "retracted-rust-newer",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "test",
            content: "retracted rust convention",
            fact_keys: &rust_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 400,
            sign: false,
        })
        .await?;
    store
        .retract_memory_provenance_entry("retracted-rust-newer", 450)
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "active-cargo-newest",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "test",
            content: "cargo convention",
            fact_keys: &cargo_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 600,
            sign: false,
        })
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "project-docs",
            target: "PROJECT.md",
            mode: "append",
            source_kind: "test",
            content: "docs convention",
            fact_keys: &docs_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 700,
            sign: false,
        })
        .await?;

    let entries = store
        .list_active_memory_provenance_conventions(&["rust".to_string()], 2)
        .await?;
    let ids = entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["active-rust-new", "active-rust-old"]);

    let default_entries = store
        .list_active_memory_provenance_conventions(&[], 10)
        .await?;
    assert!(default_entries
        .iter()
        .all(|entry| matches!(entry.target.as_str(), "MEMORY.md" | "USER.md")));
    assert!(!default_entries
        .iter()
        .any(|entry| entry.id == "project-docs"));

    let exact_target_entries = store
        .list_active_memory_provenance_for_target("MEMORY.md", 10)
        .await?;
    assert!(!exact_target_entries
        .iter()
        .any(|entry| entry.id == "removed-rust-newest" || entry.id == "retracted-rust-newer"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn memory_provenance_report_marks_old_entries_uncertain() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let recent_keys = vec!["shell".to_string()];
    let old_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "recent",
            target: "USER.md",
            mode: "append",
            source_kind: "tool",
            content: "- shell: bash",
            fact_keys: &recent_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
            sign: true,
        })
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "old",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &old_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(40 * 86_400_000),
            sign: true,
        })
        .await?;

    let report = store.memory_provenance_report(None, 10).await?;
    assert_eq!(report.total_entries, 2);
    assert_eq!(report.summary_by_status.get("active").copied(), Some(1));
    assert_eq!(report.summary_by_status.get("uncertain").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn confirm_uncertain_memory_provenance_updates_status() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let old_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "old-confirmable",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &old_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(40 * 86_400_000),
            sign: true,
        })
        .await?;

    let before = store.memory_provenance_report(None, 10).await?;
    assert_eq!(before.entries[0].status, "uncertain");
    assert!(before.entries[0].confidence < 0.55);

    let updated = store
        .confirm_memory_provenance_entry("old-confirmable", now_ms)
        .await?;
    assert!(updated, "expected confirmation update to touch one row");

    let after = store.memory_provenance_report(None, 10).await?;
    assert_eq!(after.entries[0].status, "confirmed");
    assert!(after.entries[0].confidence >= 0.95);
    assert_eq!(after.summary_by_status.get("confirmed").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn retract_memory_provenance_entry_updates_status() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let fact_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "retractable-memory-entry",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
            sign: true,
        })
        .await?;

    let before = store.memory_provenance_report(None, 10).await?;
    assert_eq!(before.entries[0].status, "active");

    let updated = store
        .retract_memory_provenance_entry("retractable-memory-entry", now_ms)
        .await?;
    assert!(updated, "expected retract update to touch one row");

    let after = store.memory_provenance_report(None, 10).await?;
    assert_eq!(after.entries[0].status, "retracted");
    assert_eq!(after.summary_by_status.get("retracted").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn remove_memory_provenance_records_retract_relationship() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let fact_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "memory-editor-active",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(1000),
            sign: true,
        })
        .await?;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "memory-editor-remove",
            target: "MEMORY.md",
            mode: "remove",
            source_kind: "operator_correction",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
            sign: true,
        })
        .await?;

    let report = store
        .memory_provenance_report(Some("MEMORY.md"), 10)
        .await?;
    let remove_entry = report
        .entries
        .iter()
        .find(|entry| entry.id == "memory-editor-remove")
        .expect("remove provenance entry should exist");

    assert_eq!(remove_entry.relationships.len(), 1);
    assert_eq!(remove_entry.relationships[0].relation_type, "retracts");
    assert_eq!(
        remove_entry.relationships[0].related_entry_id,
        "memory-editor-active"
    );
    assert_eq!(
        remove_entry.relationships[0].fact_key.as_deref(),
        Some("editor")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn confirm_memory_provenance_entry_rejects_tampered_signed_record() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let fact_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "tampered-memory-entry",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
            sign: true,
        })
        .await?;

    store.conn.call(|conn| {
        conn.execute(
            "UPDATE memory_provenance SET entry_hash = 'tampered' WHERE id = 'tampered-memory-entry'",
            [],
        )?;
        Ok(())
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    let error = store
        .confirm_memory_provenance_entry("tampered-memory-entry", now_ms.saturating_add(1))
        .await
        .expect_err("tampered signed record should be rejected");
    assert!(error.to_string().contains("integrity validation"));

    let report = store
        .memory_provenance_report(Some("MEMORY.md"), 10)
        .await?;
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.id == "tampered-memory-entry")
        .expect("tampered memory entry should be visible in report");
    assert_eq!(entry.status, "invalid");
    assert!(!entry.hash_valid);
    assert!(!entry.signature_valid);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn collaboration_session_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    store
        .upsert_collaboration_session(
            "task-parent",
            r#"{"id":"c1","parent_task_id":"task-parent"}"#,
            42,
        )
        .await?;

    let rows = store.list_collaboration_sessions().await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].parent_task_id, "task-parent");
    assert_eq!(rows[0].updated_at, 42);
    assert!(rows[0].session_json.contains("\"id\":\"c1\""));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn get_collaboration_session_filters_parent_task_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    store
        .upsert_collaboration_session(
            "task-other",
            r#"{"id":"other","parent_task_id":"task-other"}"#,
            100,
        )
        .await?;
    store
        .upsert_collaboration_session(
            "task-parent",
            r#"{"id":"c1","parent_task_id":"task-parent"}"#,
            42,
        )
        .await?;

    let row = store
        .get_collaboration_session("task-parent")
        .await?
        .expect("parent task session should be returned");

    assert_eq!(row.parent_task_id, "task-parent");
    assert_eq!(row.updated_at, 42);
    assert!(row.session_json.contains("\"id\":\"c1\""));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provenance_report_validates_hash_and_signature() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let first = serde_json::json!({"step": 1});
    let second = serde_json::json!({"step": 2});
    store
        .record_provenance_event(&ProvenanceEventRecord {
            event_type: "goal_created",
            summary: "goal created",
            details: &first,
            agent_id: "test-agent",
            goal_run_id: Some("goal-1"),
            task_id: None,
            thread_id: Some("thread-1"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "soc2",
            sign: true,
            created_at: 1_000,
        })
        .await?;
    store
        .record_provenance_event(&ProvenanceEventRecord {
            event_type: "step_completed",
            summary: "step completed",
            details: &second,
            agent_id: "test-agent",
            goal_run_id: Some("goal-1"),
            task_id: Some("task-1"),
            thread_id: Some("thread-1"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "soc2",
            sign: true,
            created_at: 2_000,
        })
        .await?;

    let entries = read_provenance_entries(&root.join("semantic-logs").join("provenance.jsonl"))?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].signature_scheme.as_deref(), Some("ed25519"));
    assert_eq!(entries[1].signature_scheme.as_deref(), Some("ed25519"));

    let report = store.provenance_report(10)?;
    assert_eq!(report.total_entries, 2);
    assert_eq!(report.signed_entries, 2);
    assert_eq!(report.valid_hash_entries, 2);
    assert_eq!(report.valid_chain_entries, 2);
    assert_eq!(report.valid_signature_entries, 2);
    assert!(report.entries.iter().all(|entry| entry.signature_present));
    assert!(report.entries.iter().all(|entry| entry.signature_valid));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provenance_report_keeps_legacy_signature_validation() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let legacy_key_path = root.join("provenance-signing.key");
    let legacy_key = "legacy-signing-key";
    fs::write(&legacy_key_path, legacy_key)?;

    let details = serde_json::json!({"legacy": true});
    let entry_hash = compute_provenance_hash(
        0,
        1_000,
        "legacy_event",
        "legacy provenance event",
        &details,
        "genesis",
        "legacy-agent",
        None,
        None,
        Some("thread-legacy"),
        None,
        None,
        "soc2",
    );
    let entry = ProvenanceLogEntry {
        sequence: 0,
        timestamp: 1_000,
        event_type: "legacy_event".to_string(),
        summary: "legacy provenance event".to_string(),
        details,
        prev_hash: "genesis".to_string(),
        entry_hash: entry_hash.clone(),
        signature: Some(sign_provenance_hash(legacy_key, &entry_hash)),
        signature_scheme: None,
        agent_id: "legacy-agent".to_string(),
        goal_run_id: None,
        task_id: None,
        thread_id: Some("thread-legacy".to_string()),
        approval_id: None,
        causal_trace_id: None,
        compliance_mode: "soc2".to_string(),
    };
    fs::write(
        root.join("semantic-logs").join("provenance.jsonl"),
        format!("{}\n", serde_json::to_string(&entry)?),
    )?;

    let report = store.provenance_report(10)?;
    assert_eq!(report.total_entries, 1);
    assert_eq!(report.signed_entries, 1);
    assert_eq!(report.valid_signature_entries, 1);
    assert_eq!(report.entries.len(), 1);
    assert!(report.entries[0].signature_present);
    assert!(report.entries[0].signature_valid);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provenance_report_does_not_mark_unsigned_entries_as_signature_valid() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    let details = serde_json::json!({"unsigned": true});
    store
        .record_provenance_event(&ProvenanceEventRecord {
            event_type: "step_completed",
            summary: "unsigned step completed",
            details: &details,
            agent_id: "test-agent",
            goal_run_id: Some("goal-unsigned"),
            task_id: Some("task-unsigned"),
            thread_id: Some("thread-unsigned"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "standard",
            sign: false,
            created_at: 3_000,
        })
        .await?;

    let entries = read_provenance_entries(&root.join("semantic-logs").join("provenance.jsonl"))?;
    assert_eq!(entries.len(), 1);
    assert!(entries[0].signature.is_none());
    assert!(entries[0].signature_scheme.is_none());

    let report = store.provenance_report(10)?;
    assert_eq!(report.total_entries, 1);
    assert_eq!(report.signed_entries, 0);
    assert_eq!(report.valid_hash_entries, 1);
    assert_eq!(report.valid_chain_entries, 1);
    assert_eq!(report.valid_signature_entries, 0);
    assert_eq!(report.entries.len(), 1);
    assert!(!report.entries[0].signature_present);
    assert!(!report.entries[0].signature_valid);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provenance_event_chains_onto_last_valid_entry_of_a_large_log() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let record = |summary: &'static str, details: &'static serde_json::Value, created_at: u64| {
        ProvenanceEventRecord {
            event_type: "tool_call",
            summary,
            details,
            agent_id: "test-agent",
            goal_run_id: None,
            task_id: None,
            thread_id: Some("thread-large-log"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "standard",
            sign: false,
            created_at,
        }
    };
    let small: &'static serde_json::Value =
        Box::leak(Box::new(serde_json::json!({"payload": "x".repeat(2_000)})));
    let oversized: &'static serde_json::Value =
        Box::leak(Box::new(serde_json::json!({"payload": "y".repeat(300_000)})));
    for index in 0..100u64 {
        store
            .record_provenance_event(&record("filler", small, 1_000 + index))
            .await?;
    }
    store
        .record_provenance_event(&record("oversized", oversized, 2_000))
        .await?;
    let log_path = root.join("semantic-logs").join("provenance.jsonl");
    let mut log = fs::OpenOptions::new().append(true).open(&log_path)?;
    std::io::Write::write_all(&mut log, b"{\"truncated\": \n\n\n")?;

    store
        .record_provenance_event(&record("after corruption", small, 3_000))
        .await?;

    let entries = read_provenance_entries(&log_path)?;
    let oversized_entry = &entries[entries.len() - 2];
    let latest = &entries[entries.len() - 1];
    assert_eq!(oversized_entry.summary, "oversized");
    assert_eq!(
        latest.sequence,
        oversized_entry.sequence + 1,
        "a new event must continue the sequence of the last valid entry, even when that entry is larger than the tail read window"
    );
    assert_eq!(
        latest.prev_hash, oversized_entry.entry_hash,
        "the hash chain must link to the last valid entry and skip a corrupt trailing line"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}
