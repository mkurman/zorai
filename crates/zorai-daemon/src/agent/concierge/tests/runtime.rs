use super::*;
use crate::agent::now_millis;

fn sample_goal_run_for_welcome(
    goal_run_id: &str,
    title: &str,
    status: GoalRunStatus,
    updated_at: u64,
    summary: Option<&str>,
) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: title.to_string(),
        goal: title.to_string(),
        client_request_id: None,
        status,
        priority: TaskPriority::Normal,
        created_at: updated_at.saturating_sub(100),
        updated_at,
        started_at: Some(updated_at.saturating_sub(50)),
        completed_at: None,
        thread_id: Some(format!("thread-{goal_run_id}")),
        root_thread_id: None,
        supervision_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("step-1".to_string()),
        current_step_kind: Some(GoalRunStepKind::Research),
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 0,
        max_replans: 2,
        plan_summary: summary.map(str::to_string),
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
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "step-1".to_string(),
            instructions: "inspect".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "done".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Pending,
            task_id: None,
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        }],
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: crate::agent::AutonomyLevel::Aware,
        authorship_tag: None,
    }
}

fn sample_goal_run_with_kind(
    goal_run_id: &str,
    kind: GoalRunStepKind,
    instructions: &str,
) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "goal with custom step".to_string(),
        goal: "validate custom step routing".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
        priority: TaskPriority::Normal,
        created_at: now_millis(),
        updated_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        thread_id: Some("thread-goal-custom".to_string()),
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("step-1".to_string()),
        current_step_kind: Some(kind.clone()),
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
            title: "step-1".to_string(),
            instructions: instructions.to_string(),
            kind,
            success_criteria: "done".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Pending,
            task_id: None,
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        }],
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: crate::agent::AutonomyLevel::Aware,
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        supervision_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    }
}

#[tokio::test]
async fn generate_welcome_survives_low_confidence_goal_plan_approval_resume() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.concierge.initialize(&engine.threads).await;

    let goal_run_id = "goal-low-confidence-plan";
    let approval_id = "goal-plan-approval-1";
    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Research,
        "Inspect the deployment before taking action",
    );
    goal_run.status = GoalRunStatus::AwaitingApproval;
    goal_run.awaiting_approval_id = Some(approval_id.to_string());
    goal_run.active_task_id = Some("approval-task".to_string());
    goal_run.thread_id = Some("thread-low-confidence-plan".to_string());
    engine.goal_runs.lock().await.push_back(goal_run);

    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task".to_string(),
        title: "Review low-confidence goal plan".to_string(),
        description: "Review low-confidence goal plan".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some("thread-low-confidence-plan".to_string()),
        source: "goal_plan_approval".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
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
            .await,
        "approval resolution should succeed for low-confidence plan reviews"
    );

    let welcome = engine
        .concierge
        .generate_welcome(&engine.threads, &engine.history)
        .await
        .expect("welcome should be returned after approval resume");

    assert!(
        !welcome.0.trim().is_empty(),
        "welcome should still render non-empty content after approval resume"
    );
}

#[tokio::test]
async fn concierge_recovery_deduplicates_inflight_investigations_per_thread_signature() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let first = engine
        .concierge
        .maybe_start_recovery_investigation(
            &engine,
            "thread-recovery",
            "request-invalid-empty-tool-name",
            "request_invalid",
            "invalid request body",
            &serde_json::json!({"raw_message": "Invalid 'input[12].name': empty string"}),
        )
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let second = engine
        .concierge
        .maybe_start_recovery_investigation(
            &engine,
            "thread-recovery",
            "request-invalid-empty-tool-name",
            "request_invalid",
            "invalid request body",
            &serde_json::json!({"raw_message": "Invalid 'input[12].name': empty string"}),
        )
        .await;

    assert!(first.is_some());
    assert!(second.is_none());

    let first_task_id = first.as_deref().expect("first recovery task id");
    let tasks = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(first_task_id.to_string()),
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
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source, "concierge_recovery");
    assert_eq!(
        tasks[0].parent_thread_id.as_deref(),
        Some("thread-recovery")
    );
    assert_eq!(
        tasks[0].sub_agent_def_id.as_deref(),
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    );
    assert!(
        tasks[0]
            .override_system_prompt
            .as_deref()
            .is_some_and(|prompt| prompt.contains(crate::agent::agent_identity::WELES_AGENT_ID)),
        "recovery investigation should be owned by daemon WELES"
    );
}

