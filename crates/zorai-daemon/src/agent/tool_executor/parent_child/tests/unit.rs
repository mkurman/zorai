use super::super::{notes_cursor_after_eviction, select_open_ask};
use super::*;
use crate::agent::task_scheduler::refresh_task_queue_state;
use crate::agent::types::AgentConfig;
use std::collections::VecDeque;

#[test]
fn select_open_ask_requires_ask_id_when_several_are_outstanding() {
    let record = |question: &str| AskParentRecord {
        question: question.to_string(),
        options: Vec::new(),
        asked_at: 1,
        timeout_minutes: 1,
        default: None,
        state: "open".to_string(),
        answer: None,
        answer_delivered: false,
        slug: String::new(),
    };
    let open = vec![
        ("ask_parent:child:one".to_string(), record("First?")),
        ("ask_parent:child:two".to_string(), record("Second?")),
    ];
    let error = select_open_ask(&open, None).expect_err("ambiguous answer");
    assert!(error.to_string().contains("ask_id"));
    let selected = select_open_ask(&open, Some("two")).expect("specific ask");
    assert_eq!(selected.0, "ask_parent:child:two");
    let mut slugged = open.clone();
    slugged[0].1.slug = "q1".to_string();
    let by_slug = select_open_ask(&slugged, Some("Q1")).expect("slug is case-insensitive");
    assert_eq!(by_slug.0, "ask_parent:child:one");
}

#[test]
fn notes_cursor_rewinds_when_the_oldest_note_is_evicted() {
    assert_eq!(notes_cursor_after_eviction(20, 1), 19);
    assert_eq!(notes_cursor_after_eviction(0, 1), 0);
}

#[test]
fn ask_timeout_action_covers_open_default_and_unanswered_paths() {
    let now: u64 = 10_000_000;
    let base = |state: &str, default: Option<&str>, asked_at: u64| AskParentRecord {
        question: "q".to_string(),
        options: Vec::new(),
        asked_at,
        timeout_minutes: 60,
        default: default.map(ToOwned::to_owned),
        state: state.to_string(),
        answer: None,
        answer_delivered: false,
        slug: String::new(),
    };

    assert_eq!(
        ask_timeout_action(&base("open", Some("d"), now), now),
        AskTimeoutAction::None,
        "before the deadline nothing fires"
    );
    assert_eq!(
        ask_timeout_action(&base("open", Some("d"), now - 3_600_000), now + 1),
        AskTimeoutAction::TimeoutDefaulted,
        "due with default auto-answers"
    );
    assert_eq!(
        ask_timeout_action(&base("open", None, now - 3_600_000), now + 1),
        AskTimeoutAction::TimeoutUnanswered,
        "due without default proceeds with judgment"
    );
    assert_eq!(
        ask_timeout_action(&base("open", Some("  "), now - 3_600_000), now + 1),
        AskTimeoutAction::TimeoutUnanswered,
        "blank default counts as absent"
    );
    assert_eq!(
        ask_timeout_action(&base("answered", Some("d"), now - 3_600_000), now + 1),
        AskTimeoutAction::None,
        "resolved records never re-fire"
    );
}

#[test]
fn awaiting_parent_prefix_exempts_only_awaiting_parent_children_from_stalled_recovery() {
    let base = |blocked_reason: Option<&str>| AgentTask {
        id: "task-x".to_string(),
        title: "t".to_string(),
        description: "d".to_string(),
        status: TaskStatus::Blocked,
        priority: Default::default(),
        progress: 40,
        created_at: 0,
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
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: blocked_reason.map(ToOwned::to_owned),
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
    };

    assert!(task_is_awaiting_parent(&base(Some(
        "awaiting parent: Which schema?"
    ))));
    assert!(!task_is_awaiting_parent(&base(Some(
        "stuck_needs_recovery"
    ))));
    assert!(!task_is_awaiting_parent(&base(Some(
        "waiting for operator approval: cargo"
    ))));
    assert!(!task_is_awaiting_parent(&base(None)));

    let mut queued = VecDeque::from(vec![base(Some("awaiting parent: Which schema?"))]);
    let changed = refresh_task_queue_state(&mut queued, 100, &[], &AgentConfig::default());
    assert!(
        changed.is_empty(),
        "queue refresh must not treat an open ask_parent as a cleared gate"
    );
    assert_eq!(queued[0].status, TaskStatus::Blocked);
    assert_eq!(
        queued[0].blocked_reason.as_deref(),
        Some("awaiting parent: Which schema?")
    );
}

#[test]
fn queue_refresh_does_not_rewrite_awaiting_parent_when_nested_subagents_are_live() {
    let awaiting = |id: &str| AgentTask {
        id: id.to_string(),
        title: "t".to_string(),
        description: "d".to_string(),
        status: TaskStatus::Blocked,
        priority: Default::default(),
        progress: 40,
        created_at: 0,
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
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("awaiting parent: Which schema?".to_string()),
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
    };
    let nested = |id: &str, parent: &str, status: TaskStatus| {
        let mut task = awaiting(id);
        task.status = status;
        task.blocked_reason = None;
        task.parent_task_id = Some(parent.to_string());
        task
    };

    let mut queued = VecDeque::from(vec![
        awaiting("task-x"),
        nested("nested-1", "task-x", TaskStatus::InProgress),
    ]);
    let changed = refresh_task_queue_state(&mut queued, 100, &[], &AgentConfig::default());
    assert!(
        changed.is_empty(),
        "live nested subagents must not replace an open ask_parent gate"
    );
    assert_eq!(queued[0].status, TaskStatus::Blocked);
    assert_eq!(
        queued[0].blocked_reason.as_deref(),
        Some("awaiting parent: Which schema?"),
        "awaiting-parent prefix must stay so stalled-turn exemption still applies"
    );

    queued[1].status = TaskStatus::Completed;
    let changed = refresh_task_queue_state(&mut queued, 100, &[], &AgentConfig::default());
    assert!(
        changed.is_empty(),
        "finishing nested subagents must not re-queue a child that still has an open ask"
    );
    assert_eq!(queued[0].status, TaskStatus::Blocked);
    assert_eq!(
        queued[0].blocked_reason.as_deref(),
        Some("awaiting parent: Which schema?")
    );
}
