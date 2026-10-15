use super::*;

fn sample_goal_run(
    id: &str,
    title: &str,
    status: GoalRunStatus,
    updated_at: u64,
    summary: Option<&str>,
) -> GoalRun {
    GoalRun {
        id: id.to_string(),
        title: title.to_string(),
        goal: title.to_string(),
        client_request_id: None,
        status,
        priority: TaskPriority::Normal,
        created_at: updated_at.saturating_sub(100),
        updated_at,
        started_at: Some(updated_at.saturating_sub(50)),
        completed_at: None,
        thread_id: Some(format!("thread-{id}")),
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

fn persisted_thread_summary(
    id: &str,
    title: &str,
    updated_at: u64,
    message_count: usize,
) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        title: title.to_string(),
        updated_at,
        message_count,
        opening_message: None,
        last_messages: Vec::new(),
    }
}

fn concierge_goal_context<I>(goal_runs: I) -> crate::history::ConciergeGoalContext
where
    I: IntoIterator<Item = GoalRun>,
{
    let goal_runs = goal_runs.into_iter().collect::<Vec<_>>();
    crate::history::ConciergeGoalContext {
        latest_goal_run: goal_runs
            .iter()
            .max_by_key(|goal_run| goal_run.updated_at)
            .cloned(),
        running_goal_total: goal_runs
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Running)
            .count(),
        paused_goal_total: goal_runs
            .iter()
            .filter(|goal_run| goal_run.status == GoalRunStatus::Paused)
            .count(),
    }
}

#[tokio::test]
async fn context_summary_gathers_recent_messages_and_goal_snapshot() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let _threads = RwLock::new(HashMap::from([
        (
            "thread-1".to_string(),
            AgentThread {
                id: "thread-1".to_string(),
                agent_name: None,
                title: "Newest".to_string(),
                created_at: 1,
                updated_at: now,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("kickoff scope", now - 8),
                    assistant_message("reply-1", now - 7),
                    AgentMessage::user("msg-2", now - 6),
                    assistant_message("msg-3", now - 5),
                    AgentMessage::user("msg-4", now - 4),
                    assistant_message("msg-5", now - 3),
                    AgentMessage::user("msg-6", now - 2),
                ],
            },
        ),
        (
            "thread-2".to_string(),
            AgentThread {
                id: "thread-2".to_string(),
                agent_name: None,
                title: "Older".to_string(),
                created_at: 1,
                updated_at: now - 1_000,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![assistant_message("old", now - 1_000)],
            },
        ),
    ]));
    let goal_context = concierge_goal_context([
        sample_goal_run(
            "goal-running",
            "Ship concierge perf fix",
            GoalRunStatus::Running,
            now - 2_000,
            Some("Trim the welcome query payload"),
        ),
        sample_goal_run(
            "goal-latest",
            "Stabilize concierge startup",
            GoalRunStatus::Paused,
            now - 1_000,
            Some("Waiting on operator review"),
        ),
        sample_goal_run(
            "goal-paused-2",
            "Archive old briefs",
            GoalRunStatus::Paused,
            now - 1_500,
            Some("Wrap up cleanup"),
        ),
    ]);

    let persisted_threads = [persisted_thread_summary("thread-1", "Newest", now, 7)];
    let context = engine.context_from_goal_context(
        ConciergeDetailLevel::ContextSummary,
        &persisted_threads,
        goal_context,
    );

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].title, "Newest");
    assert_eq!(context.recent_threads[0].opening_message, None);
    assert!(context.recent_threads[0].last_messages.is_empty());
    assert_eq!(context.recent_threads[0].message_count, 7);
    assert_eq!(context.running_goal_total, 1);
    assert_eq!(context.paused_goal_total, 2);
    let latest_goal = context
        .latest_goal_run
        .as_ref()
        .expect("latest goal should be present");
    assert_eq!(latest_goal.label, "Stabilize concierge startup");
    assert_eq!(latest_goal.status, GoalRunStatus::Paused);
    assert_eq!(
        latest_goal.summary.as_deref(),
        Some("Waiting on operator review")
    );
}

#[tokio::test]
async fn context_summary_picks_latest_goal_by_updated_at() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let _threads = RwLock::new(HashMap::from([(
        "thread-1".to_string(),
        thread_with_messages(
            "thread-1",
            "Actual work",
            now,
            vec![AgentMessage::user("kickoff", now - 5)],
        ),
    )]));
    let goal_context = concierge_goal_context([
        sample_goal_run(
            "goal-old",
            "Older goal",
            GoalRunStatus::Running,
            now - 10_000,
            Some("Older summary"),
        ),
        sample_goal_run(
            "goal-new",
            "Newer goal",
            GoalRunStatus::Completed,
            now - 1_000,
            Some("Newer summary"),
        ),
    ]);

    let context =
        engine.context_from_goal_context(ConciergeDetailLevel::ContextSummary, &[], goal_context);

    assert_eq!(
        context
            .latest_goal_run
            .as_ref()
            .map(|goal| goal.label.as_str()),
        Some("Newer goal")
    );
}

