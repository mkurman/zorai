use super::*;
use crate::agent::types::{AgentTask, TaskPriority, TaskStatus};
use rusqlite::TransactionBehavior;
use zorai_protocol::{AgentDbMessage, AgentDbThread};

fn sample_thread() -> AgentDbThread {
    AgentDbThread {
        id: "thread-1".to_string(),
        workspace_id: Some("workspace-1".into()),
        surface_id: None,
        pane_id: None,
        agent_name: None,
        title: "Thread".to_string(),
        created_at: 90,
        updated_at: 100,
        message_count: 0,
        total_tokens: 0,
        last_preview: String::new(),
        metadata_json: None,
    }
}

fn sample_message(id: &str, content: &str) -> AgentDbMessage {
    AgentDbMessage {
        reasoning_tokens: None,
        thinking_ms: None,
        id: id.to_string(),
        thread_id: "thread-1".to_string(),
        created_at: 100,
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
    }
}

fn sample_task(id: &str) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: "Implement indexing".to_string(),
        description: "Persist semantic indexing work from daemon events".to_string(),
        status: TaskStatus::Queued,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: 200,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some("thread-1".to_string()),
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: Some("session-1".to_string()),
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
async fn adding_message_enqueues_embedding_job() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;

    store
        .add_message(&sample_message("msg-1", "semantic content"))
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].source_kind, "agent_message");
    assert_eq!(jobs[0].source_id, "msg-1");
    assert_eq!(jobs[0].body, "semantic content");
    assert_eq!(jobs[0].thread_id.as_deref(), Some("thread-1"));
    Ok(())
}

#[tokio::test]
async fn adding_large_message_splits_embedding_jobs_into_bounded_chunks() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    let content = "semantic content ".repeat(2_000);

    store
        .add_message(&sample_message("msg-large", &content))
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert!(jobs.len() > 1);
    assert_eq!(jobs[0].chunk_id, "0");
    assert_eq!(jobs[1].chunk_id, "1");
    assert!(jobs.iter().all(|job| job.body.chars().count() <= 6_000));
    assert!(jobs.iter().all(|job| job.source_id == "msg-large"));
    Ok(())
}

#[tokio::test]
async fn shrinking_large_message_removes_stale_embedding_chunks() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    let content = "semantic content ".repeat(2_000);
    store
        .add_message(&sample_message("msg-shrink", &content))
        .await?;

    store
        .update_message(
            "msg-shrink",
            &AgentMessagePatch {
                content: Some("short content".to_string()),
                ..AgentMessagePatch::default()
            },
        )
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].chunk_id, "0");
    assert_eq!(jobs[0].body, "short content");
    Ok(())
}

#[tokio::test]
async fn claiming_legacy_large_embedding_job_splits_it_before_claiming() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    let content = "legacy semantic content ".repeat(2_000);
    store
        .conn
        .call(move |conn| {
            conn.execute(
                "INSERT INTO embedding_jobs (
                    source_kind,
                    source_id,
                    chunk_id,
                    content_hash,
                    title,
                    body,
                    workspace_id,
                    thread_id,
                    agent_id,
                    source_timestamp,
                    queued_at,
                    updated_at,
                    claimed_at,
                    attempts,
                    last_error
                ) VALUES ('agent_message', 'legacy-large', '0', 'legacy-hash', 'user', ?1, NULL, 'thread-1', NULL, 100, 100, 100, NULL, 0, NULL)",
                params![content],
            )?;
            Ok(())
        })
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert!(jobs.len() > 1);
    assert!(jobs.iter().all(|job| job.body.chars().count() <= 6_000));
    assert!(jobs.iter().all(|job| job.source_id == "legacy-large"));
    assert!(jobs.iter().all(|job| job.attempts == 0));
    Ok(())
}

#[tokio::test]
async fn adding_blank_message_does_not_enqueue_embedding_job() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;

    store
        .add_message(&sample_message("msg-blank", "   "))
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert!(jobs.is_empty());
    let deletions = store.claim_embedding_deletions(10).await?;
    assert!(deletions.is_empty());
    Ok(())
}

