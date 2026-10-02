use crate::agent::types::{AgentEvent, MessageRole, TaskStatus};

use super::super::*;
use super::common::*;

#[tokio::test]
async fn apply_halt_retries_blocks_same_pattern_retry_in_same_thread() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-halt";
    seed_runtime(&engine, thread_id).await;
    let scope = scope(thread_id, Some("goal-1"));
    let decision = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop retrying the same failing bash path.".to_string(),
        strategy_hint: None,
        retry_guard: Some("approach-hash-1".to_string()),
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: false,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: false,
        },
    };

    engine.record_policy_decision(&scope, decision, 1_000).await;
    engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &PolicyDecision {
                action: PolicyAction::HaltRetries,
                reason: "Stop retrying the same failing bash path.".to_string(),
                strategy_hint: None,
                retry_guard: Some("approach-hash-1".to_string()),
            },
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("halt retries should apply");

    let outcome = engine
        .enforce_orchestrator_retry_guard(
            thread_id,
            Some("task-1"),
            &scope,
            "approach-hash-1",
            1_010,
        )
        .await
        .expect("retry guard should be enforced");

    assert_eq!(outcome, PolicyLoopAction::AbortRetry);
    let tasks = engine.tasks.lock().await;
    let task = tasks.iter().find(|task| task.id == "task-1").expect("task");
    assert_eq!(task.retry_count, task.max_retries);
}

#[tokio::test]
async fn apply_fresh_halt_retries_marks_task_as_failed_immediately() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-fresh-halt";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop retrying the same failing bash path.".to_string(),
        strategy_hint: None,
        retry_guard: Some("approach-hash-1".to_string()),
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: false,
        },
    };

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("halt retries should apply");

    assert_eq!(outcome, PolicyLoopAction::AbortRetry);
    let tasks = engine.tasks.lock().await;
    let task = tasks.iter().find(|task| task.id == "task-1").expect("task");
    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.retry_count, task.max_retries);
    assert_eq!(
        task.blocked_reason.as_deref(),
        Some("policy halted repeated retry")
    );
    assert_eq!(
        task.last_error.as_deref(),
        Some("policy halted repeated retry")
    );
    assert!(task.completed_at.is_some());
    assert!(task
        .logs
        .iter()
        .any(|entry| entry.message.contains("policy halted repeated retry")));
}

#[tokio::test]
async fn apply_fresh_halt_retries_marks_persisted_task_after_live_queue_clear() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-fresh-halt-persisted-task";
    seed_runtime(&engine, thread_id).await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();
    let decision = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop retrying the same failing bash path.".to_string(),
        strategy_hint: None,
        retry_guard: Some("approach-hash-1".to_string()),
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: false,
        },
    };

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("halt retries should apply");

    assert_eq!(outcome, PolicyLoopAction::AbortRetry);
    let persisted = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some("task-1".to_string()),
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
    assert_eq!(persisted.status, TaskStatus::Failed);
    assert_eq!(persisted.retry_count, persisted.max_retries);
    assert_eq!(
        persisted.blocked_reason.as_deref(),
        Some("policy halted repeated retry")
    );
    assert_eq!(
        persisted.last_error.as_deref(),
        Some("policy halted repeated retry")
    );
    assert!(persisted.completed_at.is_some());
    assert!(persisted
        .logs
        .iter()
        .any(|entry| entry.message.contains("policy halted repeated retry")));
}

#[tokio::test]
async fn apply_pivot_routes_into_existing_strategy_refresh_behavior() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-pivot";
    seed_runtime(&engine, thread_id).await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();
    engine
        .history
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE goal_run_steps SET started_at = 'not-an-integer' WHERE goal_run_id = ?1",
                rusqlite::params!["goal-1"],
            )?;
            Ok(())
        })
        .await
        .expect("corrupt hydration-only step column");
    let decision = PolicyDecision {
        action: PolicyAction::Pivot,
        reason: "The repeated failures justify a different strategy.".to_string(),
        strategy_hint: Some("Inspect state before running the same command again.".to_string()),
        retry_guard: Some("approach-hash-1".to_string()),
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: false,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: true,
            should_escalate: false,
        },
    };

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("pivot should apply");

    assert_eq!(outcome, PolicyLoopAction::RestartLoop);
    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread");
    let injected = thread
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message.content.contains("Investigate failure")
                && message
                    .content
                    .contains("Inspect state before running the same command again")
        })
        .expect("strategy refresh prompt");
    assert!(injected.content.contains("Fallback strategy"));
    assert!(
        injected.content.contains("Spawn a sub-agent"),
        "pivot should use persisted retry_count=1 when choosing the refresh strategy: {}",
        injected.content
    );
}

#[tokio::test]
async fn apply_escalate_routes_into_existing_escalation_behavior() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-escalate";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::Escalate,
        reason: "Repeated failures need operator guidance now.".to_string(),
        strategy_hint: None,
        retry_guard: None,
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: true,
        },
    };
    let mut events = engine.subscribe();

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("escalation should apply");

    assert_eq!(outcome, PolicyLoopAction::InterruptForApproval);
    let tasks = engine.tasks.lock().await;
    let task = tasks.iter().find(|task| task.id == "task-1").expect("task");
    assert_eq!(task.status, TaskStatus::AwaitingApproval);
    assert!(task.awaiting_approval_id.is_some());
    drop(tasks);

    let (saw_escalation_update, saw_audit_action) =
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            let mut saw_escalation_update = false;
            let mut saw_audit_action = false;
            while !saw_escalation_update || !saw_audit_action {
                let event = events.recv().await.expect("event");
                match event {
                    AgentEvent::EscalationUpdate {
                        thread_id: event_thread_id,
                        ..
                    } if event_thread_id == thread_id => {
                        saw_escalation_update = true;
                    }
                    AgentEvent::AuditAction {
                        thread_id: Some(event_thread_id),
                        ..
                    } if event_thread_id == thread_id => {
                        saw_audit_action = true;
                    }
                    _ => {}
                }
            }
            (saw_escalation_update, saw_audit_action)
        })
        .await
        .expect("expected escalation update and audit action");
    assert!(saw_escalation_update);
    assert!(saw_audit_action);
}

