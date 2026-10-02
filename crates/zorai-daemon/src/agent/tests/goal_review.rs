use super::*;
use crate::session_manager::SessionManager;
use std::sync::Arc;
use tempfile::tempdir;

async fn test_engine() -> Arc<AgentEngine> {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await
}

async fn start_supervised_goal(engine: &AgentEngine) -> GoalRun {
    let (owner, _) = engine
        .get_or_create_thread_with_target(Some("owner-supervisor"), "Owner thread", None)
        .await;
    engine
        .start_goal_run(
            "Ship the worker-supervisor protocol".to_string(),
            Some("Protocol goal".to_string()),
            Some(owner),
            None,
            Some("normal"),
            None,
            None,
            None,
        )
        .await
}

async fn enqueue_worker(engine: &AgentEngine, goal_run_id: &str) -> AgentTask {
    engine
        .enqueue_goal_worker(goal_run_id)
        .await
        .expect("enqueue worker");
    let goal = engine.get_goal_run(goal_run_id).await.expect("goal");
    let task_id = goal.active_task_id.clone().expect("worker task");
    engine
        .task_by_id_for_dispatcher(&task_id)
        .await
        .expect("worker task loaded")
}

#[tokio::test]
async fn start_enqueues_one_worker_and_pins_owner_when_distinct() {
    let engine = test_engine().await;
    let goal = start_supervised_goal(&engine).await;
    let worker = enqueue_worker(&engine, &goal.id).await;

    assert_eq!(worker.source, "goal_run");
    assert_eq!(worker.goal_run_id.as_deref(), Some(goal.id.as_str()));
    assert_eq!(worker.parent_thread_id, goal.supervision_thread_id);
    assert_ne!(goal.thread_id, goal.supervision_thread_id);

    let owner_id = goal.supervision_thread_id.clone().expect("owner thread");
    let threads = engine.threads.read().await;
    let owner = threads.get(&owner_id).expect("owner thread loaded");
    assert!(owner.pinned, "owner supervisor thread should stay pinned");
}

#[tokio::test]
async fn request_goal_review_blocks_worker_and_sets_awaiting_review() {
    let engine = test_engine().await;
    let goal = start_supervised_goal(&engine).await;
    let worker = enqueue_worker(&engine, &goal.id).await;

    let updated = engine
        .request_goal_review(&goal.id, &worker.id, "Delivered the protocol change.")
        .await
        .expect("review request");
    assert_eq!(updated.status, GoalRunStatus::AwaitingReview);
    assert_eq!(
        updated.pending_review_report.as_deref(),
        Some("Delivered the protocol change.")
    );

    let blocked = engine
        .task_by_id_for_dispatcher(&worker.id)
        .await
        .expect("worker");
    assert_eq!(blocked.status, TaskStatus::Blocked);
    assert!(blocked
        .blocked_reason
        .as_deref()
        .is_some_and(|reason| reason.starts_with(AWAITING_SUPERVISOR_BLOCKED_PREFIX)));
}

#[tokio::test]
async fn accept_completes_goal_stops_worker_and_unpins_owner() {
    let engine = test_engine().await;
    let goal = start_supervised_goal(&engine).await;
    let worker = enqueue_worker(&engine, &goal.id).await;
    engine
        .request_goal_review(&goal.id, &worker.id, "Done.")
        .await
        .expect("review");

    let owner_id = goal.supervision_thread_id.clone().expect("owner");
    let updated = engine
        .submit_goal_review(&goal.id, GoalSupervisorVerdict::Accept, "", Some(&owner_id))
        .await
        .expect("accept");
    assert_eq!(updated.status, GoalRunStatus::Completed);

    let worker = engine
        .task_by_id_for_dispatcher(&worker.id)
        .await
        .expect("worker");
    assert_eq!(worker.status, TaskStatus::Cancelled);
    let threads = engine.threads.read().await;
    assert!(!threads.get(&owner_id).expect("owner").pinned);
}