#[tokio::test]
async fn upserting_task_enqueues_embedding_job() -> Result<()> {
    let (store, _root) = make_test_store().await?;

    store.upsert_agent_task(&sample_task("task-1")).await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].source_kind, "agent_task");
    assert_eq!(jobs[0].source_id, "task-1");
    assert!(jobs[0].body.contains("Implement indexing"));
    assert_eq!(jobs[0].thread_id.as_deref(), Some("thread-1"));
    Ok(())
}

#[tokio::test]
async fn embedding_completion_is_scoped_to_model_and_dimensions() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-1", "same source"))
        .await?;

    let first = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(first.len(), 1);
    store
        .complete_embedding_job(&first[0], "text-embedding-3-small", 1536)
        .await?;

    let same_model = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert!(same_model.is_empty());

    let switched_model = store
        .claim_embedding_jobs("nomic-embed-text", 768, 10)
        .await?;
    assert_eq!(switched_model.len(), 1);
    assert_eq!(switched_model[0].source_id, "msg-1");
    Ok(())
}

#[tokio::test]
async fn deleting_unindexed_message_does_not_queue_vector_deletion() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-delete", "remove me"))
        .await?;

    let deleted = store.delete_messages("thread-1", &["msg-delete"]).await?;
    assert_eq!(deleted, 1);

    let deletions = store.claim_embedding_deletions(10).await?;
    assert!(deletions.is_empty());
    Ok(())
}

#[tokio::test]
async fn deleting_indexed_message_queues_vector_deletion() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-delete-indexed", "remove me"))
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    store
        .complete_embedding_job(&jobs[0], "text-embedding-3-small", 1536)
        .await?;

    let deleted = store
        .delete_messages("thread-1", &["msg-delete-indexed"])
        .await?;
    assert_eq!(deleted, 1);

    let deletions = store.claim_embedding_deletions(10).await?;
    assert_eq!(deletions.len(), 1);
    assert_eq!(deletions[0].source_kind, "agent_message");
    assert_eq!(deletions[0].source_id, "msg-delete-indexed");
    Ok(())
}

#[tokio::test]
async fn delete_thread_batches_embedding_deletions_for_all_messages() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    for id in ["msg-batch-1", "msg-batch-2", "msg-batch-3"] {
        store
            .add_message(&sample_message(id, "content for batched deletion"))
            .await?;
    }

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 3);
    for job in &jobs {
        store
            .complete_embedding_job(job, "text-embedding-3-small", 1536)
            .await?;
    }

    store.delete_thread("thread-1").await?;

    let mut deletions = store.claim_embedding_deletions(10).await?;
    deletions.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    assert_eq!(deletions.len(), 3);
    for deletion in &deletions {
        assert_eq!(deletion.source_kind, "agent_message");
    }
    assert_eq!(
        deletions
            .iter()
            .map(|d| d.source_id.as_str())
            .collect::<Vec<_>>(),
        vec!["msg-batch-1", "msg-batch-2", "msg-batch-3"],
    );
    Ok(())
}

#[tokio::test]
async fn updating_message_content_refreshes_embedding_job() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-update", "old content"))
        .await?;

    store
        .update_message(
            "msg-update",
            &AgentMessagePatch {
                content: Some("new content".to_string()),
                ..AgentMessagePatch::default()
            },
        )
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].body, "new content");
    Ok(())
}

#[tokio::test]
async fn queue_semantic_backfill_enqueues_existing_messages_and_tasks() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-backfill", "index this old message"))
        .await?;
    store
        .upsert_agent_task(&sample_task("task-backfill"))
        .await?;

    let initial = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    for job in &initial {
        store
            .complete_embedding_job(job, "text-embedding-3-small", 1536)
            .await?;
    }

    let result = store.queue_semantic_backfill(None).await?;
    assert_eq!(result.messages_queued, 1);
    assert_eq!(result.tasks_queued, 1);

    let switched_model = store
        .claim_embedding_jobs("nomic-embed-text", 768, 10)
        .await?;
    assert_eq!(switched_model.len(), 2);
    Ok(())
}