#[tokio::test]
async fn context_summary_excludes_goal_threads_but_keeps_goal_metadata() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let _threads = RwLock::new(HashMap::from([
        (
            "thread-real".to_string(),
            thread_with_messages(
                "thread-real",
                "Actual work",
                now - 100,
                vec![user_message("continue regular work", now - 110)],
            ),
        ),
        (
            "thread-goal-root".to_string(),
            thread_with_messages(
                "thread-goal-root",
                "Goal root",
                now,
                vec![user_message("hidden goal prompt copy", now - 20)],
            ),
        ),
        (
            "thread-goal-active".to_string(),
            thread_with_messages(
                "thread-goal-active",
                "Goal active step",
                now - 10,
                vec![user_message("hidden active step", now - 15)],
            ),
        ),
        (
            "thread-goal-exec".to_string(),
            thread_with_messages(
                "thread-goal-exec",
                "Goal execution step",
                now - 20,
                vec![user_message("hidden execution detail", now - 25)],
            ),
        ),
    ]));
    let mut goal_run = sample_goal_run(
        "goal-latest",
        "Trim concierge goal context",
        GoalRunStatus::Running,
        now,
        Some("Plan the context cleanup"),
    );
    goal_run.goal = "Exclude goal-owned threads from concierge welcome history".to_string();
    goal_run.thread_id = Some("thread-goal-root".to_string());
    goal_run.root_thread_id = Some("thread-goal-root".to_string());
    goal_run.active_thread_id = Some("thread-goal-active".to_string());
    goal_run.execution_thread_ids = vec!["thread-goal-exec".to_string()];
    goal_run.steps[0].status = GoalRunStepStatus::Completed;
    goal_run.steps[0].summary = Some("Identified all goal-owned thread IDs".to_string());
    goal_run.steps[0].completed_at = Some(now - 5);
    let goal_context = concierge_goal_context([goal_run]);

    let persisted_threads = [persisted_thread_summary(
        "thread-real",
        "Actual work",
        now - 100,
        1,
    )];
    let context = engine.context_from_goal_context(
        ConciergeDetailLevel::ProactiveTriage,
        &persisted_threads,
        goal_context,
    );

    assert_eq!(
        context
            .recent_threads
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-real"]
    );
    let latest_goal = context
        .latest_goal_run
        .as_ref()
        .expect("goal metadata should still be present");
    assert_eq!(latest_goal.status, GoalRunStatus::Running);
    assert_eq!(
        latest_goal.prompt.as_deref(),
        Some("Exclude goal-owned threads from concierge welcome history")
    );
    assert_eq!(
        latest_goal.latest_step_result.as_deref(),
        Some("Identified all goal-owned thread IDs")
    );
}

#[tokio::test]
async fn context_summary_ignores_assistant_only_concierge_like_threads() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let _threads = RwLock::new(HashMap::from([
        (
            "thread-real".to_string(),
            thread_with_messages(
                "thread-real",
                "Actual work",
                now - 100,
                vec![
                    AgentMessage::user("fix concierge context", now - 120),
                    assistant_message("working on it", now - 110),
                ],
            ),
        ),
        (
            "thread-meta".to_string(),
            thread_with_messages(
                "thread-meta",
                "Concierge",
                now,
                vec![assistant_message("welcome back", now - 10)],
            ),
        ),
    ]));
    let context = engine.context_from_goal_context(
        ConciergeDetailLevel::ContextSummary,
        &[persisted_thread_summary(
            "thread-real",
            "Actual work",
            now - 100,
            2,
        )],
        crate::history::ConciergeGoalContext::default(),
    );

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].id, "thread-real");
}

#[tokio::test]
async fn context_summary_excludes_structured_heartbeat_threads() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let _threads = RwLock::new(HashMap::from([
        (
            "thread-real".to_string(),
            thread_with_messages(
                "thread-real",
                "Actual work",
                now - 100,
                vec![
                    user_message("fix concierge context", now - 120),
                    assistant_message("working on it", now - 110),
                ],
            ),
        ),
        (
            "thread-heartbeat".to_string(),
            thread_with_messages(
                "thread-heartbeat",
                "HEARTBEAT SYNTHESIS",
                now,
                vec![
                    user_message(
                        "HEARTBEAT SYNTHESIS\nYou are performing a scheduled heartbeat check for the operator.",
                        now - 20,
                    ),
                    assistant_message(
                        "ACTIONABLE: false\nDIGEST: All systems normal.\nITEMS:",
                        now - 10,
                    ),
                ],
            ),
        ),
    ]));
    let context = engine.context_from_goal_context(
        ConciergeDetailLevel::ContextSummary,
        &[persisted_thread_summary(
            "thread-real",
            "Actual work",
            now - 100,
            2,
        )],
        crate::history::ConciergeGoalContext::default(),
    );

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].id, "thread-real");
}

