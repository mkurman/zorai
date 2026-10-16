use super::*;
use crate::agent::types::AgentConfig;
use crate::session_manager::SessionManager;
use std::sync::Arc;
use tempfile::tempdir;

async fn setup() -> (tempfile::TempDir, Arc<AgentEngine>) {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    (root, engine)
}

#[tokio::test]
async fn report_subagent_outcome_records_summary_on_spawned_child() {
    let (_root, engine) = setup().await;
    let parent = engine
        .enqueue_task(
            "Parent".into(),
            "coord".into(),
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
    let mut child = engine
        .enqueue_task(
            "Child".into(),
            "work".into(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".into()),
            None,
        )
        .await;
    child.thread_id = Some("thread-child".into());
    persist_task_update(&engine, &child, None)
        .await
        .expect("persist child");

    let result = execute_report_subagent_outcome(
        &serde_json::json!({
            "status": "done",
            "summary": "parser tests now pass"
        }),
        &engine,
        "thread-child",
        Some(&child.id),
    )
    .await
    .expect("report should succeed");
    assert!(result.contains("parser tests now pass"));
    let stored = task_by_id_for_tool_scope(&engine, &child.id)
        .await
        .expect("child exists");
    assert_eq!(stored.result.as_deref(), Some("parser tests now pass"));
    let result = stored
        .completion_contract
        .as_ref()
        .and_then(|contract| contract.child_result.as_ref())
        .expect("typed child result");
    assert_eq!(result.report_state, ChildReportState::Usable);
    assert!(result.asks_reconciled);
    assert_eq!(result.parent_notification, ParentNotificationState::Pending);
}

#[tokio::test]
async fn empty_and_truncated_reports_are_persisted_but_not_usable() {
    let (_root, engine) = setup().await;
    let parent = engine
        .enqueue_task(
            "Parent".into(),
            "coord".into(),
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
    let mut child = engine
        .enqueue_task(
            "Child".into(),
            "work".into(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id),
            Some("thread-parent".into()),
            None,
        )
        .await;
    child.thread_id = Some("thread-child-empty".into());
    persist_task_update(&engine, &child, None).await.unwrap();

    execute_report_subagent_outcome(
        &serde_json::json!({"status": "done", "summary": ""}),
        &engine,
        "thread-child-empty",
        Some(&child.id),
    )
    .await
    .expect("empty report is classified, not discarded");
    let stored = task_by_id_for_tool_scope(&engine, &child.id).await.unwrap();
    assert_eq!(
        stored
            .completion_contract
            .as_ref()
            .unwrap()
            .child_result
            .as_ref()
            .unwrap()
            .report_state,
        ChildReportState::Empty
    );
    assert!(stored
        .completion_blockers()
        .iter()
        .any(|reason| reason.contains("not usable")));

    execute_report_subagent_outcome(
        &serde_json::json!({"status": "done", "summary": "partial", "truncated": true}),
        &engine,
        "thread-child-empty",
        Some(&child.id),
    )
    .await
    .unwrap();
    let stored = task_by_id_for_tool_scope(&engine, &child.id).await.unwrap();
    assert_eq!(
        stored
            .completion_contract
            .as_ref()
            .unwrap()
            .child_result
            .as_ref()
            .unwrap()
            .report_state,
        ChildReportState::Truncated
    );
}

#[tokio::test]
async fn duplicate_report_reuses_terminal_version_and_indexes_artifacts() {
    let (root, engine) = setup().await;
    let parent = engine
        .enqueue_task(
            "Parent".into(),
            "coord".into(),
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
    let mut child = engine
        .enqueue_task(
            "Child".into(),
            "work".into(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id),
            Some("thread-parent".into()),
            None,
        )
        .await;
    child.thread_id = Some("thread-child-artifact".into());
    persist_task_update(&engine, &child, None).await.unwrap();
    let artifact = root
        .path()
        .join("threads/thread-child-artifact/artifacts/specs/result.md");
    std::fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    std::fs::write(&artifact, "artifact-only recovery").unwrap();

    for _ in 0..2 {
        execute_report_subagent_outcome(
            &serde_json::json!({"status": "done", "summary": ""}),
            &engine,
            "thread-child-artifact",
            Some(&child.id),
        )
        .await
        .unwrap();
    }
    {
        let mut live = engine.tasks.lock().await;
        live.retain(|task| task.id != child.id);
    }
    let stored = task_by_id_for_tool_scope(&engine, &child.id).await.unwrap();
    let result = stored.completion_contract.unwrap().child_result.unwrap();
    assert_eq!(result.terminal_version, 1);
    assert_eq!(result.report_state, ChildReportState::Empty);
    assert_eq!(result.summary, None);
    assert_eq!(result.artifact_refs, vec![artifact.display().to_string()]);
}

#[tokio::test]
async fn extend_subagent_budget_raises_child_ceiling_and_unlocks_budget_exceeded() {
    let (_root, engine) = setup().await;
    let parent = engine
        .enqueue_task(
            "Parent".into(),
            "coord".into(),
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
    let mut child = engine
        .enqueue_task(
            "Child".into(),
            "work".into(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".into()),
            None,
        )
        .await;
    child.thread_id = Some("thread-child".into());
    child.context_budget_tokens = Some(1_000);
    child.status = TaskStatus::BudgetExceeded;
    child.error = Some("execution budget exceeded for this thread".into());
    persist_task_update(&engine, &child, None)
        .await
        .expect("persist child");

    let result = execute_extend_subagent_budget(
        &serde_json::json!({
            "child_task_id": child.id,
            "additional_tokens": 512,
            "reason": "need to finish the remaining tests"
        }),
        &engine,
        "thread-parent",
        Some(&parent.id),
    )
    .await
    .expect("extend should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("extend json");
    assert_eq!(parsed["resumed"], true);
    let stored = task_by_id_for_tool_scope(&engine, &child.id)
        .await
        .expect("child exists");
    assert_eq!(stored.context_budget_tokens, Some(1_512));
    assert_eq!(
        stored.status,
        TaskStatus::Queued,
        "a resumed child goes back through the dispatcher so its outcome is tracked and the parent's tool call is not blocked"
    );
    assert!(stored.error.is_none());
    assert!(
        !engine.operator_stream_stop_requested("thread-child").await,
        "waking the idle child must not leave an operator-stop flag that drops its resume continuation"
    );
}

#[tokio::test]
async fn usable_report_from_a_failed_child_completes_it_and_tells_the_parent_again() {
    let (_root, engine) = setup().await;
    let mut child = engine
        .enqueue_task(
            "Child".into(),
            "work".into(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some("thread-parent-of-failed".into()),
            None,
        )
        .await;
    child.thread_id = Some("thread-failed-child".into());
    child.status = TaskStatus::Failed;
    child.completed_at = Some(now_millis());
    child.last_error = Some("xai spending-limit".into());
    persist_task_update(&engine, &child, None).await.unwrap();
    let mut old_notice = crate::agent::types::AgentMessage::user(
        format!("Child `{}` failed.\n\nStatus: error", child.id),
        1,
    );
    old_notice.role = crate::agent::types::MessageRole::System;
    engine.threads.write().await.insert(
        "thread-parent-of-failed".to_string(),
        crate::agent::types::AgentThread {
            id: "thread-parent-of-failed".to_string(),
            agent_name: None,
            title: "Parent".to_string(),
            messages: vec![
                crate::agent::types::AgentMessage::user("coordinate", 1),
                old_notice,
            ],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );

    execute_report_subagent_outcome(
        &serde_json::json!({"status": "done", "summary": "evidence audit written"}),
        &engine,
        "thread-failed-child",
        Some(&child.id),
    )
    .await
    .expect("report succeeds");

    let stored = task_by_id_for_tool_scope(&engine, &child.id).await.unwrap();
    assert_eq!(
        stored.status,
        TaskStatus::Completed,
        "a usable result must not stay filed under a failed task, or the parent is told to retry and wipe it"
    );
    let parent_notices = engine
        .threads
        .read()
        .await
        .get("thread-parent-of-failed")
        .map(|thread| {
            thread
                .messages
                .iter()
                .filter(|message| message.role == crate::agent::types::MessageRole::System)
                .map(|message| message.content.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        parent_notices
            .iter()
            .any(|notice| notice.contains("finished") && notice.contains("evidence audit written")),
        "the parent's visible history must show the new outcome, not only the old failure: {parent_notices:?}"
    );
}
