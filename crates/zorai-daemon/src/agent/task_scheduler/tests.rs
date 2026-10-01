use super::*;

fn make_task(id: &str, status: TaskStatus, goal_run_id: Option<&str>) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: format!("task-{id}"),
        description: "scheduler test task".to_string(),
        status,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: 1,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: None,
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: goal_run_id.map(ToString::to_string),
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

fn make_default_config() -> AgentConfig {
    AgentConfig::default()
}

#[test]
fn select_ready_task_indices_excludes_queued_tasks_for_awaiting_approval_goal_runs() {
    let mut tasks = VecDeque::new();
    tasks.push_back(make_task(
        "goal-task",
        TaskStatus::Queued,
        Some("goal-awaiting-approval"),
    ));
    tasks.push_back(make_task("independent-task", TaskStatus::Queued, None));

    let mut goal_run_statuses = HashMap::new();
    goal_run_statuses.insert(
        "goal-awaiting-approval".to_string(),
        GoalRunStatus::AwaitingApproval,
    );
    let selected =
        select_ready_task_indices(&tasks, &[], &goal_run_statuses, &make_default_config());

    assert_eq!(
        selected,
        vec![(1, "daemon-main".to_string())],
        "goal-linked queued task should stay blocked"
    );
}

#[test]
fn select_ready_task_indices_excludes_queued_tasks_for_awaiting_review_goal_runs() {
    let mut tasks = VecDeque::new();
    tasks.push_back(make_task(
        "goal-task",
        TaskStatus::Queued,
        Some("goal-awaiting-review"),
    ));
    tasks.push_back(make_task("independent-task", TaskStatus::Queued, None));

    let mut goal_run_statuses = HashMap::new();
    goal_run_statuses.insert(
        "goal-awaiting-review".to_string(),
        GoalRunStatus::AwaitingReview,
    );
    let selected =
        select_ready_task_indices(&tasks, &[], &goal_run_statuses, &make_default_config());

    assert_eq!(
        selected,
        vec![(1, "daemon-main".to_string())],
        "supervisor-review goal should not dispatch a second worker"
    );
}

#[test]
fn select_ready_task_indices_fail_closed_when_goal_metadata_missing_for_goal_linked_task() {
    let mut tasks = VecDeque::new();
    tasks.push_back(make_task(
        "goal-task",
        TaskStatus::Queued,
        Some("unknown-goal-run"),
    ));
    tasks.push_back(make_task("independent-task", TaskStatus::Queued, None));

    let selected = select_ready_task_indices(&tasks, &[], &HashMap::new(), &make_default_config());

    assert_eq!(
        selected,
        vec![(1, "daemon-main".to_string())],
        "goal-linked queued task should not dispatch when goal status is unavailable"
    );
}

#[test]
fn goal_worker_dispatches_on_its_own_lane_while_the_session_lane_is_busy() {
    let mut chat = make_task("chat-turn", TaskStatus::InProgress, None);
    chat.session_id = Some("sess-1".to_string());
    chat.lane_id = Some("session:sess-1".to_string());

    let mut worker = make_task("goal-worker", TaskStatus::Queued, Some("goal-1"));
    worker.source = "goal_run".to_string();
    worker.session_id = Some("sess-1".to_string());
    worker.parent_thread_id = Some("owner-thread".to_string());
    worker.thread_id = Some("worker-thread".to_string());

    let mut tasks = VecDeque::new();
    tasks.push_back(chat);
    tasks.push_back(worker);
    let mut statuses = HashMap::new();
    statuses.insert("goal-1".to_string(), GoalRunStatus::Running);

    assert_eq!(
        select_ready_task_indices(&tasks, &[], &statuses, &make_default_config()),
        vec![(1, "daemon-subagent:goal-worker".to_string())],
    );
}

#[test]
fn select_ready_task_indices_allows_four_parallel_child_daemon_tasks() {
    let mut tasks = VecDeque::new();
    for id in 0..4 {
        let mut task = make_task(&format!("child-{id}"), TaskStatus::Queued, None);
        task.source = "subagent".to_string();
        task.parent_task_id = Some("parent-task".to_string());
        tasks.push_back(task);
    }

    let selected = select_ready_task_indices(&tasks, &[], &HashMap::new(), &make_default_config());

    assert_eq!(
        selected.len(),
        4,
        "up to four child daemon tasks should dispatch in parallel"
    );

    let lanes = selected
        .into_iter()
        .map(|(_, lane)| lane)
        .collect::<Vec<_>>();
    assert_eq!(
        lanes,
        vec![
            "daemon-subagent:child-0".to_string(),
            "daemon-subagent:child-1".to_string(),
            "daemon-subagent:child-2".to_string(),
            "daemon-subagent:child-3".to_string(),
        ]
    );
}

#[test]
fn select_ready_task_indices_assigns_second_weles_lane_when_first_is_busy() {
    let mut tasks = VecDeque::new();
    let mut active = make_task("weles-active", TaskStatus::InProgress, None);
    active.source = "subagent".to_string();
    active.sub_agent_def_id =
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
    active.lane_id = Some("weles:0".to_string());
    tasks.push_back(active);

    let mut queued = make_task("weles-queued", TaskStatus::Queued, None);
    queued.source = "subagent".to_string();
    queued.sub_agent_def_id =
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
    tasks.push_back(queued);

    let selected = select_ready_task_indices(&tasks, &[], &HashMap::new(), &make_default_config());

    assert_eq!(selected, vec![(1, "weles:1".to_string())]);
}