#[tokio::test]
async fn generate_welcome_uses_latest_goal_summary_and_running_paused_counts() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.concierge.initialize(&engine.threads).await;

    let now = now_millis();
    for goal_run in [
        sample_goal_run_for_welcome(
            "goal-old-running",
            "Ship concierge perf fix",
            GoalRunStatus::Running,
            now - 5_000,
            Some("Trim the welcome query payload"),
        ),
        sample_goal_run_for_welcome(
            "goal-latest-paused",
            "Stabilize concierge startup",
            GoalRunStatus::Paused,
            now - 1_000,
            Some("Waiting on operator review"),
        ),
        sample_goal_run_for_welcome(
            "goal-other-paused",
            "Archive old briefs",
            GoalRunStatus::Paused,
            now - 2_000,
            Some("Wrap up cleanup"),
        ),
    ] {
        engine.history.upsert_goal_run(&goal_run).await.unwrap();
    }

    let welcome = engine
        .concierge
        .generate_welcome(&engine.threads, &engine.history)
        .await
        .expect("welcome should be returned");

    assert!(
        welcome.0.contains("Stabilize concierge startup"),
        "welcome should mention the latest goal title"
    );
    assert!(
        welcome.0.to_ascii_lowercase().contains("paused"),
        "welcome should mention the latest goal status"
    );
    assert!(
        welcome.0.contains("Waiting on operator review"),
        "welcome should include the latest goal summary"
    );
    assert!(
        welcome.0.contains("1 running"),
        "welcome should report running goal count"
    );
    assert!(
        welcome.0.contains("2 paused"),
        "welcome should report paused goal count"
    );
}

#[tokio::test]
async fn prune_welcome_messages_removes_all_concierge_welcomes() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let threads = RwLock::new(HashMap::from([(
        CONCIERGE_THREAD_ID.to_string(),
        concierge_thread(vec![
            assistant_message("hello", 1),
            AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("welcome 1", 2)
            },
            AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("welcome 2", 3)
            },
        ]),
    )]));

    engine.prune_welcome_messages(&threads).await;

    let guard = threads.read().await;
    let thread = guard.get(CONCIERGE_THREAD_ID).unwrap();
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].content, "hello");
}

#[tokio::test]
async fn prune_welcome_messages_clears_welcome_cache() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let action = ConciergeAction {
        label: "Dismiss".to_string(),
        action_type: ConciergeActionType::DismissWelcome,
        thread_id: None,
    };
    engine.cache_welcome("sig", "cached", &[action]).await;
    assert!(engine.cached_welcome("sig").await.is_some());

    let threads = RwLock::new(HashMap::<String, AgentThread>::new());
    engine.prune_welcome_messages(&threads).await;
    assert!(engine.cached_welcome("sig").await.is_none());
}

#[tokio::test]
async fn generate_welcome_reuses_recent_persisted_welcome_without_new_user_message() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            CONCIERGE_THREAD_ID.to_string(),
            concierge_thread(vec![AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("persisted welcome", now - 60_000)
            }]),
        ),
        (
            "thread-1".to_string(),
            thread_with_messages(
                "thread-1",
                "Thread One",
                now - 120_000,
                vec![assistant_message("old reply", now - 120_000)],
            ),
        ),
    ]));
    let root = tempdir().unwrap();
    let history = crate::history::HistoryStore::new_test_store(root.path())
        .await
        .unwrap();

    let result = engine
        .generate_welcome(&threads, &history)
        .await
        .expect("welcome should be returned");
    assert_eq!(result.0, "persisted welcome");
}

#[tokio::test]
async fn generate_welcome_regenerates_when_user_messaged_after_welcome() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            CONCIERGE_THREAD_ID.to_string(),
            concierge_thread(vec![AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("persisted welcome", now - 60_000)
            }]),
        ),
        (
            "thread-1".to_string(),
            thread_with_messages(
                "thread-1",
                "Thread One",
                now - 30_000,
                vec![AgentMessage::user("new user message", now - 30_000)],
            ),
        ),
    ]));
    let root = tempdir().unwrap();
    let history = crate::history::HistoryStore::new_test_store(root.path())
        .await
        .unwrap();

    let result = engine
        .generate_welcome(&threads, &history)
        .await
        .expect("welcome should be returned");
    assert_ne!(result.0, "persisted welcome");
}

#[tokio::test]
async fn generate_welcome_reuses_persisted_welcome_when_only_heartbeat_ran_after() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            CONCIERGE_THREAD_ID.to_string(),
            concierge_thread(vec![AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("persisted welcome", now - 60_000)
            }]),
        ),
        (
            "thread-heartbeat".to_string(),
            thread_with_messages(
                "thread-heartbeat",
                "HEARTBEAT SYNTHESIS",
                now - 10_000,
                vec![
                    user_message(
                        "HEARTBEAT SYNTHESIS\nYou are performing a scheduled heartbeat check for the operator.",
                        now - 10_000,
                    ),
                    assistant_message(
                        "ACTIONABLE: false\nDIGEST: All systems normal.\nITEMS:",
                        now - 9_000,
                    ),
                ],
            ),
        ),
    ]));
    let root = tempdir().unwrap();
    let history = crate::history::HistoryStore::new_test_store(root.path())
        .await
        .unwrap();

    let result = engine
        .generate_welcome(&threads, &history)
        .await
        .expect("welcome should be returned");
    assert_eq!(result.0, "persisted welcome");
}