#[tokio::test]
async fn context_summary_hides_weles_internal_threads() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let _threads = RwLock::new(HashMap::from([
        (
            "thread-real".to_string(),
            thread_with_messages(
                "thread-real",
                "Actual work",
                now - 100,
                vec![
                    user_message("fix concierge context", now - 120),
                    assistant_message("working on it", now - 110),
                ],
            ),
        ),
        (
            "thread-weles".to_string(),
            thread_with_messages(
                "thread-weles",
                "WELES governance runtime thread",
                now,
                vec![
                    user_message(
                        &crate::agent::agent_identity::build_weles_persona_prompt("governance"),
                        now - 20,
                    ),
                    assistant_message("Internal governance review", now - 10),
                ],
            ),
        ),
    ]));
    let context = engine.context_from_goal_context(
        ConciergeDetailLevel::ContextSummary,
        &[persisted_thread_summary(
            "thread-real",
            "Actual work",
            now - 100,
            2,
        )],
        crate::history::ConciergeGoalContext::default(),
    );

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].id, "thread-real");
}

#[tokio::test]
async fn context_summary_excludes_internal_dm_threads() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::CONCIERGE_AGENT_ID,
    );
    let _threads = RwLock::new(HashMap::from([
        (
            "thread-real".to_string(),
            thread_with_messages(
                "thread-real",
                "Actual work",
                now - 100,
                vec![
                    user_message("fix concierge context", now - 120),
                    assistant_message("working on it", now - 110),
                ],
            ),
        ),
        (
            dm_thread_id.clone(),
            thread_with_messages(
                &dm_thread_id,
                "Internal DM",
                now,
                vec![
                    user_message("Continue Spec 03 work", now - 20),
                    assistant_message("Proceeding internally", now - 10),
                ],
            ),
        ),
    ]));
    let context = engine.context_from_goal_context(
        ConciergeDetailLevel::ContextSummary,
        &[persisted_thread_summary(
            "thread-real",
            "Actual work",
            now - 100,
            2,
        )],
        crate::history::ConciergeGoalContext::default(),
    );

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].id, "thread-real");
}

#[tokio::test]
async fn context_summary_excludes_participant_playground_threads() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let now = test_now_millis();
    let playground_thread_id =
        crate::agent::agent_identity::participant_playground_thread_id("thread-real", "weles");
    let _threads = RwLock::new(HashMap::from([
        (
            "thread-real".to_string(),
            thread_with_messages(
                "thread-real",
                "Actual work",
                now - 100,
                vec![
                    user_message("fix concierge context", now - 120),
                    assistant_message("working on it", now - 110),
                ],
            ),
        ),
        (
            playground_thread_id.clone(),
            thread_with_messages(
                &playground_thread_id,
                "Participant Playground",
                now,
                vec![
                    user_message("hidden drafting prompt", now - 20),
                    assistant_message("hidden drafting response", now - 10),
                ],
            ),
        ),
    ]));
    let context = engine.context_from_goal_context(
        ConciergeDetailLevel::ContextSummary,
        &[persisted_thread_summary(
            "thread-real",
            "Actual work",
            now - 100,
            2,
        )],
        crate::history::ConciergeGoalContext::default(),
    );

    assert_eq!(context.recent_threads.len(), 1);
    assert_eq!(context.recent_threads[0].id, "thread-real");
}

#[tokio::test]
async fn recent_persisted_threads_skip_heartbeats_whose_title_was_rewritten() {
    let config = Arc::new(RwLock::new(AgentConfig::default()));
    let (event_tx, _) = broadcast::channel(8);
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty(),
    ));
    let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new(), circuit_breakers);
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let history = manager.history();
    for (id, title, updated_at, content) in [
        ("thread-real", "Actual work", 1_000, "fix concierge context"),
        (
            "thread-heartbeat",
            "Scheduled Heartbeat and Stale Todo Review",
            2_000,
            "HEARTBEAT SYNTHESIS\nYou are performing a scheduled heartbeat check for the operator.",
        ),
    ] {
        history
            .create_thread(&zorai_protocol::AgentDbThread {
                id: id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: title.to_string(),
                created_at: updated_at,
                updated_at,
                message_count: 1,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await
            .unwrap();
        history
            .add_message(&zorai_protocol::AgentDbMessage {
                id: format!("{id}-msg"),
                thread_id: id.to_string(),
                created_at: updated_at,
                role: "user".to_string(),
                content: content.to_string(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await
            .unwrap();
    }

    let recent = engine.recent_persisted_history_threads(&manager).await;

    assert_eq!(
        recent.iter().map(|thread| thread.id.as_str()).collect::<Vec<_>>(),
        vec!["thread-real"],
        "heartbeat threads get LLM-written titles, so Rarog must recognise them by their prompt and never offer them as the session to continue"
    );
}
