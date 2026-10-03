use super::*;
use crate::agent::task_scheduler::make_task_log_entry;
use crate::agent::types::{
    AgentTask, ChildReportState, ChildResultContract, DeferredVisibleThreadContinuation,
    ParentNotificationState, TaskLogLevel, TaskStatus,
};
use crate::history::AgentTaskListQuery;

const MIN_EXTENSION_TOKENS: u32 = 256;
const MAX_EXTENSION_TOKENS: u32 = 32_768;
const MAX_BUDGET_EXTENSIONS: u32 = 3;
const MIN_EXTENSION_WALL_SECS: u64 = 30;
const MAX_EXTENSION_WALL_SECS: u64 = 1_800;

pub(crate) async fn execute_report_subagent_outcome(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let status = parse_report_status(args)?;
    let raw_summary = args
        .get("summary")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let summary = raw_summary.trim().to_string();

    let mut task = resolve_current_spawned_task(agent, thread_id, task_id).await?;
    let report_state = classify_report_state(args, &summary);
    let ask_records = super::parent_child::list_ask_records(agent, &task.id).await?;
    let open_ask_ids = ask_records
        .iter()
        .filter(|(_, record)| record.state == "open")
        .filter_map(|(key, _)| key.rsplit_once(':').map(|(_, ask_id)| ask_id.to_string()))
        .collect::<Vec<_>>();
    let artifact_refs = collect_child_artifact_refs(agent, &task).await;
    let parent_notification = if task.parent_task_id.is_some() || task.parent_thread_id.is_some() {
        ParentNotificationState::Pending
    } else {
        ParentNotificationState::NotRequired
    };
    let prior_version = task
        .completion_contract
        .as_ref()
        .and_then(|contract| contract.child_result.as_ref())
        .map(|result| result.terminal_version)
        .unwrap_or(0);
    task.result = (!summary.is_empty()).then(|| summary.clone());
    let child_result = ChildResultContract {
        terminal_status: Some(report_terminal_status(status)),
        terminal_version: prior_version.max(1),
        report_state,
        summary: (!summary.is_empty()).then(|| summary.clone()),
        summary_chars: summary.chars().count(),
        report_error: (report_state == ChildReportState::Failed)
            .then(|| summary.clone())
            .filter(|value| !value.is_empty()),
        artifact_refs,
        open_ask_ids: open_ask_ids.clone(),
        asks_reconciled: open_ask_ids.is_empty(),
        parent_notification,
        parent_notification_error: None,
        parent_notified_at: None,
        integration_acknowledged_at: None,
    };
    if let Some(contract) = task.completion_contract.as_mut() {
        contract.child_result = Some(child_result);
    }
    if report_state == ChildReportState::Usable && open_ask_ids.is_empty() {
        if let Some(contract) = task.completion_contract.as_mut() {
            contract.satisfy_requirement(
                crate::agent::types::SUBAGENT_REPORT_REQUIREMENT_DESCRIPTION,
                format!("reported summary: {summary}"),
            );
        }
    }
    task.logs.push(make_task_log_entry(
        task.retry_count,
        match status {
            "done" => TaskLogLevel::Info,
            "cancelled" => TaskLogLevel::Warn,
            _ => TaskLogLevel::Error,
        },
        "report",
        &format!("subagent reported {status}"),
        Some(summary.clone()),
    ));
    persist_task_update(agent, &task, Some(format!("Subagent reported {status}"))).await?;

    Ok(serde_json::json!({
        "ok": true,
        "action": "report",
        "status": status,
        "task_id": task.id,
        "report_state": report_state,
        "open_ask_ids": open_ask_ids,
        "artifacts": task.completion_contract.as_ref().and_then(|contract| contract.child_result.as_ref()).map(|result| &result.artifact_refs),
        "summary": summary,
    })
    .to_string())
}