#[tokio::test]
async fn soft_reject_continues_same_worker_with_explanation() {
    let engine = test_engine().await;
    let goal = start_supervised_goal(&engine).await;
    let worker = enqueue_worker(&engine, &goal.id).await;
    engine
        .request_goal_review(&goal.id, &worker.id, "Claimed done.")
        .await
        .expect("review");

    let owner_id = goal.supervision_thread_id.clone().expect("owner");
    let updated = engine
        .submit_goal_review(
            &goal.id,
            GoalSupervisorVerdict::SoftReject,
            "Missing tests.",
            Some(&owner_id),
        )
        .await
        .expect("soft reject");
    assert_eq!(updated.status, GoalRunStatus::Running);
    assert_eq!(updated.active_task_id.as_deref(), Some(worker.id.as_str()));

    let continued = engine
        .task_by_id_for_dispatcher(&worker.id)
        .await
        .expect("worker");
    assert_eq!(continued.status, TaskStatus::Queued);
    assert!(continued.blocked_reason.is_none());
    let threads = engine.threads.read().await;
    assert!(threads.get(&owner_id).expect("owner").pinned);
}

#[tokio::test]
async fn hard_reject_fails_goal_cancels_worker_and_unpins() {
    let engine = test_engine().await;
    let goal = start_supervised_goal(&engine).await;
    let worker = enqueue_worker(&engine, &goal.id).await;
    engine
        .request_goal_review(&goal.id, &worker.id, "Claimed done.")
        .await
        .expect("review");

    let owner_id = goal.supervision_thread_id.clone().expect("owner");
    let updated = engine
        .submit_goal_review(
            &goal.id,
            GoalSupervisorVerdict::HardReject,
            "Wrong objective.",
            Some(&owner_id),
        )
        .await
        .expect("hard reject");
    assert_eq!(updated.status, GoalRunStatus::Failed);

    let worker = engine
        .task_by_id_for_dispatcher(&worker.id)
        .await
        .expect("worker");
    assert_eq!(worker.status, TaskStatus::Cancelled);
    let threads = engine.threads.read().await;
    assert!(!threads.get(&owner_id).expect("owner").pinned);
}

#[tokio::test]
async fn worker_completed_without_review_does_not_complete_goal() {
    let engine = test_engine().await;
    let goal = start_supervised_goal(&engine).await;
    let mut worker = enqueue_worker(&engine, &goal.id).await;
    worker.status = TaskStatus::Completed;
    worker.completed_at = Some(now_millis());
    engine.upsert_live_task(&worker).await;

    engine
        .nudge_goal_worker_for_review(&goal.id, &worker)
        .await
        .expect("nudge");
    let goal = engine.get_goal_run(&goal.id).await.expect("goal");
    assert_ne!(goal.status, GoalRunStatus::Completed);
    assert_eq!(goal.status, GoalRunStatus::Running);
}

#[tokio::test]
async fn hibernated_worker_is_not_requeued_by_review_nudge() {
    let engine = test_engine().await;
    let goal = start_supervised_goal(&engine).await;
    let mut worker = enqueue_worker(&engine, &goal.id).await;
    let worker_thread = goal.thread_id.clone().expect("worker thread");
    worker.thread_id = Some(worker_thread.clone());
    worker.status = TaskStatus::Completed;
    worker.completed_at = Some(now_millis());
    engine.upsert_live_task(&worker).await;

    // Hibernation signal 1: pending operation wakeup (background op in flight).
    engine
        .register_operation_wakeup(&worker_thread, "bash_command", "op-hibernated-1", true)
        .await;

    engine
        .nudge_goal_worker_for_review(&goal.id, &worker)
        .await
        .expect("nudge");

    let after = engine
        .task_by_id_for_dispatcher(&worker.id)
        .await
        .expect("worker");
    assert_eq!(
        after.status,
        TaskStatus::Completed,
        "hibernated worker must not be requeued"
    );
    {
        let threads = engine.threads.read().await;
        let thread = threads.get(&worker_thread).expect("worker thread");
        assert!(
            !thread
                .messages
                .iter()
                .any(|message| message.role == MessageRole::System
                    && message.content.contains("finished a turn without calling")),
            "hibernated worker must not receive the review nudge message"
        );
    }

    // Hibernation signal 2: pending scheduled timer wakeup.
    engine.claim_operation_wakeup("op-hibernated-1").await;
    engine.timer_wakeups.lock().await.insert(
        "wakeup-hibernated-1".to_string(),
        crate::agent::agent_wakeup::AgentWakeup {
            id: "wakeup-hibernated-1".to_string(),
            thread_id: worker_thread.clone(),
            message: "training checkpoint".to_string(),
            interval_ms: 3_600_000,
            next_fire_at: now_millis() + 3_600_000,
            repetitions_remaining: Some(1),
            wakeup_kind: "generic".to_string(),
            goal_run_id: None,
            created_at: now_millis(),
        },
    );
    engine
        .nudge_goal_worker_for_review(&goal.id, &worker)
        .await
        .expect("nudge");
    let after = engine
        .task_by_id_for_dispatcher(&worker.id)
        .await
        .expect("worker");
    assert_eq!(
        after.status,
        TaskStatus::Completed,
        "timer-hibernated worker must not be requeued"
    );

    // Once hibernation clears (no pending wakeups), the nudge resumes normal
    // behavior: requeue so the worker gets the review reminder turn.
    engine
        .timer_wakeups
        .lock()
        .await
        .remove("wakeup-hibernated-1");
    engine
        .nudge_goal_worker_for_review(&goal.id, &worker)
        .await
        .expect("nudge");
    let after = engine
        .task_by_id_for_dispatcher(&worker.id)
        .await
        .expect("worker");
    assert_eq!(after.status, TaskStatus::Queued);
}

