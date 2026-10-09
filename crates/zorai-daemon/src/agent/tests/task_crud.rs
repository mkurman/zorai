use super::*;
use crate::agent::episodic::{EpisodeType, LinkType};
use crate::session_manager::SessionManager;
use bytes::BytesMut;
use tempfile::tempdir;
use tokio_util::codec::Encoder;

#[tokio::test]
async fn production_enqueue_activates_completion_contracts_for_runtime_cohorts() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut task_ids = Vec::new();
    for source in ["goal_run", "workspace_runtime", "subagent"] {
        let task = engine
            .enqueue_task(
                format!("{source} contract"),
                format!("Complete the {source} objective"),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                source,
                (source == "goal_run").then(|| "goal-contract".to_string()),
                None,
                Some("thread-contract".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        let contract = task
            .completion_contract
            .as_ref()
            .expect("new production tasks must have an explicit contract");
        assert_eq!(contract.objective, task.description);
        if source == "subagent" {
            assert_eq!(contract.open_completion_reasons().len(), 1);
            assert!(
                contract.open_completion_reasons()[0].contains("usable subagent outcome report")
            );
        } else {
            assert!(contract.open_completion_reasons().is_empty());
        }
        task_ids.push(task.id.clone());
    }

    engine.tasks.lock().await.clear();
    let persisted = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
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
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: task_ids,
            parent_task_ids: Vec::new(),
        })
        .await;
    assert_eq!(persisted.len(), 3);
    assert!(persisted
        .iter()
        .all(|task| task.completion_contract.is_some()));
}

#[tokio::test]
async fn compaction_scope_snapshot_resolves_persisted_task_by_id() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let task = engine
        .enqueue_task(
            "Compact this task".to_string(),
            "Preserve task context while compacting".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-compaction-scope".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .history
        .conn
        .call({
            let task_id = task.id.clone();
            move |conn| {
                conn.execute(
                    "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                    rusqlite::params![task_id],
                )?;
                Ok(())
            }
        })
        .await
        .expect("corrupt unrelated hydration column");
    engine.tasks.lock().await.clear();

    let scope = engine
        .compaction_scope_snapshot("thread-compaction-scope", Some(&task.id))
        .await
        .expect("persisted task should produce a compaction scope");

    assert_eq!(scope.task_id.as_deref(), Some(task.id.as_str()));
    assert_eq!(scope.active_task_id.as_deref(), Some(task.id.as_str()));
    assert_eq!(scope.thread_id, "thread-compaction-scope");
}

#[tokio::test]
async fn compaction_scope_snapshot_resolves_persisted_goal_without_hydrating_goal() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut goal = sample_supervised_goal_run(
        "goal-compaction-scope-fast",
        "task-compaction-active",
        "approval-compaction",
    );
    goal.title = "Compaction goal".to_string();
    goal.goal = "Preserve persisted goal context".to_string();
    goal.thread_id = Some("thread-compaction-root".to_string());
    goal.root_thread_id = Some("thread-compaction-root".to_string());
    goal.active_thread_id = Some("thread-compaction-active".to_string());
    goal.execution_thread_ids = vec![
        "thread-compaction-root".to_string(),
        "thread-compaction-active".to_string(),
    ];
    goal.steps[0].title = "Inspect persisted scope".to_string();
    goal.steps[0].summary = Some("Persisted scope summary".to_string());
    goal.plan_summary = Some("Persisted compaction plan".to_string());
    goal.last_error = Some("Persisted scope error".to_string());
    goal.events = vec![GoalRunEvent {
        id: "event-compaction-scope-fast".to_string(),
        timestamp: now_millis(),
        phase: "progress".to_string(),
        message: "Persisted event message".to_string(),
        details: None,
        step_index: Some(0),
        todo_snapshot: Vec::new(),
    }];
    engine
        .history
        .upsert_goal_run(&goal)
        .await
        .expect("goal run should persist");
    engine.goal_runs.lock().await.clear();
    engine
        .history
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                rusqlite::params!["goal-compaction-scope-fast"],
            )?;
            conn.execute(
                "UPDATE goal_run_events SET timestamp = 'not-an-integer' WHERE goal_run_id = ?1",
                rusqlite::params!["goal-compaction-scope-fast"],
            )?;
            Ok(())
        })
        .await
        .expect("corrupt hydration-only goal columns");

    let scope = engine
        .compaction_scope_snapshot("thread-compaction-active", None)
        .await
        .expect("persisted goal should produce a compaction scope");

    assert_eq!(
        scope.goal_run_id.as_deref(),
        Some("goal-compaction-scope-fast")
    );
    assert_eq!(scope.task_id.as_deref(), Some("task-compaction-active"));
    assert_eq!(scope.goal_title.as_deref(), Some("Compaction goal"));
    assert_eq!(
        scope.goal.as_deref(),
        Some("Preserve persisted goal context")
    );
    assert_eq!(
        scope.current_step_title.as_deref(),
        Some("Inspect persisted scope")
    );
    assert_eq!(
        scope.current_step_summary.as_deref(),
        Some("Persisted scope summary")
    );
    assert_eq!(scope.latest_error.as_deref(), Some("Persisted scope error"));
    assert_eq!(scope.recent_events, vec!["Persisted event message"]);
}

#[tokio::test]
async fn retarget_task_to_weles_updates_persisted_task_after_live_queue_clear() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let task = engine
        .enqueue_task(
            "Route to Weles".to_string(),
            "Apply Weles identity to this persisted task".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "event_trigger",
            None,
            None,
            Some("thread-retarget-weles".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.tasks.lock().await.clear();

    let updated = engine
        .retarget_task_to_weles(&task.id)
        .await
        .expect("persisted task should be retargeted");

    assert_eq!(
        updated.sub_agent_def_id.as_deref(),
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    );
    assert!(updated
        .override_system_prompt
        .as_deref()
        .is_some_and(|prompt| prompt.contains(crate::agent::agent_identity::WELES_AGENT_ID)));

    let persisted = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(task.id.clone()),
            status: None,
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
        .await
        .into_iter()
        .next()
        .expect("persisted task should remain queryable");
    assert_eq!(
        persisted.sub_agent_def_id.as_deref(),
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    );
}

#[tokio::test]
async fn start_goal_run_reuses_matching_persisted_active_goal_after_live_queue_clear() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let first = engine
        .start_goal_run(
            "Build a persisted duplicate guard".to_string(),
            Some("Persisted duplicate guard".to_string()),
            Some("thread-persisted-duplicate-guard".to_string()),
            Some("session-persisted-duplicate-guard".to_string()),
            Some("normal"),
            Some("client-request-persisted-duplicate-guard".to_string()),
            None,
            None,
        )
        .await;
    engine.goal_runs.lock().await.clear();

    let persisted_candidates = engine
        .history
        .list_active_goal_runs_for_start_request(
            Some("thread-persisted-duplicate-guard".to_string()),
            Some("session-persisted-duplicate-guard".to_string()),
            Some("client-request-persisted-duplicate-guard".to_string()),
        )
        .await
        .expect("persisted active goal run query should succeed");
    assert_eq!(
        persisted_candidates
            .iter()
            .map(|goal_run| goal_run.id.as_str())
            .collect::<Vec<_>>(),
        vec![first.id.as_str()]
    );

    let second = engine
        .start_goal_run(
            "Build a persisted duplicate guard".to_string(),
            Some("Persisted duplicate guard".to_string()),
            Some("thread-persisted-duplicate-guard".to_string()),
            Some("session-persisted-duplicate-guard".to_string()),
            Some("normal"),
            Some("client-request-persisted-duplicate-guard".to_string()),
            None,
            None,
        )
        .await;

    assert_eq!(
        second.id, first.id,
        "matching active goal run should be selected from SQLite when the live queue is empty"
    );
    let (persisted_ids, persisted_total) = engine
        .history
        .list_goal_run_ids_page(10, 0)
        .await
        .expect("persisted goal runs should be listed");
    assert_eq!(persisted_total, 1);
    assert_eq!(persisted_ids, vec![first.id]);
}

fn sample_supervised_goal_run(goal_run_id: &str, task_id: &str, approval_id: &str) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "supervised goal".to_string(),
        goal: "verify explicit acknowledgment".to_string(),
        client_request_id: None,
        status: GoalRunStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        created_at: now_millis(),
        updated_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        thread_id: None,
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("step-1".to_string()),
        current_step_kind: Some(GoalRunStepKind::Command),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        step_failure_history: Vec::new(),
        stopped_reason: None,
        child_task_ids: vec![task_id.to_string()],
        child_task_count: 1,
        approval_count: 0,
        awaiting_approval_id: Some(approval_id.to_string()),
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: Some(task_id.to_string()),
        pending_review_report: None,
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "step-1".to_string(),
            instructions: "do supervised work".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "done".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: Some(task_id.to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: None,
        }],
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: super::autonomy::AutonomyLevel::Supervised,
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        supervision_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    }
}