pub(crate) async fn execute_extend_subagent_budget(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let additional_tokens = args
        .get("additional_tokens")
        .and_then(|value| value.as_u64())
        .map(|value| value.min(u32::MAX as u64) as u32)
        .ok_or_else(|| anyhow::anyhow!("missing 'additional_tokens' argument"))?;
    if additional_tokens < MIN_EXTENSION_TOKENS {
        anyhow::bail!("'additional_tokens' must be at least {MIN_EXTENSION_TOKENS}");
    }
    let additional_tokens = additional_tokens.min(MAX_EXTENSION_TOKENS);
    let additional_wall_time_secs = args
        .get("additional_wall_time_secs")
        .and_then(|value| value.as_u64())
        .map(|value| value.clamp(MIN_EXTENSION_WALL_SECS, MAX_EXTENSION_WALL_SECS));
    let reason = args
        .get("reason")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'reason' argument"))?
        .to_string();

    let mut child = resolve_target_spawned_task(agent, args, thread_id, task_id).await?;
    authorize_budget_extension(agent, &child, thread_id, task_id).await?;
    let extension_count = budget_extension_count(&child);
    if extension_count >= MAX_BUDGET_EXTENSIONS {
        anyhow::bail!(
            "spawned thread `{}` already used {MAX_BUDGET_EXTENSIONS} budget extensions",
            child.thread_id.as_deref().unwrap_or(&child.id)
        );
    }
    if matches!(
        child.status,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
    ) {
        anyhow::bail!(
            "cannot extend budget for terminal subagent `{}` in status {:?}",
            child.id,
            child.status
        );
    }

    let previous_tokens = child.context_budget_tokens.unwrap_or(0);
    let new_tokens = previous_tokens
        .saturating_add(additional_tokens)
        .max(MIN_EXTENSION_TOKENS);
    child.context_budget_tokens = Some(new_tokens);
    if child.context_overflow_action.is_none() {
        child.context_overflow_action = Some(crate::agent::types::ContextOverflowAction::Error);
    }
    if let Some(extra_secs) = additional_wall_time_secs {
        child.max_duration_secs = Some(
            child
                .max_duration_secs
                .unwrap_or(0)
                .saturating_add(extra_secs),
        );
    }

    let resumed = child.status == TaskStatus::BudgetExceeded;
    if resumed {
        child.status = TaskStatus::InProgress;
        child.progress = child.progress.min(90);
        child.completed_at = None;
        child.error = None;
        child.last_error = None;
        child.blocked_reason = None;
        child.next_retry_at = None;
    }
    child.logs.push(make_task_log_entry(
        child.retry_count,
        TaskLogLevel::Info,
        "budget",
        &format!(
            "execution budget extended by {additional_tokens} tokens ({} of {MAX_BUDGET_EXTENSIONS})",
            extension_count.saturating_add(1)
        ),
        Some(reason.clone()),
    ));
    persist_task_update(
        agent,
        &child,
        Some(format!("Subagent budget extended to {new_tokens} tokens")),
    )
    .await?;

    if resumed {
        wake_spawned_thread_after_budget_extension(agent, &child, additional_tokens, &reason).await;
    }

    Ok(serde_json::json!({
        "ok": true,
        "action": "extend",
        "task_id": child.id,
        "thread_id": child.thread_id,
        "previous_max_tokens": previous_tokens,
        "new_max_tokens": new_tokens,
        "additional_tokens": additional_tokens,
        "resumed": resumed,
        "reason": reason,
    })
    .to_string())
}

fn report_terminal_status(status: &str) -> TaskStatus {
    match status {
        "done" => TaskStatus::Completed,
        "cancelled" => TaskStatus::Cancelled,
        _ => TaskStatus::Failed,
    }
}

fn classify_report_state(args: &serde_json::Value, summary: &str) -> ChildReportState {
    if args
        .get("truncated")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        ChildReportState::Truncated
    } else if summary.is_empty() {
        ChildReportState::Empty
    } else if args.get("status").and_then(|value| value.as_str()) == Some("done") {
        ChildReportState::Usable
    } else {
        ChildReportState::Failed
    }
}

async fn collect_child_artifact_refs(agent: &AgentEngine, task: &AgentTask) -> Vec<String> {
    let Some(thread_id) = task.thread_id.as_deref() else {
        return Vec::new();
    };
    let root = agent
        .history
        .data_root()
        .join("threads")
        .join(thread_id)
        .join("artifacts");
    let mut refs = Vec::new();
    for directory in ["specs", "media", "previews"] {
        let path = root.join(directory);
        let Ok(mut entries) = tokio::fs::read_dir(&path).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.file_type().await.is_ok_and(|kind| kind.is_file()) {
                refs.push(entry.path().display().to_string());
            }
        }
    }
    refs.sort();
    refs.dedup();
    refs
}

fn parse_report_status(args: &serde_json::Value) -> Result<&'static str> {
    match args
        .get("status")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("")
    {
        "done" => Ok("done"),
        "cancelled" | "canceled" => Ok("cancelled"),
        "error" => Ok("error"),
        other if other.is_empty() => anyhow::bail!("missing 'status' argument"),
        other => anyhow::bail!("invalid report status '{other}'"),
    }
}

fn budget_extension_count(task: &AgentTask) -> u32 {
    task.logs.iter().filter(|log| log.phase == "budget").count() as u32
}

async fn persist_task_update(
    agent: &AgentEngine,
    updated: &AgentTask,
    message: Option<String>,
) -> Result<()> {
    let updated_live = {
        let mut tasks = agent.tasks.lock().await;
        if let Some(task) = tasks.iter_mut().find(|task| task.id == updated.id) {
            *task = updated.clone();
            true
        } else {
            false
        }
    };
    agent.history.upsert_agent_task(updated).await?;
    if updated_live {
        agent.persist_tasks().await;
    }
    agent.emit_task_update(updated, message);
    Ok(())
}

