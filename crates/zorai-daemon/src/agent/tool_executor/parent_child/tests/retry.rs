use super::*;
use crate::agent::types::ChildResultContract;

async fn fail_child(engine: &AgentEngine, child: &AgentTask) -> AgentTask {
    let mut failed = child.clone();
    failed.status = TaskStatus::Failed;
    failed.retry_count = 3;
    failed.completed_at = Some(now_millis());
    failed.error = Some("provider returned 529 overloaded".into());
    failed.last_error = failed.error.clone();
    failed.blocked_reason = Some("retry budget exhausted".into());
    failed
        .completion_contract
        .as_mut()
        .expect("spawned child carries a completion contract")
        .child_result = Some(ChildResultContract {
        terminal_status: Some(TaskStatus::Failed),
        integration_acknowledged_at: Some(now_millis()),
        ..Default::default()
    });
    persist_task_update(engine, &failed, None)
        .await
        .expect("persist failed child");
    engine
        .tasks
        .lock()
        .await
        .retain(|task| task.id != failed.id);
    failed
}

async fn live_child(engine: &AgentEngine, child_id: &str) -> AgentTask {
    engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == child_id)
        .cloned()
        .expect("retried child must be back in the live queue so the dispatcher picks it up")
}

#[tokio::test]
async fn parent_retry_requeues_failed_child_and_reopens_parent_notification() {
    let (_root, engine) = setup().await;
    let child = spawn_child(&engine).await;
    let failed = fail_child(&engine, &child).await;

    execute_retry_subagent(
        &serde_json::json!({ "child_task_id": failed.id }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect("parent may retry its failed child");

    let retried = live_child(&engine, &failed.id).await;
    assert_eq!(retried.status, TaskStatus::Queued);
    assert_eq!(retried.thread_id.as_deref(), Some("thread-child"));
    assert_eq!(retried.retry_count, 0, "a manual retry gets a fresh auto-retry budget");
    assert!(retried.error.is_none() && retried.completed_at.is_none());
    assert!(
        retried
            .completion_contract
            .as_ref()
            .and_then(|contract| contract.child_result.as_ref())
            .is_none(),
        "the old integrated result must be cleared or the parent is never told about the retry's outcome"
    );
}

#[tokio::test]
async fn retry_subagent_rejects_non_parents_and_children_that_have_not_failed() {
    let (_root, engine) = setup().await;
    let child = spawn_child(&engine).await;

    let running = execute_retry_subagent(
        &serde_json::json!({ "child_task_id": child.id }),
        &engine,
        "thread-parent",
        None,
    )
    .await;
    assert!(running.is_err(), "a child that has not failed must not be requeued twice");

    let failed = fail_child(&engine, &child).await;
    let outsider = execute_retry_subagent(
        &serde_json::json!({ "child_task_id": failed.id }),
        &engine,
        "thread-other",
        None,
    )
    .await;
    assert!(outsider.is_err());
}

#[tokio::test]
async fn retry_now_on_a_failed_child_thread_requeues_its_task() {
    let (_root, engine) = setup().await;
    let child = spawn_child(&engine).await;
    let failed = fail_child(&engine, &child).await;

    assert!(engine.retry_stream_now("thread-child").await);

    assert_eq!(
        live_child(&engine, &failed.id).await.status,
        TaskStatus::Queued,
        "retrying in the child thread must bring the task back, not leave it failed"
    );
}