fn sample_owner_profile(
    agent_label: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> GoalRuntimeOwnerProfile {
    GoalRuntimeOwnerProfile {
        agent_label: agent_label.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
    }
}

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

fn goal_run_detail_object(goal_run_json: &str) -> serde_json::Map<String, serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(goal_run_json).expect("parse capped goal run detail json");
    value
        .as_object()
        .cloned()
        .expect("goal run detail payload should be a JSON object")
}

async fn sample_awaiting_task(
    engine: &AgentEngine,
    goal_run_id: &str,
    task_id: &str,
    approval_id: &str,
) {
    engine.tasks.lock().await.push_back(AgentTask {
        id: task_id.to_string(),
        title: "step task".to_string(),
        description: "goal step work".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 30,
        created_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        error: None,
        result: None,
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("supervised goal".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("awaiting supervised acknowledgment".to_string()),
        awaiting_approval_id: Some(approval_id.to_string()),
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
    });
}

#[tokio::test]
async fn approval_resolution_clears_thread_skill_gate_when_task_is_approved() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let approval_id = "managed-approval-1";
    let thread_id = "thread-approval-resolution";

    engine
        .set_thread_skill_discovery_state(
            thread_id,
            LatestSkillDiscoveryState {
                query: "debug panic".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "request_approval systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: true,
                mesh_approval_id: Some(approval_id.to_string()),
                read_skill_identifier: Some("systematic-debugging".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: true,
                compliant: false,
                updated_at: now_millis(),
            },
        )
        .await;

    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
        source: "managed_command".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: Some("echo ok".to_string()),
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
        blocked_reason: Some("awaiting approval".to_string()),
        awaiting_approval_id: Some(approval_id.to_string()),
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
    });

    assert!(
        engine
            .handle_task_approval_resolution(
                approval_id,
                zorai_protocol::ApprovalDecision::ApproveOnce
            )
            .await
    );

    let state = engine
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("thread skill state should remain present");
    assert!(state.compliant);
    assert!(!state.mesh_requires_approval);
}

#[tokio::test]
async fn unrelated_task_approval_does_not_clear_thread_skill_gate() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-unrelated-approval";

    engine
        .set_thread_skill_discovery_state(
            thread_id,
            LatestSkillDiscoveryState {
                query: "debug panic".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "request_approval systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: true,
                mesh_approval_id: Some("mesh-approval-id".to_string()),
                read_skill_identifier: Some("systematic-debugging".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: true,
                compliant: false,
                updated_at: now_millis(),
            },
        )
        .await;

    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-unrelated".to_string(),
        title: "approval task unrelated".to_string(),
        description: "awaiting unrelated approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
        source: "managed_command".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: Some("echo ok".to_string()),
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
        blocked_reason: Some("awaiting approval".to_string()),
        awaiting_approval_id: Some("different-approval-id".to_string()),
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
    });

    assert!(
        engine
            .handle_task_approval_resolution(
                "different-approval-id",
                zorai_protocol::ApprovalDecision::ApproveOnce
            )
            .await
    );

    let state = engine
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("thread skill state should remain present");
    assert!(!state.compliant);
    assert!(state.mesh_requires_approval);
    assert_eq!(state.mesh_approval_id.as_deref(), Some("mesh-approval-id"));
}

#[tokio::test]
async fn denied_task_approval_converts_thread_skill_gate_to_bypassable_state() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let approval_id = "managed-approval-deny";
    let thread_id = "thread-approval-denied";

    engine
        .set_thread_skill_discovery_state(
            thread_id,
            LatestSkillDiscoveryState {
                query: "debug panic".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "request_approval systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: true,
                mesh_approval_id: Some(approval_id.to_string()),
                read_skill_identifier: Some("systematic-debugging".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: true,
                compliant: false,
                updated_at: now_millis(),
            },
        )
        .await;

    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-denied".to_string(),
        title: "approval task denied".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
        source: "managed_command".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: Some("echo ok".to_string()),
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
        blocked_reason: Some("awaiting approval".to_string()),
        awaiting_approval_id: Some(approval_id.to_string()),
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
    });

    assert!(
        engine
            .handle_task_approval_resolution(approval_id, zorai_protocol::ApprovalDecision::Deny)
            .await
    );

    let state = engine
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("thread skill state should remain present");
    assert!(!state.mesh_requires_approval);
    assert_eq!(state.recommended_action, "justify_skill_skip");
    assert_eq!(
        state.mesh_next_step,
        Some(crate::agent::skill_mesh::types::SkillMeshNextStep::JustifySkillSkip)
    );
    assert!(!state.compliant);
}

#[tokio::test]
async fn resume_does_not_clear_supervised_awaiting_approval_gate() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised";
    let task_id = "task-supervised";
    let approval_id = "autonomy-ack-1";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let changed = engine
        .control_goal_run(goal_run_id, "resume", None, None)
        .await;
    assert!(
        !changed,
        "resume should not mutate awaiting-approval supervised runs"
    );

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::AwaitingApproval);
    assert_eq!(goal.awaiting_approval_id.as_deref(), Some(approval_id));

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::AwaitingApproval);
    assert_eq!(task.awaiting_approval_id.as_deref(), Some(approval_id));
}

#[tokio::test]
async fn explicit_acknowledgment_unblocks_goal_and_current_step_task() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised";
    let task_id = "task-supervised";
    let approval_id = "autonomy-ack-2";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let changed = engine
        .control_goal_run(goal_run_id, "acknowledge", None, None)
        .await;
    assert!(changed, "acknowledge should clear supervised gate");

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::Running);
    assert!(goal.awaiting_approval_id.is_none());

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());
}

#[tokio::test]
async fn explicit_acknowledgment_unblocks_persisted_current_step_task_after_live_queue_clear() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised-persisted";
    let task_id = "task-supervised-persisted";
    let approval_id = "autonomy-ack-persisted";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let changed = engine
        .control_goal_run(goal_run_id, "acknowledge", None, None)
        .await;
    assert!(changed, "acknowledge should clear supervised gate");

    let task = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(task_id.to_string()),
            status: None,
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
        .await
        .into_iter()
        .next()
        .expect("persisted task should remain queryable");

    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());
    assert!(
        task.logs.iter().any(|entry| {
            entry.phase == "autonomy_acknowledgment"
                && entry.message.contains("task released to queue")
        }),
        "persisted task should record acknowledgment release"
    );
}

#[tokio::test]
async fn task_approval_resolution_syncs_parent_goal_run_state() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-policy-escalation";
    let task_id = "task-policy-escalation";
    let approval_id = "policy-escalation-thread_sync-1000";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    assert!(
        engine
            .handle_task_approval_resolution(
                approval_id,
                zorai_protocol::ApprovalDecision::ApproveOnce
            )
            .await
    );

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::Running);
    assert!(goal.awaiting_approval_id.is_none());

    let task = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(task_id.to_string()),
            status: None,
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
        .await
        .into_iter()
        .next()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());
}

