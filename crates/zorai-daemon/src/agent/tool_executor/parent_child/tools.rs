use super::super::{
    make_task_log_entry, now_millis, task_by_id_for_tool_scope, AgentEngine, Result, TaskLogLevel,
};
use super::handles::next_ask_slug;
use super::wakeup::{wake_child_thread, wake_parent_for_ask};
use super::{
    ask_state_prefix, caller_is_parent, list_ask_records, notes_cursor_after_eviction,
    persist_ask_record, persist_task_update, remaining_open_count, resolve_caller_task,
    resolve_child_reference, select_open_ask, unblock_child_task, AskParentRecord,
    AWAITING_PARENT_BLOCKED_PREFIX, DEFAULT_TIMEOUT_MINUTES, LOG_PHASE, MAX_NOTES_PER_CHILD,
    MAX_NOTE_CHARS, MAX_OPEN_ASKS_PER_CHILD, NOTES_CURSOR_STATE_PREFIX, NOTES_STATE_PREFIX,
};
use crate::agent::types::TaskStatus;

pub(crate) async fn execute_ask_parent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    _thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let question = args
        .get("question")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'question' argument"))?
        .to_string();
    let options = args
        .get("options")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let timeout_minutes = args
        .get("timeout_minutes")
        .and_then(|value| value.as_u64())
        .map(|value| value.max(1))
        .unwrap_or(DEFAULT_TIMEOUT_MINUTES);
    let default = args
        .get("default")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let caller_task_id = task_id.ok_or_else(|| {
        anyhow::anyhow!(
            "ask_parent can only be called from within a child task; this tool call has no task context"
        )
    })?;
    let task = task_by_id_for_tool_scope(agent, caller_task_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("task {caller_task_id} not found"))?;
    if task.parent_task_id.is_none() && task.parent_thread_id.is_none() {
        anyhow::bail!(
            "ask_parent requires a parent: task {} has no parent_task_id or parent_thread_id. \
             Use ask_questions to reach the operator instead.",
            task.id
        );
    }

    let existing = list_ask_records(agent, &task.id).await?;
    let open_count = existing
        .iter()
        .filter(|(_, record)| record.state == "open")
        .count();
    if open_count >= MAX_OPEN_ASKS_PER_CHILD {
        anyhow::bail!(
            "{open_count} open ask_parent records already exist for task {}; batch your questions \
             into a single ask_parent call or wait for existing answers before asking again",
            task.id
        );
    }

    let ask_slug = next_ask_slug(&existing);
    let child_slug = super::ensure_task_slug(agent, &task).await?;
    let record = AskParentRecord {
        question: question.clone(),
        options,
        asked_at: now_millis(),
        timeout_minutes,
        default,
        state: "open".to_string(),
        answer: None,
        answer_delivered: false,
        slug: ask_slug.clone(),
    };
    let ask_id = uuid::Uuid::new_v4().to_string();
    let key = format!("{}{ask_id}", ask_state_prefix(&task.id));
    persist_ask_record(agent, &key, &record).await?;

    let mut updated = task.clone();
    updated.status = TaskStatus::Blocked;
    updated.blocked_reason = Some(format!("{AWAITING_PARENT_BLOCKED_PREFIX} {question}"));
    updated.logs.push(make_task_log_entry(
        updated.retry_count,
        TaskLogLevel::Info,
        LOG_PHASE,
        "asked parent a blocking question; task paused until answered or timed out",
        Some(question.clone()),
    ));
    persist_task_update(
        agent,
        &updated,
        Some("Child is awaiting a parent answer".into()),
    )
    .await?;
    wake_parent_for_ask(
        agent,
        &updated,
        &ask_id,
        &ask_slug,
        &child_slug,
        &question,
        &record.options,
    )
    .await;

    Ok(serde_json::json!({
        "ok": true,
        "state": "open",
        "ask_id": ask_id,
        "ask_slug": ask_slug,
        "timeout_minutes": timeout_minutes,
        "message": "Task is now blocked awaiting the parent's answer. Stop working and wait to be unblocked; do not retry ask_parent."
    })
    .to_string())
}