async fn resolve_current_spawned_task(
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<AgentTask> {
    let task = if let Some(task_id) = task_id {
        task_by_id_for_tool_scope(agent, task_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("task not found: {task_id}"))?
    } else {
        find_spawned_task_by_thread(agent, thread_id)
            .await
            .ok_or_else(|| {
                anyhow::anyhow!("report_subagent_outcome requires a spawned subagent thread")
            })?
    };
    if !task.is_spawned_subagent() {
        anyhow::bail!("report_subagent_outcome is only available to spawned subagents");
    }
    Ok(task)
}

async fn resolve_target_spawned_task(
    agent: &AgentEngine,
    args: &serde_json::Value,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<AgentTask> {
    if let Some(child_task_id) = args
        .get("child_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let task = resolve_child_reference(agent, child_task_id, thread_id, task_id).await?;
        if !task.is_spawned_subagent() {
            anyhow::bail!("task `{child_task_id}` is not a spawned subagent");
        }
        return Ok(task);
    }
    if let Some(child_thread_id) = args
        .get("child_thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return find_spawned_task_by_thread(agent, child_thread_id)
            .await
            .ok_or_else(|| {
                anyhow::anyhow!("spawned subagent not found for thread `{child_thread_id}`")
            });
    }
    resolve_current_spawned_task(agent, thread_id, task_id).await
}

async fn authorize_budget_extension(
    agent: &AgentEngine,
    child: &AgentTask,
    caller_thread_id: &str,
    caller_task_id: Option<&str>,
) -> Result<()> {
    if caller_task_id.is_some_and(|task_id| task_id == child.id) {
        return Ok(());
    }
    if child
        .thread_id
        .as_deref()
        .is_some_and(|thread_id| thread_id == caller_thread_id)
    {
        return Ok(());
    }
    if let Some(caller_task_id) = caller_task_id {
        if child.parent_task_id.as_deref() == Some(caller_task_id) {
            return Ok(());
        }
        if let Some(caller) = task_by_id_for_tool_scope(agent, caller_task_id).await {
            if caller.thread_id.as_deref() == child.parent_thread_id.as_deref()
                && child.parent_thread_id.is_some()
            {
                return Ok(());
            }
        }
    }
    if child
        .parent_thread_id
        .as_deref()
        .is_some_and(|parent_thread_id| parent_thread_id == caller_thread_id)
    {
        return Ok(());
    }
    anyhow::bail!(
        "extend_subagent_budget can be called by the spawned child or its parent thread/task"
    );
}

async fn find_spawned_task_by_thread(agent: &AgentEngine, thread_id: &str) -> Option<AgentTask> {
    let mut tasks = agent
        .list_tasks_filtered(&AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: None,
            thread_id: Some(thread_id.to_string()),
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: true,
            limit: Some(8),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await;
    {
        let live = agent.tasks.lock().await;
        for task in live
            .iter()
            .filter(|task| task.thread_id.as_deref() == Some(thread_id))
        {
            if !tasks.iter().any(|existing| existing.id == task.id) {
                tasks.push(task.clone());
            }
        }
    }
    tasks.into_iter().find(|task| task.is_spawned_subagent())
}

async fn wake_spawned_thread_after_budget_extension(
    agent: &AgentEngine,
    child: &AgentTask,
    additional_tokens: u32,
    reason: &str,
) {
    let Some(child_thread_id) = child.thread_id.as_deref() else {
        return;
    };
    let message = format!(
        "Execution budget extended by {additional_tokens} visible output tokens.\n\nReason: {reason}\nNew ceiling: {} tokens.\nContinue the assigned work from the last completed point. Reporting back still does not count against this budget.",
        child.context_budget_tokens.unwrap_or(0)
    );
    let _ = agent
        .append_system_thread_message(child_thread_id, message.clone())
        .await;
    let agent_id = agent
        .agent_scope_id_for_turn(Some(child_thread_id), Some(child.id.as_str()))
        .await;
    agent
        .enqueue_visible_thread_continuation(
            child_thread_id,
            DeferredVisibleThreadContinuation {
                agent_id,
                task_id: Some(child.id.clone()),
                preferred_session_hint: None,
                llm_user_content: format!(
                    "Your execution budget was extended. Continue the assigned work.\n\n{message}"
                ),
                queued_at_ms: 0,
                force_compaction: false,
                rerun_participant_observers_after_turn: false,
                internal_delegate_sender: None,
                internal_delegate_message: None,
            },
        )
        .await;
    let idle = {
        let streams = agent.stream_cancellations.lock().await;
        match streams.get(child_thread_id) {
            None => true,
            Some(entry) => entry.token.is_cancelled(),
        }
    };
    if idle {
        let _ = agent.stop_stream(child_thread_id).await;
    }
    if let Err(error) = agent
        .flush_deferred_visible_thread_continuations(child_thread_id)
        .await
    {
        tracing::warn!(
            thread_id = %child_thread_id,
            task_id = %child.id,
            %error,
            "failed to flush child continuation after budget extension"
        );
    }
}

#[cfg(test)]
#[path = "subagent_budget_tests.rs"]
mod tests;