#[tokio::test]
async fn get_run_reads_persisted_task_after_live_queue_clear() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let task = engine
        .enqueue_task(
            "Persisted run".to_string(),
            "get_run should fetch this task directly from history".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some("thread-persisted-run".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let run = engine
        .get_run(&task.id)
        .await
        .expect("persisted task should project to a run");
    assert_eq!(run.id, task.id);
    assert_eq!(run.title, "Persisted run");
}

#[tokio::test]
async fn run_listing_skips_persisted_task_logs_but_keeps_the_tasks() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let task = engine
        .enqueue_task(
            "Run with logs".to_string(),
            "run listings are polled and must not hydrate the task log table".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some("thread-run-with-logs".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();
    let query = crate::history::AgentTaskListQuery {
        id: Some(task.id.clone()),
        ..Default::default()
    };

    let with_logs = engine.list_tasks_filtered(&query).await;
    let without_logs = engine.list_tasks_filtered_without_logs(&query).await;

    assert!(
        with_logs.iter().any(|listed| listed.id == task.id && !listed.logs.is_empty()),
        "the full listing must still hydrate persisted logs for callers that show them"
    );
    assert_eq!(
        without_logs.iter().map(|listed| listed.id.as_str()).collect::<Vec<_>>(),
        vec![task.id.as_str()],
        "skipping logs must not drop tasks from the run listing"
    );
    assert!(
        without_logs.iter().all(|listed| listed.logs.is_empty()),
        "run listings are polled every few seconds, so they must not load the task log table"
    );
}

#[tokio::test]
async fn list_runs_reads_persisted_tasks_after_live_queue_clear() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let task = engine
        .enqueue_task(
            "Persisted listed run".to_string(),
            "list_runs should fetch this task from history".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some("thread-persisted-listed-run".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let runs = engine.list_runs().await;
    assert!(
        runs.iter()
            .any(|run| run.id == task.id && run.title == "Persisted listed run"),
        "persisted task should project into list_runs after the live queue is cleared"
    );
}

#[tokio::test]
async fn list_runs_for_parent_thread_excludes_unrelated_history_and_keeps_terminal_children() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let matching = engine
        .enqueue_task(
            "Matching completed child".to_string(),
            "belongs to the selected parent thread".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some("thread-selected".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let unrelated = engine
        .enqueue_task(
            "Unrelated historical child".to_string(),
            "belongs to another parent thread".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some("thread-other".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    {
        let mut tasks = engine.tasks.lock().await;
        let selected = tasks
            .iter_mut()
            .find(|task| task.id == matching.id)
            .expect("matching task should exist");
        selected.status = TaskStatus::Completed;
        selected.completed_at = Some(selected.created_at + 1);
    }
    engine.persist_tasks().await;

    let runs = engine.list_runs_for_parent_thread("thread-selected").await;

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, matching.id);
    assert_eq!(runs[0].status, TaskStatus::Completed);
    assert!(!runs.iter().any(|run| run.id == unrelated.id));
}

#[tokio::test]
async fn list_tasks_reads_persisted_tasks_after_live_queue_clear() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let task = engine
        .enqueue_task(
            "Persisted listed task".to_string(),
            "list_tasks should fetch this task from history".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some("thread-persisted-listed-task".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let tasks = engine.list_tasks().await;
    assert!(
        tasks
            .iter()
            .any(|listed| listed.id == task.id && listed.title == "Persisted listed task"),
        "persisted task should be included after the live queue is cleared"
    );
}

#[tokio::test]
async fn create_and_revoke_task_approval_rule_tracks_pending_task_command() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let approval_id = "approval-rule-1";

    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-rule-1".to_string(),
        title: "policy escalation".to_string(),
        description: "needs approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 40,
        created_at: now_millis(),
        started_at: Some(now_millis()),
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
        blocked_reason: Some(
            "waiting for operator approval: orchestrator_policy_escalation".to_string(),
        ),
        awaiting_approval_id: Some(approval_id.to_string()),
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
    });
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let rule = engine
        .create_task_approval_rule_from_pending(approval_id)
        .await
        .expect("create rule should succeed")
        .expect("rule should be created");
    assert_eq!(rule.command, "orchestrator_policy_escalation");
    assert_eq!(engine.list_task_approval_rules().await.len(), 1);

    assert!(engine.revoke_task_approval_rule(&rule.id).await);
    assert!(engine.list_task_approval_rules().await.is_empty());
}

#[tokio::test]
async fn create_task_approval_rule_from_live_managed_approval_without_task() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let approval = ToolPendingApproval {
        approval_id: "managed-approval-rule-1".to_string(),
        execution_id: "exec-1".to_string(),
        command: "git status --short".to_string(),
        rationale: "Check repo status".to_string(),
        risk_level: "medium".to_string(),
        blast_radius: "thread".to_string(),
        reasons: vec!["network access requested".to_string()],
        session_id: Some("session-1".to_string()),
    };

    engine.remember_pending_approval_command(&approval).await;

    let rule = engine
        .create_task_approval_rule_from_pending(&approval.approval_id)
        .await
        .expect("create rule should succeed")
        .expect("rule should be created from live approval metadata");

    assert_eq!(rule.command, approval.command);
    assert_eq!(engine.list_task_approval_rules().await.len(), 1);

    engine
        .forget_pending_approval_command(&approval.approval_id)
        .await;
    assert!(engine.revoke_task_approval_rule(&rule.id).await);
}

#[tokio::test]
async fn mark_task_awaiting_approval_updates_persisted_task_after_live_queue_clear() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-persisted-mark-awaiting-approval";
    let task = engine
        .enqueue_task(
            "Persisted approval task".to_string(),
            "Mark persisted task as awaiting operator approval".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();
    let approval = ToolPendingApproval {
        approval_id: "approval-persisted-mark-awaiting".to_string(),
        execution_id: "exec-persisted-mark-awaiting".to_string(),
        command: "git status --short".to_string(),
        rationale: "Check repo status".to_string(),
        risk_level: "medium".to_string(),
        blast_radius: "thread".to_string(),
        reasons: vec!["operator confirmation required".to_string()],
        session_id: Some("session-persisted-mark-awaiting".to_string()),
    };

    engine
        .mark_task_awaiting_approval(&task.id, thread_id, &approval)
        .await;

    let persisted = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(task.id.clone()),
            status: None,
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
        .await
        .pop()
        .expect("persisted task should remain queryable");
    assert_eq!(persisted.status, TaskStatus::AwaitingApproval);
    assert_eq!(
        persisted.awaiting_approval_id.as_deref(),
        Some(approval.approval_id.as_str())
    );
    assert_eq!(persisted.thread_id.as_deref(), Some(thread_id));
    assert_eq!(
        persisted.session_id.as_deref(),
        approval.session_id.as_deref()
    );
    assert!(persisted
        .logs
        .iter()
        .any(|entry| entry.message == "managed command paused for operator approval"));
}

#[tokio::test]
async fn cancelling_goal_run_settles_unresolved_goal_plan_trace() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised-cancel";
    let task_id = "task-supervised-cancel";
    let approval_id = "autonomy-ack-cancel";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let selected_json = serde_json::json!({
        "option_type": "goal_plan",
        "reasoning": "Use a supervised single-step plan",
        "rejection_reason": null,
        "estimated_success_prob": 0.61,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let unresolved =
        serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
            .expect("serialize unresolved outcome");
    engine
        .history
        .insert_causal_trace(
            "causal_goal_plan_cancel_hook",
            None,
            Some(goal_run_id),
            None,
            "plan_selection",
            crate::agent::learning::traces::DecisionType::PlanSelection.family_label(),
            &selected_json,
            "[]",
            "ctx_hash",
            "[]",
            &unresolved,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert goal plan causal trace");

    let changed = engine
        .control_goal_run(goal_run_id, "cancel", None, None)
        .await;
    assert!(changed, "cancel should update goal state");

    let records = engine
        .history
        .list_recent_causal_trace_records("goal_plan", 1)
        .await
        .expect("list goal plan traces");
    let outcome = serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &records[0].outcome_json,
    )
    .expect("deserialize settled outcome");
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            assert!(reason.contains("cancelled"));
        }
        other => panic!("expected cancelled failure outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn stopping_goal_run_records_operator_stop_resume_decision() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised-stop";
    let task_id = "task-supervised-stop";
    let approval_id = "autonomy-ack-stop";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;
    let wakeup = engine
        .schedule_wakeup_with_context(
            "thread-supervised-stop",
            60_000,
            1,
            "supervise stop",
            "goal_supervision",
            Some(goal_run_id),
        )
        .await
        .expect("valid goal supervision");

    let changed = engine
        .control_goal_run(goal_run_id, "stop", None, None)
        .await;
    assert!(changed, "stop should update goal state");

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::Cancelled);
    assert_eq!(goal.stopped_reason.as_deref(), Some("operator_stop"));
    assert!(goal.awaiting_approval_id.is_none());
    assert!(goal.active_task_id.is_none());
    assert!(!engine.timer_wakeups.lock().await.contains_key(&wakeup.id));
    assert!(!engine
        .history
        .list_agent_wakeups()
        .await
        .expect("wakeups")
        .iter()
        .any(|row| row.id == wakeup.id));
}

#[tokio::test]
async fn pausing_goal_suspends_recursive_tasks_and_resume_restores_without_duplication() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-pause-tree";
    let root_task_id = "task-pause-root";
    let child_task_id = "task-pause-child";
    let root_thread = "thread-pause-root";
    let child_thread = "thread-pause-child";
    let mut goal = sample_supervised_goal_run(goal_run_id, root_task_id, "approval-pause");
    goal.status = GoalRunStatus::Running;
    goal.awaiting_approval_id = None;
    goal.thread_id = Some(root_thread.to_string());
    goal.execution_thread_ids = vec![child_thread.to_string()];
    engine.goal_runs.lock().await.push_back(goal);
    sample_awaiting_task(&engine, goal_run_id, root_task_id, "approval-pause").await;
    {
        let mut tasks = engine.tasks.lock().await;
        let root = tasks
            .iter_mut()
            .find(|task| task.id == root_task_id)
            .unwrap();
        root.status = TaskStatus::InProgress;
        root.thread_id = Some(root_thread.to_string());
        root.awaiting_approval_id = None;
        let mut child = root.clone();
        child.id = child_task_id.to_string();
        child.goal_run_id = None;
        child.parent_task_id = Some(root_task_id.to_string());
        child.parent_thread_id = Some(root_thread.to_string());
        child.thread_id = Some(child_thread.to_string());
        tasks.push_back(child);
    }
    engine.persist_tasks().await;
    let (_, root_token, _) = engine.begin_stream_cancellation(root_thread).await;
    let (_, child_token, _) = engine.begin_stream_cancellation(child_thread).await;

    assert!(
        engine
            .control_goal_run(goal_run_id, "pause", None, None)
            .await
    );
    assert!(root_token.is_cancelled());
    assert!(child_token.is_cancelled());
    for id in [root_task_id, child_task_id] {
        let task = engine
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some(id.to_string()),
                status: None,
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
            .await
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(task.status, TaskStatus::Blocked);
        assert!(task
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| reason.starts_with("goal_paused:")));
    }

    assert!(
        engine
            .control_goal_run(goal_run_id, "resume", None, None)
            .await
    );
    for id in [root_task_id, child_task_id] {
        let task = engine
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some(id.to_string()),
                status: None,
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
            .await
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(task.status, TaskStatus::Queued);
        assert!(task.blocked_reason.is_none());
    }
}