#[test]
fn select_ready_task_indices_blocks_weles_when_lane_pool_is_exhausted() {
    let mut tasks = VecDeque::new();
    for slot in 0..2 {
        let mut active = make_task(
            &format!("weles-active-{slot}"),
            TaskStatus::InProgress,
            None,
        );
        active.source = "subagent".to_string();
        active.sub_agent_def_id =
            Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
        active.lane_id = Some(format!("weles:{slot}"));
        tasks.push_back(active);
    }

    let mut queued = make_task("weles-queued", TaskStatus::Queued, None);
    queued.source = "subagent".to_string();
    queued.sub_agent_def_id =
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
    tasks.push_back(queued.clone());

    let mut changed = tasks.clone();
    let updates = refresh_task_queue_state(&mut changed, 1, &[], &make_default_config());
    let selected =
        select_ready_task_indices(&changed, &[], &HashMap::new(), &make_default_config());

    assert!(
        selected.is_empty(),
        "queued WELES task should not dispatch once both lanes are busy"
    );
    let blocked = updates
        .into_iter()
        .find(|task| task.id == queued.id)
        .expect("expected blocked task update");
    assert_eq!(
        blocked.blocked_reason.as_deref(),
        Some("waiting for lane availability: weles")
    );
}

#[test]
fn select_ready_task_indices_skips_tasks_for_cancelled_and_paused_goals() {
    let mut tasks = VecDeque::new();
    tasks.push_back(make_task(
        "cancelled-goal-task",
        TaskStatus::Queued,
        Some("goal-cancelled"),
    ));
    tasks.push_back(make_task(
        "paused-goal-task",
        TaskStatus::Queued,
        Some("goal-paused"),
    ));
    tasks.push_back(make_task("independent-task", TaskStatus::Queued, None));

    let mut goal_run_statuses = HashMap::new();
    goal_run_statuses.insert("goal-cancelled".to_string(), GoalRunStatus::Cancelled);
    goal_run_statuses.insert("goal-paused".to_string(), GoalRunStatus::Paused);
    let selected =
        select_ready_task_indices(&tasks, &[], &goal_run_statuses, &make_default_config());

    assert_eq!(
        selected,
        vec![(2, "daemon-main".to_string())],
        "queued tasks for cancelled or paused goals must not dispatch"
    );
}

#[test]
fn refresh_task_queue_state_clears_open_contract_blocks_before_attempt_limit() {
    let mut tasks = VecDeque::new();
    let mut contract_blocked =
        make_task("contract-open", TaskStatus::Blocked, Some("goal-running"));
    contract_blocked.blocked_reason =
        Some("completion contract remains open: required deliverable: marker".to_string());
    contract_blocked.completion_contract = Some(TaskCompletionContract {
        open_completion_attempts: 1,
        ..TaskCompletionContract::default()
    });
    let mut exhausted_contract = make_task(
        "contract-exhausted",
        TaskStatus::Blocked,
        Some("goal-running"),
    );
    exhausted_contract.blocked_reason = Some(
        "completion contract remains open after 3 attempts: required deliverable: marker"
            .to_string(),
    );
    exhausted_contract.completion_contract = Some(TaskCompletionContract {
        open_completion_attempts: OPEN_COMPLETION_CONTRACT_ATTEMPT_LIMIT,
        ..TaskCompletionContract::default()
    });
    let mut paused_blocked = make_task(
        "goal-paused-task",
        TaskStatus::Blocked,
        Some("goal-running"),
    );
    paused_blocked.blocked_reason = Some("goal_paused:\"queued\"".to_string());
    let mut lane_blocked = make_task("lane-wait", TaskStatus::Blocked, None);
    lane_blocked.blocked_reason = Some("waiting for lane availability: daemon-main".to_string());
    tasks.push_back(contract_blocked);
    tasks.push_back(exhausted_contract);
    tasks.push_back(paused_blocked);
    tasks.push_back(lane_blocked);

    let updates = refresh_task_queue_state(&mut tasks, 1, &[], &make_default_config());

    assert_eq!(tasks[0].status, TaskStatus::Queued);
    assert_eq!(tasks[1].status, TaskStatus::Blocked);
    assert_eq!(tasks[2].status, TaskStatus::Blocked);
    assert_eq!(tasks[3].status, TaskStatus::Queued);
    assert!(
        updates
            .iter()
            .any(|task| task.id == "contract-open" && task.status == TaskStatus::Queued),
        "multi-turn goal steps must requeue while the open-completion budget remains"
    );
    assert!(
        updates
            .iter()
            .all(|task| task.id != "contract-exhausted" && task.id != "goal-paused-task"),
        "exhausted contracts and goal-paused blocks must stay blocked"
    );
    assert!(
        updates
            .iter()
            .any(|task| task.id == "lane-wait" && task.status == TaskStatus::Queued),
        "resource-wait blocks should still clear when the lane is free"
    );
}