#[tokio::test]
async fn semantic_index_status_counts_pending_for_selected_model() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-status", "status content"))
        .await?;

    let pending = store
        .semantic_index_status("text-embedding-3-small", 1536)
        .await?;
    assert_eq!(pending.pending_for_model, 1);
    assert_eq!(pending.completed_for_model, 0);

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    store
        .complete_embedding_job(&jobs[0], "text-embedding-3-small", 1536)
        .await?;

    let completed = store
        .semantic_index_status("text-embedding-3-small", 1536)
        .await?;
    assert_eq!(completed.pending_for_model, 0);
    assert_eq!(completed.completed_for_model, 1);
    Ok(())
}

#[tokio::test]
async fn repairing_semantic_index_state_requeues_completed_and_failed_jobs() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message(
            "msg-repair-completed",
            "repair this vector",
        ))
        .await?;
    store
        .add_message(&sample_message("msg-repair-delete", "repair this deletion"))
        .await?;

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 2);
    for job in &jobs {
        store
            .complete_embedding_job(job, "text-embedding-3-small", 1536)
            .await?;
    }
    let completed_job = jobs
        .iter()
        .find(|job| job.source_id == "msg-repair-completed")
        .expect("completed job should exist");
    store
        .fail_embedding_job(completed_job, "stale failure")
        .await?;

    let deleted = store
        .delete_messages("thread-1", &["msg-repair-delete"])
        .await?;
    assert_eq!(deleted, 1);
    let deletions = store.claim_embedding_deletions(10).await?;
    assert_eq!(deletions.len(), 1);
    store
        .fail_embedding_deletion(&deletions[0], "stale deletion failure")
        .await?;

    let before = store
        .semantic_index_status("text-embedding-3-small", 1536)
        .await?;
    assert_eq!(before.completed_for_model, 1);
    assert_eq!(before.failed_jobs, 1);
    assert_eq!(before.queued_deletions, 1);
    assert_eq!(before.failed_deletions, 1);

    let reset = store.reset_semantic_vector_index_state().await?;
    assert_eq!(reset.cleared_completions, 1);
    assert_eq!(reset.cleared_deletions, 1);
    assert_eq!(reset.reset_failed_jobs, 1);

    let after = store
        .semantic_index_status("text-embedding-3-small", 1536)
        .await?;
    assert_eq!(after.completed_for_model, 0);
    assert_eq!(after.pending_for_model, 1);
    assert_eq!(after.failed_jobs, 0);
    assert_eq!(after.queued_deletions, 0);
    assert_eq!(after.failed_deletions, 0);
    Ok(())
}

#[tokio::test]
async fn claim_embedding_jobs_retries_transient_database_locks() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    store
        .add_message(&sample_message("msg-locked", "retry me after lock release"))
        .await?;

    let db_path = root.join("history").join("command-history.db");
    let mut lock_conn = rusqlite::Connection::open(&db_path)?;
    let lock_tx = lock_conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let claim_task = {
        let store = store.clone();
        tokio::spawn(async move {
            store
                .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
                .await
        })
    };

    tokio::time::sleep(std::time::Duration::from_millis(5_200)).await;
    drop(lock_tx);

    let jobs = claim_task.await??;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].source_id, "msg-locked");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn reasoning_tokens_and_thinking_time_survive_a_reload_of_the_thread() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    store.create_thread(&sample_thread()).await?;
    let mut message = sample_message("msg-think", "answer");
    message.role = "assistant".to_string();
    message.reasoning_tokens = Some(1996);
    message.thinking_ms = Some(128_000);
    store.add_message(&message).await?;

    let reloaded = store.list_messages("thread-1", None).await?;
    let limited = store.list_messages("thread-1", Some(5)).await?;

    assert_eq!(reloaded[0].reasoning_tokens, Some(1996));
    assert_eq!(reloaded[0].thinking_ms, Some(128_000));
    assert_eq!(limited[0].reasoning_tokens, Some(1996));
    Ok(())
}