#[tokio::test]
async fn stopping_goal_run_cancels_all_related_goal_tasks_and_streams() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-stop-all-related";
    let active_task_id = "task-goal-active";
    let child_task_id = "task-goal-child";
    let approval_id = "autonomy-ack-stop-all";
    let active_thread_id = "thread-goal-active";
    let child_thread_id = "thread-goal-child";

    let mut goal_run = sample_supervised_goal_run(goal_run_id, active_task_id, approval_id);
    goal_run.status = GoalRunStatus::Running;
    goal_run.awaiting_approval_id = None;
    goal_run.active_thread_id = Some(active_thread_id.to_string());
    goal_run.execution_thread_ids = vec![active_thread_id.to_string(), child_thread_id.to_string()];
    engine.goal_runs.lock().await.push_back(goal_run);

    sample_awaiting_task(&engine, goal_run_id, active_task_id, approval_id).await;
    {
        let mut tasks = engine.tasks.lock().await;
        let active = tasks
            .iter_mut()
            .find(|task| task.id == active_task_id)
            .expect("active task should exist");
        active.status = TaskStatus::InProgress;
        active.awaiting_approval_id = None;
        active.blocked_reason = None;
        active.thread_id = Some(active_thread_id.to_string());

        let mut child = active.clone();
        child.id = child_task_id.to_string();
        child.title = "spawned goal child".to_string();
        child.thread_id = Some(child_thread_id.to_string());
        child.parent_task_id = Some(active_task_id.to_string());
        child.parent_thread_id = Some(active_thread_id.to_string());
        child.status = TaskStatus::InProgress;
        tasks.push_back(child);
    }
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let (_active_generation, active_token, _active_retry) =
        engine.begin_stream_cancellation(active_thread_id).await;
    let (_child_generation, child_token, _child_retry) =
        engine.begin_stream_cancellation(child_thread_id).await;
    let generic_wakeup = engine
        .schedule_wakeup_with_context(
            child_thread_id,
            60_000,
            1,
            "child generic wakeup",
            "generic",
            None,
        )
        .await
        .expect("valid child wakeup");
    engine
        .register_operation_wakeup(child_thread_id, "bash_command", "operation-child", true)
        .await;

    let changed = engine
        .control_goal_run(goal_run_id, "stop", None, None)
        .await;
    assert!(changed, "stop should update goal state");

    let active = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(active_task_id.to_string()),
            status: None,
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
        .await
        .into_iter()
        .next()
        .expect("active task should remain recorded");
    let child = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(child_task_id.to_string()),
            status: None,
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
        .await
        .into_iter()
        .next()
        .expect("child task should remain recorded");
    assert_eq!(active.status, TaskStatus::Cancelled);
    assert_eq!(child.status, TaskStatus::Cancelled);
    assert!(
        active_token.is_cancelled(),
        "active stream should be cancelled"
    );
    assert!(
        child_token.is_cancelled(),
        "child stream should be cancelled"
    );
    assert!(!engine
        .timer_wakeups
        .lock()
        .await
        .contains_key(&generic_wakeup.id));
    assert_eq!(engine.pending_operation_wakeup_count().await, 0);
}

#[tokio::test]
async fn get_goal_run_capped_for_ipc_truncates_oversized_detail_payload() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-huge-detail";
    let huge_details = "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    let mut goal_run = sample_supervised_goal_run(goal_run_id, "task-huge", "approval-huge");
    goal_run.events.push(GoalRunEvent {
        id: "event-huge".to_string(),
        timestamp: now_millis(),
        phase: "running".to_string(),
        message: "huge event".to_string(),
        details: Some(huge_details),
        step_index: Some(0),
        todo_snapshot: Vec::new(),
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_capped_for_ipc(goal_run_id)
        .await
        .expect("goal should exist");
    assert!(truncated, "oversized goal detail should be truncated");
    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_start")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_end")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        goal_run_object
            .get("total_step_count")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        goal_run_object
            .get("total_event_count")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert!(
        goal_run.events.is_empty(),
        "huge event should be dropped to fit the IPC cap"
    );
    let mut frame = BytesMut::new();
    zorai_protocol::DaemonCodec::default()
        .encode(
            zorai_protocol::DaemonMessage::AgentGoalRunDetail { goal_run_json },
            &mut frame,
        )
        .expect("serialize goal run detail frame");

    assert!(
        frame.len().saturating_sub(4) <= zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES,
        "goal run detail should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn get_goal_run_capped_for_ipc_preserves_owner_profiles_when_step_slice_drops_prefix() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-owner-metadata-step-slice";
    let huge_instructions = "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    let mut goal_run = sample_supervised_goal_run(goal_run_id, "task-owner", "approval-owner");
    goal_run.launch_assignment_snapshot = vec![
        sample_assignment("planner", "openai", "gpt-5", Some("high")),
        sample_assignment("executor", "anthropic", "claude-sonnet-4", None),
    ];
    goal_run.runtime_assignment_list = vec![sample_assignment(
        "executor",
        "anthropic",
        "claude-sonnet-4",
        None,
    )];
    goal_run.root_thread_id = Some("thread-root".to_string());
    goal_run.active_thread_id = Some("thread-current".to_string());
    goal_run.execution_thread_ids = vec![
        "thread-root".to_string(),
        "thread-current".to_string(),
        "thread-followup".to_string(),
    ];
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    goal_run.current_step_index = 1;
    goal_run.current_step_title = Some("step-current".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.active_task_id = Some("task-current".to_string());
    goal_run.steps = vec![
        GoalRunStep {
            id: "step-prefix".to_string(),
            position: 0,
            title: "step-prefix".to_string(),
            instructions: huge_instructions,
            kind: GoalRunStepKind::Command,
            success_criteria: "prefix can be dropped".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Completed,
            task_id: Some("task-prefix".to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: Some(now_millis()),
        },
        GoalRunStep {
            id: "step-current".to_string(),
            position: 1,
            title: "step-current".to_string(),
            instructions: "keep this step".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "current step remains meaningful".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: Some("task-current".to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: None,
        },
    ];
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_capped_for_ipc(goal_run_id)
        .await
        .expect("goal should exist");
    assert!(truncated, "oversized goal detail should be truncated");
    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(2),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_start")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_end")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("total_step_count")
            .and_then(serde_json::Value::as_u64),
        Some(2),
    );
    assert_eq!(
        goal_run_object
            .get("total_event_count")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("current_step_index")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "current step index should be rebased after slicing",
    );
    assert_eq!(
        goal_run_object.get("current_step_kind"),
        Some(&serde_json::json!("command")),
        "current step kind should follow the rebased current step",
    );
    assert_eq!(
        goal_run_object.get("active_task_id"),
        Some(&serde_json::json!("task-current")),
        "active task id should follow the rebased current step",
    );
    assert_eq!(
        goal_run_object.get("root_thread_id"),
        Some(&serde_json::json!("thread-root")),
        "root thread id should survive IPC capping",
    );
    assert_eq!(
        goal_run_object.get("active_thread_id"),
        Some(&serde_json::json!("thread-current")),
        "active thread id should survive IPC capping",
    );
    assert_eq!(
        goal_run_object
            .get("execution_thread_ids")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(3),
        "execution thread ids should survive IPC capping",
    );
    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert_eq!(
        goal_run.planner_owner_profile,
        Some(sample_owner_profile(
            "planner",
            "openai",
            "gpt-5",
            Some("high"),
        )),
        "planner owner profile should survive IPC capping",
    );
    assert_eq!(
        goal_run.current_step_owner_profile,
        Some(sample_owner_profile(
            "current-step",
            "anthropic",
            "claude-sonnet-4",
            None,
        )),
        "current-step owner profile should survive IPC capping when the step still exists",
    );
    assert_eq!(
        goal_run.steps.len(),
        1,
        "step slicing should drop the oversized prefix step",
    );
    assert_eq!(goal_run.current_step_index, 0);
    assert_eq!(goal_run.current_step_kind, Some(GoalRunStepKind::Command));
    assert_eq!(goal_run.active_task_id.as_deref(), Some("task-current"));
    assert_eq!(goal_run.current_step_title.as_deref(), Some("step-current"));
    assert_eq!(
        goal_run.launch_assignment_snapshot,
        vec![
            sample_assignment("planner", "openai", "gpt-5", Some("high")),
            sample_assignment("executor", "anthropic", "claude-sonnet-4", None),
        ],
        "launch assignment snapshot should survive IPC capping",
    );
    assert_eq!(
        goal_run.runtime_assignment_list,
        vec![sample_assignment(
            "executor",
            "anthropic",
            "claude-sonnet-4",
            None,
        )],
        "runtime assignment list should survive IPC capping",
    );
    assert_eq!(
        goal_run.root_thread_id.as_deref(),
        Some("thread-root"),
        "root thread id should survive IPC capping",
    );
    assert_eq!(
        goal_run.active_thread_id.as_deref(),
        Some("thread-current"),
        "active thread id should survive IPC capping",
    );
    assert_eq!(
        goal_run.execution_thread_ids,
        vec![
            "thread-root".to_string(),
            "thread-current".to_string(),
            "thread-followup".to_string(),
        ],
        "execution thread ids should survive IPC capping",
    );
}

#[tokio::test]
async fn get_goal_run_capped_for_ipc_preserves_planner_owner_profile_in_stripped_summary_payload() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-owner-metadata-summary";
    let huge_instructions = "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    let mut goal_run = sample_supervised_goal_run(goal_run_id, "task-owner", "approval-owner");
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    goal_run.steps = vec![GoalRunStep {
        id: "step-summary".to_string(),
        position: 0,
        title: "step-summary".to_string(),
        instructions: huge_instructions,
        kind: GoalRunStepKind::Command,
        success_criteria: "summary path should strip steps".to_string(),
        session_id: None,
        status: GoalRunStepStatus::InProgress,
        task_id: Some("task-summary".to_string()),
        summary: None,
        error: None,
        started_at: Some(now_millis()),
        completed_at: None,
    }];
    goal_run.current_step_index = 0;
    goal_run.current_step_title = Some("step-summary".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.active_task_id = Some("task-summary".to_string());
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_capped_for_ipc(goal_run_id)
        .await
        .expect("goal should exist");
    assert!(truncated, "oversized goal detail should be truncated");
    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_start")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_event_end")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    assert_eq!(
        goal_run_object
            .get("total_step_count")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("total_event_count")
            .and_then(serde_json::Value::as_u64),
        Some(0),
    );
    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert_eq!(
        goal_run.planner_owner_profile,
        Some(sample_owner_profile(
            "planner",
            "openai",
            "gpt-5",
            Some("high"),
        )),
        "planner owner profile should survive summary stripping",
    );
    assert!(
        goal_run.current_step_owner_profile.is_none(),
        "current-step owner profile should be cleared when the current step is stripped",
    );
    assert!(
        goal_run_object.get("current_step_owner_profile").is_none(),
        "the raw wire payload should omit current-step owner metadata when the current step is stripped",
    );
    assert!(
        goal_run.steps.is_empty(),
        "summary stripping should remove oversized step payloads",
    );
}

#[tokio::test]
async fn get_goal_run_page_capped_for_ipc_clears_current_step_owner_profile_for_empty_step_window()
{
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-empty-step-window";

    let mut goal_run =
        sample_supervised_goal_run(goal_run_id, "task-empty-window", "approval-empty-window");
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_page_capped_for_ipc(goal_run_id, Some(1), Some(0), None, None)
        .await
        .expect("goal should exist");
    assert!(
        !truncated,
        "an explicit empty step window should still serialize"
    );

    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object.get("current_step_owner_profile"),
        None,
        "current-step owner profile should be cleared when the step window is empty",
    );
    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert!(goal_run.steps.is_empty());
    assert!(goal_run.current_step_owner_profile.is_none());
    assert_eq!(goal_run.current_step_index, 0);
    assert!(goal_run.current_step_title.is_none());
    assert!(goal_run.current_step_kind.is_none());
    assert!(goal_run.active_task_id.is_none());
}