#[tokio::test]
async fn initialize_preserves_recent_welcome_so_restart_reuses_it_without_regenerating() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let welcome_timestamp = now - 60_000;
    let threads = RwLock::new(HashMap::from([(
        CONCIERGE_THREAD_ID.to_string(),
        concierge_thread(vec![
            user_message("leftover hydrated message", now - 120_000),
            AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("persisted welcome", welcome_timestamp)
            },
        ]),
    )]));

    engine.initialize(&threads).await;

    {
        let threads_guard = threads.read().await;
        let thread = threads_guard.get(CONCIERGE_THREAD_ID).unwrap();
        assert_eq!(
            thread.messages.len(),
            1,
            "initialize should keep only the recent welcome from the hydrated thread"
        );
        assert_eq!(thread.messages[0].content, "persisted welcome");
    }

    let root = tempdir().unwrap();
    let history = crate::history::HistoryStore::new_test_store(root.path())
        .await
        .unwrap();
    let result = engine
        .generate_welcome(&threads, &history)
        .await
        .expect("welcome should be returned");
    assert_eq!(
        result.0, "persisted welcome",
        "a welcome younger than the reuse window must survive a daemon restart instead of triggering a fresh generation"
    );

    let threads_guard = threads.read().await;
    let thread = threads_guard.get(CONCIERGE_THREAD_ID).unwrap();
    assert_eq!(
        thread.messages[0].timestamp, welcome_timestamp,
        "reuse must keep the original creation timestamp so the reuse window measures real age, not last reconnect"
    );
}

#[tokio::test]
async fn initialize_clears_welcome_older_than_reuse_window() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([(
        CONCIERGE_THREAD_ID.to_string(),
        concierge_thread(vec![AgentMessage {
            provider: Some("concierge".into()),
            ..assistant_message("stale welcome", now - WELCOME_REUSE_WINDOW_MS - 1)
        }]),
    )]));

    engine.initialize(&threads).await;

    let threads_guard = threads.read().await;
    let thread = threads_guard.get(CONCIERGE_THREAD_ID).unwrap();
    assert!(
        thread.messages.is_empty(),
        "a welcome past the reuse window is stale and must not be offered for reuse after restart"
    );
}

#[tokio::test]
async fn generate_welcome_regenerates_when_persisted_welcome_is_stale() {
    let mut config_value = AgentConfig::default();
    config_value.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let config = Arc::new(RwLock::new(config_value));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let threads = RwLock::new(HashMap::from([
        (
            CONCIERGE_THREAD_ID.to_string(),
            concierge_thread(vec![AgentMessage {
                provider: Some("concierge".into()),
                ..assistant_message("persisted welcome", now - WELCOME_REUSE_WINDOW_MS - 1)
            }]),
        ),
        (
            "thread-1".to_string(),
            thread_with_messages(
                "thread-1",
                "Thread One",
                now - 30_000,
                vec![assistant_message("old reply", now - 30_000)],
            ),
        ),
    ]));
    let root = tempdir().unwrap();
    let history = crate::history::HistoryStore::new_test_store(root.path())
        .await
        .unwrap();

    let result = engine
        .generate_welcome(&threads, &history)
        .await
        .expect("welcome should be returned");
    assert_ne!(result.0, "persisted welcome");
}

#[tokio::test]
async fn welcome_waits_for_setup_and_keeps_onboarding_for_after_it() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.model = String::new();
    config.tier.onboarding_completed = false;
    config.concierge.detail_level = ConciergeDetailLevel::Minimal;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.concierge.initialize(&engine.threads).await;
    let mut events = engine.subscribe();

    engine.request_concierge_welcome().await;

    assert!(
        events.try_recv().is_err(),
        "no welcome should be shown before a provider and model are configured"
    );
    assert!(
        !engine.config.read().await.tier.onboarding_completed,
        "onboarding must still be pending so it runs once setup is done"
    );

    engine.config.write().await.model = "gpt-5".to_string();
    engine.request_concierge_welcome().await;

    let mut welcomed = false;
    while let Ok(event) = events.try_recv() {
        welcomed |= matches!(event, AgentEvent::ConciergeWelcome { .. });
    }
    assert!(welcomed, "welcome should appear once setup is complete");
    assert!(engine.config.read().await.tier.onboarding_completed);
}