pub(crate) async fn execute_answer_child(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let child_task_id = required_string_arg(args, "child_task_id")?;
    let answer = required_string_arg(args, "answer")?;
    let requested_ask_id = args
        .get("ask_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let child = resolve_child_reference(agent, &child_task_id, thread_id, task_id).await?;
    let caller_task = resolve_caller_task(agent, thread_id, task_id).await;
    if !caller_is_parent(&child, caller_task.as_ref(), thread_id) {
        anyhow::bail!(
            "only the child's parent (parent task {} or parent thread {}) may answer task {}; \
             call it from the parent thread or parent task context",
            child.parent_task_id.as_deref().unwrap_or("(none)"),
            child.parent_thread_id.as_deref().unwrap_or("(none)"),
            child.id
        );
    }

    let records = list_ask_records(agent, &child.id).await?;
    let open: Vec<(String, AskParentRecord)> = records
        .iter()
        .filter(|(_, record)| record.state == "open")
        .cloned()
        .collect();
    let (key, mut record) = select_open_ask(&open, requested_ask_id)?;
    record.state = "answered".to_string();
    record.answer = Some(answer.clone());
    record.answer_delivered = false;
    persist_ask_record(agent, &key, &record).await?;
    let still_open = remaining_open_count(&records, &key);

    let mut updated = child.clone();
    let unblocked = still_open == 0 && unblock_child_task(agent, &mut updated).await;
    updated.logs.push(make_task_log_entry(
        updated.retry_count,
        TaskLogLevel::Info,
        LOG_PHASE,
        if unblocked {
            "parent answered; child unblocked"
        } else if still_open > 0 {
            "parent answered one open question; child still blocked on remaining asks"
        } else {
            "parent answered an open question"
        },
        Some(answer.chars().take(MAX_NOTE_CHARS).collect()),
    ));
    persist_task_update(
        agent,
        &updated,
        Some(if unblocked {
            "Parent answered; child unblocked".into()
        } else if still_open > 0 {
            "Parent answered one question; child still waiting".into()
        } else {
            "Parent answered an open question".into()
        }),
    )
    .await?;
    if unblocked {
        wake_child_thread(
            agent,
            &updated,
            "Your parent answered your blocking question. The full answer is provided in the [parent answer] context block. Continue the assigned work.".to_string(),
        )
        .await;
    }

    Ok(serde_json::json!({
        "ok": true,
        "answered": 1,
        "remaining_open": still_open,
        "unblocked": unblocked,
        "child_status": format!("{:?}", updated.status).to_lowercase(),
    })
    .to_string())
}

pub(crate) async fn execute_note_to_child(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let child_task_id = required_string_arg(args, "child_task_id")?;
    let note = required_string_arg(args, "note")?;

    let child = resolve_child_reference(agent, &child_task_id, thread_id, task_id).await?;
    let child_task_id = child.id.clone();
    let caller_task = resolve_caller_task(agent, thread_id, task_id).await;
    if !caller_is_parent(&child, caller_task.as_ref(), thread_id) {
        anyhow::bail!(
            "only the child's parent (parent task {} or parent thread {}) may note task {}; \
             call it from the parent thread or parent task context",
            child.parent_task_id.as_deref().unwrap_or("(none)"),
            child.parent_thread_id.as_deref().unwrap_or("(none)"),
            child.id
        );
    }

    let notes_key = format!("{NOTES_STATE_PREFIX}{child_task_id}");
    let mut notes: Vec<String> = agent
        .history
        .get_consolidation_state(&notes_key)
        .await?
        .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
        .unwrap_or_default();
    let truncated: String = note.chars().take(MAX_NOTE_CHARS).collect();
    notes.push(truncated);
    let evicted = notes.len() > MAX_NOTES_PER_CHILD;
    if evicted {
        notes.remove(0);
    }
    agent
        .history
        .set_consolidation_state(&notes_key, &serde_json::to_string(&notes)?, now_millis())
        .await?;
    if evicted {
        let cursor_key = format!("{NOTES_CURSOR_STATE_PREFIX}{child_task_id}");
        let delivered = agent
            .history
            .get_consolidation_state(&cursor_key)
            .await?
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        agent
            .history
            .set_consolidation_state(
                &cursor_key,
                &notes_cursor_after_eviction(delivered, 1).to_string(),
                now_millis(),
            )
            .await?;
    }

    let mut updated = child.clone();
    updated.logs.push(make_task_log_entry(
        updated.retry_count,
        TaskLogLevel::Info,
        LOG_PHASE,
        "parent attached a guidance note; delivered with the next turn",
        Some(note.chars().take(MAX_NOTE_CHARS).collect()),
    ));
    persist_task_update(agent, &updated, Some("Parent sent guidance note".into())).await?;

    Ok(serde_json::json!({
        "ok": true,
        "stored_notes": notes.len(),
        "oldest_evicted": evicted,
        "status_changed": false,
    })
    .to_string())
}

fn required_string_arg(args: &serde_json::Value, name: &str) -> Result<String> {
    args.get(name)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("missing '{name}' argument"))
}