#[tokio::test]
async fn get_goal_run_page_capped_for_ipc_clears_current_step_owner_profile_when_paged_window_excludes_current_step(
) {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-paged-window-excludes-current-step";

    let mut goal_run =
        sample_supervised_goal_run(goal_run_id, "task-paged-window", "approval-paged-window");
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    goal_run.steps = vec![
        GoalRunStep {
            id: "step-current".to_string(),
            position: 0,
            title: "step-current".to_string(),
            instructions: "real current step".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "current step exists".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: Some("task-current".to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: None,
        },
        GoalRunStep {
            id: "step-paged".to_string(),
            position: 1,
            title: "step-paged".to_string(),
            instructions: "windowed step".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "paged step exists".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Pending,
            task_id: Some("task-paged".to_string()),
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        },
    ];
    goal_run.current_step_index = 0;
    goal_run.current_step_title = Some("step-current".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.active_task_id = Some("task-current".to_string());
    engine.goal_runs.lock().await.push_back(goal_run);

    let (goal_run_json, truncated) = engine
        .get_goal_run_page_capped_for_ipc(goal_run_id, Some(1), Some(1), None, None)
        .await
        .expect("goal should exist");
    assert!(!truncated, "paged detail should fit without truncation");

    let goal_run_object = goal_run_detail_object(&goal_run_json);
    assert_eq!(
        goal_run_object
            .get("loaded_step_start")
            .and_then(serde_json::Value::as_u64),
        Some(1),
    );
    assert_eq!(
        goal_run_object
            .get("loaded_step_end")
            .and_then(serde_json::Value::as_u64),
        Some(2),
    );
    assert_eq!(
        goal_run_object.get("current_step_title"),
        Some(&serde_json::json!("step-paged")),
    );
    assert_eq!(
        goal_run_object.get("current_step_kind"),
        Some(&serde_json::json!("research")),
    );
    assert_eq!(
        goal_run_object.get("active_task_id"),
        Some(&serde_json::json!("task-paged")),
    );
    assert!(
        goal_run_object.get("current_step_owner_profile").is_none(),
        "current-step owner metadata should be omitted when the retained window excludes the real current step",
    );

    let goal_run: Option<GoalRun> =
        serde_json::from_str(&goal_run_json).expect("parse capped goal run detail json");
    let goal_run = goal_run.expect("goal run detail should still exist");
    assert_eq!(goal_run.current_step_index, 0);
    assert_eq!(goal_run.current_step_title.as_deref(), Some("step-paged"));
    assert_eq!(goal_run.current_step_kind, Some(GoalRunStepKind::Research));
    assert_eq!(goal_run.active_task_id.as_deref(), Some("task-paged"));
    assert!(goal_run.current_step_owner_profile.is_none());
    assert_eq!(
        goal_run.planner_owner_profile,
        Some(sample_owner_profile(
            "planner",
            "openai",
            "gpt-5",
            Some("high"),
        )),
    );
}

#[tokio::test]
async fn goal_run_with_step_slice_clears_current_step_owner_profile_when_slice_excludes_current_step(
) {
    let mut goal_run =
        sample_supervised_goal_run("goal-step-slice", "task-step-slice", "approval-step-slice");
    goal_run.planner_owner_profile = Some(sample_owner_profile(
        "planner",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    goal_run.current_step_owner_profile = Some(sample_owner_profile(
        "current-step",
        "anthropic",
        "claude-sonnet-4",
        None,
    ));
    goal_run.steps = vec![
        GoalRunStep {
            id: "step-current".to_string(),
            position: 0,
            title: "step-current".to_string(),
            instructions: "real current step".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "current step exists".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: Some("task-current".to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: None,
        },
        GoalRunStep {
            id: "step-sliced".to_string(),
            position: 1,
            title: "step-sliced".to_string(),
            instructions: "retained step".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "retained step exists".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Pending,
            task_id: Some("task-sliced".to_string()),
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        },
    ];
    goal_run.current_step_index = 0;
    goal_run.current_step_title = Some("step-current".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.active_task_id = Some("task-current".to_string());

    let sliced = goal_run_with_step_slice(&goal_run, 1);

    assert_eq!(sliced.steps.len(), 1);
    assert_eq!(sliced.current_step_index, 0);
    assert_eq!(sliced.current_step_title.as_deref(), Some("step-sliced"));
    assert_eq!(sliced.current_step_kind, Some(GoalRunStepKind::Research));
    assert_eq!(sliced.active_task_id.as_deref(), Some("task-sliced"));
    assert!(sliced.current_step_owner_profile.is_none());
}

#[tokio::test]
async fn list_goal_runs_payload_stays_below_ipc_frame_cap() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let huge_details = "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    let mut goal_run = sample_supervised_goal_run("goal-huge-list", "task-huge", "approval-huge");
    goal_run.events.push(GoalRunEvent {
        id: "event-huge-list".to_string(),
        timestamp: now_millis(),
        phase: "running".to_string(),
        message: "huge event".to_string(),
        details: Some(huge_details),
        step_index: Some(0),
        todo_snapshot: Vec::new(),
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let goal_runs = engine.list_goal_runs().await;
    let goal_runs_json = serde_json::to_string(&goal_runs).expect("serialize goal run list json");
    let mut frame = BytesMut::new();
    zorai_protocol::DaemonCodec::default()
        .encode(
            zorai_protocol::DaemonMessage::AgentGoalRunList { goal_runs_json },
            &mut frame,
        )
        .expect("serialize goal run list frame");

    assert!(
        frame.len().saturating_sub(4) <= zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES,
        "goal run list should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn list_goal_runs_pagination_obeys_newest_first_limit_and_offset() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut goals = engine.goal_runs.lock().await;
    for (id, updated_at) in [("goal-one", 30), ("goal-two", 20), ("goal-three", 10)] {
        let mut goal =
            sample_supervised_goal_run(id, &format!("task-{id}"), &format!("approval-{id}"));
        goal.updated_at = updated_at;
        goals.push_back(goal);
    }
    drop(goals);

    let (first_page, _) = engine
        .list_goal_runs_paginated_capped_for_ipc(Some(2), Some(0))
        .await;
    let (second_page, _) = engine
        .list_goal_runs_paginated_capped_for_ipc(Some(2), Some(2))
        .await;

    assert_eq!(
        first_page
            .iter()
            .map(|goal| goal.id.as_str())
            .collect::<Vec<_>>(),
        vec!["goal-one", "goal-two"]
    );
    assert_eq!(
        second_page
            .iter()
            .map(|goal| goal.id.as_str())
            .collect::<Vec<_>>(),
        vec!["goal-three"]
    );
}

#[tokio::test]
async fn list_goal_runs_pagination_uses_persisted_goal_runs_after_live_queue_clear() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    {
        let mut goals = engine.goal_runs.lock().await;
        for (id, updated_at) in [
            ("goal-db-one", 30),
            ("goal-db-two", 20),
            ("goal-db-three", 10),
        ] {
            let mut goal =
                sample_supervised_goal_run(id, &format!("task-{id}"), &format!("approval-{id}"));
            goal.updated_at = updated_at;
            goals.push_back(goal);
        }
    }
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();

    let (first_page, _) = engine
        .list_goal_runs_paginated_capped_for_ipc(Some(2), Some(0))
        .await;
    let (second_page, _) = engine
        .list_goal_runs_paginated_capped_for_ipc(Some(2), Some(2))
        .await;

    assert_eq!(
        first_page
            .iter()
            .map(|goal| goal.id.as_str())
            .collect::<Vec<_>>(),
        vec!["goal-db-one", "goal-db-two"]
    );
    assert_eq!(
        second_page
            .iter()
            .map(|goal| goal.id.as_str())
            .collect::<Vec<_>>(),
        vec!["goal-db-three"]
    );
}

#[tokio::test]
async fn list_tasks_capped_for_ipc_truncates_oversized_task_logs() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-small".to_string(),
        title: "small task".to_string(),
        description: "small".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: 1,
        started_at: None,
        completed_at: Some(2),
        error: None,
        result: Some("ok".to_string()),
        thread_id: None,
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
        max_retries: 1,
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
        logs: vec![AgentTaskLogEntry {
            id: "task-small-log".to_string(),
            timestamp: 1,
            level: TaskLogLevel::Info,
            phase: "done".to_string(),
            message: "small log".to_string(),
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
    });

    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-huge".to_string(),
        title: "huge task".to_string(),
        description: "huge".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: 3,
        started_at: None,
        completed_at: Some(4),
        error: None,
        result: Some("ok".to_string()),
        thread_id: None,
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
        max_retries: 1,
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
        logs: vec![AgentTaskLogEntry {
            id: "task-huge-log".to_string(),
            timestamp: 3,
            level: TaskLogLevel::Info,
            phase: "done".to_string(),
            message: "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
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
    });

    let (tasks, truncated) = engine.list_tasks_capped_for_ipc().await;
    assert!(truncated);
    assert!(tasks.iter().any(|task| task.id == "task-small"));
    let huge = tasks
        .iter()
        .find(|task| task.id == "task-huge")
        .expect("huge task should remain present after IPC capping");
    assert!(
        huge.logs.is_empty(),
        "oversized task logs should be dropped to fit IPC"
    );

    let tasks_json = serde_json::to_string(&tasks).expect("serialize capped task list json");
    let mut frame = BytesMut::new();
    zorai_protocol::DaemonCodec::default()
        .encode(
            zorai_protocol::DaemonMessage::AgentTaskList { tasks_json },
            &mut frame,
        )
        .expect("serialize task list frame");

    assert!(
        frame.len().saturating_sub(4) <= zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES,
        "task list should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn list_tasks_metadata_only_for_ipc_skips_persisted_task_logs() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-logged".to_string(),
        title: "logged task".to_string(),
        description: "persisted with huge logs".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: 1,
        started_at: None,
        completed_at: Some(2),
        error: None,
        result: Some("ok".to_string()),
        thread_id: None,
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
        max_retries: 1,
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
        logs: vec![AgentTaskLogEntry {
            id: "task-logged-entry".to_string(),
            timestamp: 3,
            level: TaskLogLevel::Info,
            phase: "done".to_string(),
            message: "persisted log body".to_string(),
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
    });
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let (tasks, _) = engine.list_tasks_metadata_only_capped_for_ipc().await;
    let listed = tasks
        .iter()
        .find(|task| task.id == "task-logged")
        .expect("metadata-only IPC list should include persisted tasks");
    assert!(
        listed.logs.is_empty(),
        "metadata-only IPC list must not hydrate persisted task logs"
    );
}

#[tokio::test]
async fn list_todos_capped_for_ipc_truncates_oversized_payload() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.thread_todos.write().await.insert(
        "thread-small".to_string(),
        vec![TodoItem {
            id: "todo-small".to_string(),
            content: "small".to_string(),
            status: TodoStatus::Pending,
            position: 0,
            step_index: None,
            created_at: 1,
            updated_at: 2,
        }],
    );
    engine.thread_todos.write().await.insert(
        "thread-huge".to_string(),
        vec![TodoItem {
            id: "todo-huge".to_string(),
            content: "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
            status: TodoStatus::Pending,
            position: 0,
            step_index: None,
            created_at: 1,
            updated_at: 1,
        }],
    );

    let (todos_by_thread, truncated) = engine.list_todos_capped_for_ipc().await;
    assert!(truncated);
    assert!(todos_by_thread.contains_key("thread-small"));
    assert!(
        todos_by_thread
            .get("thread-huge")
            .map_or(true, |todos| todos.is_empty()),
        "oversized todo bucket should be dropped or emptied to fit IPC"
    );

    let todos_json = serde_json::to_string(&todos_by_thread).expect("serialize todo list");
    let mut frame = BytesMut::new();
    zorai_protocol::DaemonCodec::default()
        .encode(
            zorai_protocol::DaemonMessage::AgentTodoList { todos_json },
            &mut frame,
        )
        .expect("serialize todo list frame");

    assert!(
        frame.len().saturating_sub(4) <= zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES,
        "todo list should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn start_goal_run_records_goal_start_episode_with_archived_fields() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let goal = engine
        .start_goal_run(
            "repair archived parity gaps".to_string(),
            Some("Repair parity".to_string()),
            Some("thread-epis-1".to_string()),
            Some("session-epis-1".to_string()),
            None,
            None,
            None,
            None,
        )
        .await;

    let episodes = engine
        .list_episodes_for_goal_run(&goal.id)
        .await
        .expect("episodes should load");
    assert_eq!(
        episodes.len(),
        1,
        "goal start should immediately record one episode"
    );

    let episode_json =
        serde_json::to_value(&episodes[0]).expect("episode should serialize to json");
    assert_eq!(episode_json["episode_type"], "goal_start");
    assert_eq!(episode_json["goal_text"], "repair archived parity gaps");
    assert_eq!(episode_json["goal_type"], "goal_run");
    assert_eq!(
        episode_json["summary"],
        "Repair parity: repair archived parity gaps"
    );
    assert!(
        episode_json.get("confidence_before").is_some(),
        "goal-start episodes should carry explicit confidence_before field"
    );
    assert!(
        episode_json.get("confidence_after").is_some(),
        "goal-start episodes should carry explicit confidence_after field"
    );
}

#[tokio::test]
async fn repeated_goal_start_creates_retry_link_to_previous_related_episode() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let first = engine
        .start_goal_run(
            "repair archived parity gaps".to_string(),
            Some("Repair parity".to_string()),
            Some("thread-epis-2".to_string()),
            Some("session-epis-2".to_string()),
            None,
            None,
            None,
            None,
        )
        .await;
    engine
        .record_goal_episode(&first, crate::agent::episodic::EpisodeOutcome::Failure)
        .await
        .expect("first goal failure episode should record");

    let second = engine
        .start_goal_run(
            "repair archived parity gaps".to_string(),
            Some("Repair parity again".to_string()),
            Some("thread-epis-3".to_string()),
            Some("session-epis-3".to_string()),
            None,
            Some("req-2".to_string()),
            None,
            None,
        )
        .await;

    let episodes = engine
        .list_episodes_for_goal_run(&second.id)
        .await
        .expect("episodes should load");
    let start_episode = episodes
        .iter()
        .find(|episode| episode.episode_type == EpisodeType::GoalStart)
        .expect("second goal should have a goal_start episode");

    let links = engine
        .get_episode_links(&start_episode.id)
        .await
        .expect("links should load");
    assert!(
        links.iter().any(|link| link.link_type == LinkType::RetryOf),
        "repeated goal should link to the prior related episode as retry_of"
    );
}

#[tokio::test]
async fn repeated_goal_start_does_not_link_across_persona_scopes() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    run_with_agent_scope(DOMOWOJ_AGENT_ID.to_string(), async {
        let first = engine
            .start_goal_run(
                "repair archived parity gaps".to_string(),
                Some("Persona A parity".to_string()),
                Some("thread-epis-scope-1".to_string()),
                Some("session-epis-scope-1".to_string()),
                None,
                None,
                None,
                None,
            )
            .await;
        engine
            .record_goal_episode(&first, crate::agent::episodic::EpisodeOutcome::Failure)
            .await
            .expect("persona A goal failure episode should record");
    })
    .await;

    run_with_agent_scope(ROD_AGENT_ID.to_string(), async {
        let second = engine
            .start_goal_run(
                "repair archived parity gaps".to_string(),
                Some("Persona B parity".to_string()),
                Some("thread-epis-scope-2".to_string()),
                Some("session-epis-scope-2".to_string()),
                None,
                Some("req-persona-b".to_string()),
                None,
                None,
            )
            .await;

        let episodes = engine
            .list_episodes_for_goal_run(&second.id)
            .await
            .expect("persona B episodes should load");
        let start_episode = episodes
            .iter()
            .find(|episode| episode.episode_type == EpisodeType::GoalStart)
            .expect("persona B goal should have a goal_start episode");

        let links = engine
            .get_episode_links(&start_episode.id)
            .await
            .expect("persona B links should load");
        assert!(
            links.is_empty(),
            "persona-scoped goal-start episodes must not auto-link to another persona's history"
        );
    })
    .await;
}

#[tokio::test]
async fn suppressed_session_id_skips_goal_start_episode_recording() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.episodic.suppressed_session_ids = vec!["session-suppressed".to_string()];
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let goal = engine
        .start_goal_run(
            "do not persist this goal".to_string(),
            Some("Suppressed goal".to_string()),
            Some("thread-suppressed".to_string()),
            Some("session-suppressed".to_string()),
            None,
            None,
            None,
            None,
        )
        .await;

    let episodes = engine
        .list_episodes_for_goal_run(&goal.id)
        .await
        .expect("episodes should load");
    assert!(
        episodes.is_empty(),
        "suppressed session ids should prevent per-session episodic recording"
    );
}

#[tokio::test]
async fn strained_satisfaction_clamps_new_goal_run_max_replans() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.operator_satisfaction.score = 0.21;
        model.operator_satisfaction.label = "strained".to_string();
    }

    let goal = engine
        .start_goal_run(
            "reduce background churn".to_string(),
            Some("Satisfaction clamp".to_string()),
            Some("thread-satisfaction-goal".to_string()),
            Some("session-satisfaction-goal".to_string()),
            None,
            None,
            None,
            None,
        )
        .await;

    assert_eq!(
        goal.max_replans, 1,
        "strained satisfaction should clamp new goal runs to one replan"
    );
}

#[tokio::test]
async fn strained_satisfaction_clamps_goal_task_retries_but_not_regular_tasks() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.max_retries = 4;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.operator_satisfaction.score = 0.23;
        model.operator_satisfaction.label = "strained".to_string();
    }

    let goal_task = engine
        .enqueue_task(
            "goal step".to_string(),
            "execute goal-linked work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "goal_run",
            Some("goal-satisfaction-retries".to_string()),
            None,
            None,
            None,
        )
        .await;

    let regular_task = engine
        .enqueue_task(
            "regular task".to_string(),
            "execute non-goal work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            None,
            None,
        )
        .await;

    assert_eq!(
        goal_task.max_retries, 1,
        "strained satisfaction should clamp goal-linked task retries to one"
    );
    assert_eq!(
        regular_task.max_retries, 4,
        "non-goal tasks should keep the configured retry budget"
    );
}