#[tokio::test]
async fn apply_escalate_reuses_session_approval_for_later_same_thread_escalations() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-escalate-session-approval";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::Escalate,
        reason: "Repeated failures need operator guidance now.".to_string(),
        strategy_hint: None,
        retry_guard: None,
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: true,
        },
    };

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("initial escalation should apply");
    assert_eq!(outcome, PolicyLoopAction::InterruptForApproval);

    let approval_id = {
        let tasks = engine.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.id == "task-1")
            .and_then(|task| task.awaiting_approval_id.clone())
            .expect("task should await approval")
    };

    assert!(
        engine
            .handle_task_approval_resolution(
                &approval_id,
                zorai_protocol::ApprovalDecision::ApproveSession
            )
            .await
    );

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_010,
        )
        .await
        .expect("session-approved escalation should not block");
    assert_eq!(outcome, PolicyLoopAction::Continue);

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "task-1")
        .cloned()
        .expect("task");
    assert_ne!(task.status, TaskStatus::AwaitingApproval);
    assert!(task.awaiting_approval_id.is_none());
}

#[tokio::test]
async fn apply_escalate_reuses_saved_always_approve_rule() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-always-approve";
    seed_runtime(&engine, thread_id).await;
    engine
        .task_approval_rules
        .write()
        .await
        .push(zorai_protocol::TaskApprovalRule {
            id: "rule-1".to_string(),
            command: "orchestrator_policy_escalation".to_string(),
            created_at: 1,
            last_used_at: None,
            use_count: 0,
        });
    let decision = PolicyDecision {
        action: PolicyAction::Escalate,
        reason: "Repeated failures need operator guidance now.".to_string(),
        strategy_hint: None,
        retry_guard: None,
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: true,
        },
    };

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("saved rule escalation should apply");
    assert_eq!(outcome, PolicyLoopAction::Continue);

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "task-1")
        .cloned()
        .expect("task");
    assert_ne!(task.status, TaskStatus::AwaitingApproval);
    assert!(task.awaiting_approval_id.is_none());

    let rule = engine
        .list_task_approval_rules()
        .await
        .into_iter()
        .find(|rule| rule.id == "rule-1")
        .expect("saved rule should remain");
    assert_eq!(rule.use_count, 1);
    assert!(rule.last_used_at.is_some());
}

#[tokio::test]
async fn apply_continue_leaves_current_flow_unchanged() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-continue";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::Continue,
        reason: String::new(),
        strategy_hint: None,
        retry_guard: None,
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: false,
        awareness_stuck: false,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: false,
        },
    };

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("continue should apply");

    assert_eq!(outcome, PolicyLoopAction::Continue);
    let threads = engine.threads.read().await;
    assert_eq!(threads.get(thread_id).expect("thread").messages.len(), 1);
}

#[tokio::test]
async fn apply_pivot_uses_actual_trigger_context_when_refreshing_strategy() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-awareness-pivot";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::Pivot,
        reason: "Low progress suggests a context refresh.".to_string(),
        strategy_hint: Some("Re-check state before doing more work.".to_string()),
        retry_guard: None,
    };
    let trigger = awareness_only_trigger(thread_id);

    let outcome = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            Some("goal-1"),
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("pivot should apply");

    assert_eq!(outcome, PolicyLoopAction::RestartLoop);
    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread");
    let injected = thread
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::System
                && message
                    .content
                    .contains("Re-check state before doing more work.")
        })
        .expect("strategy refresh prompt");
    assert!(injected
        .content
        .contains("Spawn a sub-agent with expertise"));
    assert!(!injected.content.contains("Disable the following tools"));
}

#[tokio::test]
async fn apply_reused_pivot_does_not_reinject_strategy_refresh_message() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-reuse-no-duplicate";
    seed_runtime(&engine, thread_id).await;
    let decision = PolicyDecision {
        action: PolicyAction::Pivot,
        reason: "Switch away from the repeating failure.".to_string(),
        strategy_hint: Some("Inspect the workspace before running commands again.".to_string()),
        retry_guard: Some("approach-hash-1".to_string()),
    };
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: None,
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: true,
            should_escalate: false,
        },
    };

    let first = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            None,
            &trigger,
            &decision,
            PolicyDecisionSource::FreshEvaluation,
            1_000,
        )
        .await
        .expect("fresh pivot should apply");
    assert_eq!(first, PolicyLoopAction::RestartLoop);

    let replan_count = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread")
            .messages
            .iter()
            .filter(|message| {
                message.role == MessageRole::System && message.content.starts_with("[REPLAN]")
            })
            .count()
    };
    assert_eq!(
        replan_count, 1,
        "fresh pivot should inject one strategy refresh message"
    );

    let reused = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some("task-1"),
            None,
            &trigger,
            &decision,
            PolicyDecisionSource::ReusedRecent,
            1_010,
        )
        .await
        .expect("reused pivot should still restart the loop");
    assert_eq!(reused, PolicyLoopAction::RestartLoop);

    let replan_count_after_reuse = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread")
            .messages
            .iter()
            .filter(|message| {
                message.role == MessageRole::System && message.content.starts_with("[REPLAN]")
            })
            .count()
    };
    assert_eq!(
        replan_count_after_reuse, 1,
        "reused pivot must not inject duplicate strategy refresh messages"
    );
}