#[tokio::test]
async fn non_owner_cannot_submit_goal_review() {
    let engine = test_engine().await;
    let goal = start_supervised_goal(&engine).await;
    let worker = enqueue_worker(&engine, &goal.id).await;
    engine
        .request_goal_review(&goal.id, &worker.id, "Done.")
        .await
        .expect("review");

    let worker_thread = goal.thread_id.clone().expect("worker thread");
    let err = engine
        .submit_goal_review(
            &goal.id,
            GoalSupervisorVerdict::Accept,
            "",
            Some(&worker_thread),
        )
        .await
        .expect_err("worker must not verdict");
    assert!(err.to_string().contains("owner supervisor"));

    let (bystander, _) = engine
        .get_or_create_thread_with_target(Some("unrelated-thread"), "Bystander", None)
        .await;
    let err = engine
        .submit_goal_review(
            &goal.id,
            GoalSupervisorVerdict::Accept,
            "",
            Some(&bystander),
        )
        .await
        .expect_err("third-party thread must not verdict");
    assert!(err.to_string().contains("owner supervisor"));

    let goal = engine.get_goal_run(&goal.id).await.expect("goal");
    assert_eq!(goal.status, GoalRunStatus::AwaitingReview);
}

#[tokio::test]
async fn ui_launched_goal_has_no_owner_supervisor_thread() {
    let engine = test_engine().await;
    let goal = engine
        .start_goal_run(
            "Ship without an owner conversation".to_string(),
            Some("Direct launch".to_string()),
            None,
            None,
            Some("normal"),
            None,
            None,
            None,
        )
        .await;
    assert!(goal.supervision_thread_id.is_none());
    assert!(goal
        .thread_id
        .as_deref()
        .is_some_and(|id| id.starts_with("goal:")));
}

#[tokio::test]
async fn spawned_goal_child_does_not_steal_worker_thread_or_active_task() {
    let engine = test_engine().await;
    let goal = start_supervised_goal(&engine).await;
    let worker = enqueue_worker(&engine, &goal.id).await;
    let worker_thread = goal.thread_id.clone().expect("worker thread");

    let mut child = engine
        .enqueue_task(
            "Spawned helper".to_string(),
            "do a slice of work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            Some(goal.id.clone()),
            Some(worker.id.clone()),
            Some(worker_thread.clone()),
            None,
        )
        .await;
    child.thread_id = Some("thread_spawned_helper".to_string());
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child.id) {
            *existing = child.clone();
        }
    }

    engine.sync_goal_run_with_task(&goal.id, &child).await;

    let updated = engine.get_goal_run(&goal.id).await.expect("goal");
    assert_eq!(updated.active_task_id.as_deref(), Some(worker.id.as_str()));
    assert_eq!(updated.thread_id.as_deref(), Some(worker_thread.as_str()));
    assert_eq!(updated.execution_thread_ids, vec![worker_thread.clone()]);
    assert!(!updated
        .execution_thread_ids
        .iter()
        .any(|id| id == "thread_spawned_helper"));
}