#[tokio::test]
async fn delete_goal_run_removes_goal_and_related_tasks() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut goal_run = sample_supervised_goal_run("goal-delete", "task-delete", "approval-delete");
    goal_run.child_task_ids = vec!["task-delete".to_string()];
    engine.goal_runs.lock().await.push_back(goal_run.clone());
    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-delete".to_string(),
        goal_run_id: Some("goal-delete".to_string()),
        title: "Child task".to_string(),
        description: "goal-linked task".to_string(),
        status: TaskStatus::Queued,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_title: Some("supervised goal".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
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
    });
    engine
        .history
        .upsert_goal_run(&goal_run)
        .await
        .expect("persist goal run");
    let wakeup = engine
        .schedule_wakeup_with_context(
            "thread-goal-delete",
            60_000,
            1,
            "supervise deletion",
            "goal_supervision",
            Some("goal-delete"),
        )
        .await
        .expect("valid goal supervision");
    engine
        .history
        .upsert_agent_task(&AgentTask {
            id: "task-delete".to_string(),
            goal_run_id: Some("goal-delete".to_string()),
            title: "Child task".to_string(),
            description: "goal-linked task".to_string(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: now_millis(),
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_title: Some("supervised goal".to_string()),
            goal_step_id: Some("step-1".to_string()),
            goal_step_title: Some("step-1".to_string()),
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
        })
        .await
        .expect("persist child task");
    engine
        .history
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_tasks SET created_at = 'not-an-integer' WHERE id = ?1",
                rusqlite::params!["task-delete"],
            )?;
            Ok(())
        })
        .await
        .expect("corrupt unrelated hydrated task column");
    engine.goal_runs.lock().await.clear();
    engine.tasks.lock().await.clear();

    let deleted = engine.delete_goal_run("goal-delete").await;

    assert!(deleted);
    assert!(engine
        .goal_runs
        .lock()
        .await
        .iter()
        .all(|goal_run| goal_run.id != "goal-delete"));
    assert!(engine
        .tasks
        .lock()
        .await
        .iter()
        .all(|task| task.goal_run_id.as_deref() != Some("goal-delete")));
    assert!(!engine.timer_wakeups.lock().await.contains_key(&wakeup.id));
    assert!(!engine
        .history
        .list_agent_wakeups()
        .await
        .expect("wakeups")
        .iter()
        .any(|row| row.id == wakeup.id));
    assert!(engine
        .history
        .get_goal_run("goal-delete")
        .await
        .expect("read goal run")
        .is_none());
    assert!(
        engine
            .history
            .list_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some("task-delete".to_string()),
                status: None,
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
            .await
            .expect("read task")
            .is_empty(),
        "persisted goal task should be deleted even when it is absent from the live queue"
    );
}

async fn supervised_engine_with_goal(
    goal_run_id: &str,
) -> (std::sync::Arc<AgentEngine>, tempfile::TempDir) {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let task_id = "task-outcome";
    let approval_id = "approval-outcome";
    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;
    (engine, root)
}

#[tokio::test]
async fn contain_control_action_transitions_goal_run_to_contained_terminal_state() {
    let goal_run_id = "goal-contain";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    let changed = engine
        .control_goal_run(goal_run_id, "contain", None, None)
        .await;
    assert!(changed, "contain should update goal state");
    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::Contained);
    assert!(goal.status.is_terminal());
    assert_eq!(goal.stopped_reason.as_deref(), Some("operator_contain"));
    assert!(goal.completed_at.is_some());
    assert!(goal.awaiting_approval_id.is_none());
    assert!(goal.active_task_id.is_none());
}

#[tokio::test]
async fn compensate_control_action_transitions_goal_run_to_compensated_terminal_state() {
    let goal_run_id = "goal-compensate";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    let changed = engine
        .control_goal_run(goal_run_id, "compensate", None, None)
        .await;
    assert!(changed, "compensate should update goal state");
    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::Compensated);
    assert!(goal.status.is_terminal());
    assert_eq!(goal.stopped_reason.as_deref(), Some("operator_compensate"));
}

#[tokio::test]
async fn compensate_partial_control_action_transitions_goal_run_to_partially_compensated() {
    let goal_run_id = "goal-partial";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    let changed = engine
        .control_goal_run(goal_run_id, "compensate-partial", None, None)
        .await;
    assert!(changed, "compensate-partial should update goal state");
    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::PartiallyCompensated);
    assert!(goal.status.is_terminal());
    assert_eq!(
        goal.stopped_reason.as_deref(),
        Some("operator_partial_compensate")
    );
}

#[tokio::test]
async fn break_glass_control_action_transitions_goal_run_to_break_glass_terminal_state() {
    let goal_run_id = "goal-break-glass";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    let changed = engine
        .control_goal_run(goal_run_id, "break-glass", None, None)
        .await;
    assert!(changed, "break-glass should update goal state");
    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::BreakGlass);
    assert!(goal.status.is_terminal());
    assert_eq!(goal.stopped_reason.as_deref(), Some("operator_break_glass"));
}

#[tokio::test]
async fn terminal_outcomes_persist_through_sqlite_round_trip() {
    // Each new outcome state should round-trip through the SQLite string
    // serialization so a goal that landed in (say) BreakGlass on the last
    // process is still BreakGlass after daemon restart.
    let goal_run_id = "goal-roundtrip";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    let cases = [
        ("contain", GoalRunStatus::Contained),
        ("compensate", GoalRunStatus::Compensated),
        ("compensate-partial", GoalRunStatus::PartiallyCompensated),
        ("break-glass", GoalRunStatus::BreakGlass),
    ];
    for (action, expected) in cases {
        let run_id = format!("{goal_run_id}-{action}");
        engine
            .goal_runs
            .lock()
            .await
            .push_back(sample_supervised_goal_run(
                &run_id,
                &format!("task-{action}"),
                &format!("approval-{action}"),
            ));
        sample_awaiting_task(
            &engine,
            &run_id,
            &format!("task-{action}"),
            &format!("approval-{action}"),
        )
        .await;
        assert!(engine.control_goal_run(&run_id, action, None, None).await);
        // Clear the live cache so the next read must come from SQLite.
        engine
            .goal_runs
            .lock()
            .await
            .retain(|item| item.id != run_id);
        let persisted = engine
            .history
            .get_goal_run(&run_id)
            .await
            .expect("persisted goal lookup")
            .expect("goal should be persisted");
        assert_eq!(
            persisted.status, expected,
            "action {action} should persist as {expected:?}"
        );
    }
}

fn sample_role_edit_goal_run(goal_run_id: &str) -> GoalRun {
    // Two-step plan with current_step_index=0 so the suite can exercise the
    // current-step branch (index 0) and the future-step branch (index 1).
    let mut run = sample_supervised_goal_run(goal_run_id, "task-step-0", "approval-0");
    run.status = GoalRunStatus::Running;
    run.awaiting_approval_id = None;
    run.current_step_index = 0;
    run.steps.push(GoalRunStep {
        id: "step-2".to_string(),
        position: 1,
        title: "step-2".to_string(),
        instructions: "step two work".to_string(),
        kind: GoalRunStepKind::Command,
        success_criteria: "step-2 satisfied".to_string(),
        session_id: None,
        status: GoalRunStepStatus::Pending,
        task_id: None,
        summary: None,
        error: None,
        started_at: None,
        completed_at: None,
    });
    run.current_step_owner_profile = Some(sample_owner_profile(
        "active",
        "openai",
        "gpt-5",
        Some("high"),
    ));
    run
}

#[tokio::test]
async fn update_role_replaces_current_step_owner_profile_for_active_step() {
    let goal_run_id = "goal-update-role-current";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.clear();
        goal_runs.push_back(sample_role_edit_goal_run(goal_run_id));
    }

    let replacement = sample_owner_profile("ops", "anthropic", "claude-sonnet-4-6", None);
    let payload = serde_json::to_string(&replacement).expect("serialize payload");
    let changed = engine
        .control_goal_run(goal_run_id, "update-role", Some(0), Some(&payload))
        .await;
    assert!(changed, "current-step update should mutate state");

    let goal = engine.get_goal_run(goal_run_id).await.expect("goal exists");
    assert_eq!(goal.current_step_owner_profile.as_ref(), Some(&replacement));
    assert!(
        goal.step_owner_overrides.is_empty(),
        "current-step path should not leave an override pending"
    );
    assert!(
        goal.events.iter().any(|event| {
            event.phase == "control" && event.message == "owner profile updated for active step"
        }),
        "expected an audit event for active-step role update"
    );
}

#[tokio::test]
async fn update_role_stages_override_for_future_step_without_touching_active_profile() {
    let goal_run_id = "goal-update-role-future";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.clear();
        goal_runs.push_back(sample_role_edit_goal_run(goal_run_id));
    }
    let before = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal exists")
        .current_step_owner_profile
        .clone();

    let future_profile = sample_owner_profile("future", "openai", "gpt-5.4", Some("medium"));
    let payload = serde_json::to_string(&future_profile).expect("serialize payload");
    let changed = engine
        .control_goal_run(goal_run_id, "update-role", Some(1), Some(&payload))
        .await;
    assert!(changed, "future-step update should mutate state");

    let goal = engine.get_goal_run(goal_run_id).await.expect("goal exists");
    assert_eq!(
        goal.current_step_owner_profile, before,
        "future-step edits must not touch the active step's owner profile"
    );
    assert_eq!(
        goal.step_owner_overrides.get(&1),
        Some(&future_profile),
        "future override should be staged for the targeted step"
    );
    assert!(goal.events.iter().any(|event| {
        event.phase == "control" && event.message == "owner profile override staged for future step"
    }));
}

#[tokio::test]
async fn update_role_rejects_past_step_index_and_missing_payload() {
    let goal_run_id = "goal-update-role-rejected";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.clear();
        let mut run = sample_role_edit_goal_run(goal_run_id);
        run.current_step_index = 1;
        // step-0 status reflects already-completed past step
        run.steps[0].status = GoalRunStepStatus::Completed;
        run.steps[0].completed_at = Some(now_millis());
        goal_runs.push_back(run);
    }

    let payload = serde_json::to_string(&sample_owner_profile("x", "openai", "gpt-5", None))
        .expect("serialize payload");

    let no_payload = engine
        .control_goal_run(goal_run_id, "update-role", Some(1), None)
        .await;
    assert!(!no_payload, "missing payload should be a no-op");

    let past = engine
        .control_goal_run(goal_run_id, "update-role", Some(0), Some(&payload))
        .await;
    assert!(!past, "past step should be a no-op");

    let out_of_range = engine
        .control_goal_run(goal_run_id, "update-role", Some(99), Some(&payload))
        .await;
    assert!(!out_of_range, "step beyond plan length should be a no-op");

    let goal = engine.get_goal_run(goal_run_id).await.expect("goal exists");
    assert!(goal.step_owner_overrides.is_empty());
}

#[tokio::test]
async fn block_action_moves_running_goal_run_into_blocked_state() {
    let goal_run_id = "goal-block-running";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.clear();
        let mut run = sample_supervised_goal_run(goal_run_id, "task-step-0", "approval-0");
        run.status = GoalRunStatus::Running;
        run.awaiting_approval_id = None;
        goal_runs.push_back(run);
    }
    let changed = engine
        .control_goal_run(goal_run_id, "block", None, None)
        .await;
    assert!(changed, "block should mutate state");
    let goal = engine.get_goal_run(goal_run_id).await.expect("goal exists");
    assert_eq!(goal.status, GoalRunStatus::Blocked);
    assert!(
        goal.events.iter().any(|event| {
            event.phase == "control" && event.message == "goal run blocked by governance gate"
        }),
        "expected block audit event"
    );
}

#[tokio::test]
async fn resume_action_exits_blocked_state_back_to_running() {
    let goal_run_id = "goal-resume-blocked";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.clear();
        let mut run = sample_supervised_goal_run(goal_run_id, "task-step-0", "approval-0");
        run.status = GoalRunStatus::Blocked;
        run.awaiting_approval_id = None;
        goal_runs.push_back(run);
    }
    let changed = engine
        .control_goal_run(goal_run_id, "resume", None, None)
        .await;
    assert!(changed, "resume should unblock");
    let goal = engine.get_goal_run(goal_run_id).await.expect("goal exists");
    assert_eq!(goal.status, GoalRunStatus::Running);
    assert!(
        goal.events
            .iter()
            .any(|event| { event.phase == "control" && event.message == "goal run unblocked" }),
        "expected unblocked audit event distinct from regular resume"
    );
}

#[tokio::test]
async fn block_action_is_noop_on_terminal_goal_run() {
    let goal_run_id = "goal-block-terminal";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.clear();
        let mut run = sample_supervised_goal_run(goal_run_id, "task-step-0", "approval-0");
        run.status = GoalRunStatus::Completed;
        run.completed_at = Some(now_millis());
        goal_runs.push_back(run);
    }
    let changed = engine
        .control_goal_run(goal_run_id, "block", None, None)
        .await;
    assert!(!changed, "block must not mutate terminal runs");
    let goal = engine.get_goal_run(goal_run_id).await.expect("goal exists");
    assert_eq!(goal.status, GoalRunStatus::Completed);
}

#[tokio::test]
async fn blocked_status_is_not_terminal() {
    // Sanity check: Blocked is reversible by design.
    assert!(!GoalRunStatus::Blocked.is_terminal());
}

#[tokio::test]
async fn blocked_status_roundtrips_through_sqlite() {
    let goal_run_id = "goal-blocked-roundtrip";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.clear();
        let mut run = sample_supervised_goal_run(goal_run_id, "task-step-0", "approval-0");
        run.status = GoalRunStatus::Running;
        run.awaiting_approval_id = None;
        goal_runs.push_back(run);
    }
    assert!(
        engine
            .control_goal_run(goal_run_id, "block", None, None)
            .await
    );
    // Clear the live cache so the next read must come from SQLite.
    engine
        .goal_runs
        .lock()
        .await
        .retain(|item| item.id != goal_run_id);
    let persisted = engine
        .history
        .get_goal_run(goal_run_id)
        .await
        .expect("history lookup")
        .expect("persisted goal");
    assert_eq!(persisted.status, GoalRunStatus::Blocked);
}

#[tokio::test]
async fn update_role_rejected_after_terminal_status() {
    let goal_run_id = "goal-update-role-terminal";
    let (engine, _root) = supervised_engine_with_goal(goal_run_id).await;
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.clear();
        let mut run = sample_role_edit_goal_run(goal_run_id);
        run.status = GoalRunStatus::Completed;
        run.completed_at = Some(now_millis());
        goal_runs.push_back(run);
    }
    let payload = serde_json::to_string(&sample_owner_profile("late", "openai", "gpt-5", None))
        .expect("serialize payload");
    let changed = engine
        .control_goal_run(goal_run_id, "update-role", Some(1), Some(&payload))
        .await;
    assert!(!changed, "terminal goal should not accept role edits");
}
