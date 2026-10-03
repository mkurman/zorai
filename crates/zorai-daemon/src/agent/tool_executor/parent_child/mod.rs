use super::*;
use crate::agent::types::TaskStatus;
use serde::{Deserialize, Serialize};

mod context;
mod handles;
mod model_handles;
mod tools;
mod wakeup;

pub(crate) use handles::{ensure_task_slug, resolve_child_reference, stored_task_slug};
pub(crate) use model_handles::{
    ensure_goal_slug, ensure_payload_slug, lookup_operation_id, model_operation_id,
    resolve_goal_id, resolve_payload_id,
};
pub(crate) use tools::{execute_answer_child, execute_ask_parent, execute_note_to_child};

/// blocked_reason prefix that marks a task as blocked on an open `ask_parent`.
/// stalled_turns classification exempts tasks whose reason starts with this.
pub(in crate::agent) const AWAITING_PARENT_BLOCKED_PREFIX: &str = "awaiting parent:";

const ASK_PARENT_STATE_PREFIX: &str = "ask_parent:";
const NOTES_STATE_PREFIX: &str = "notes_to_child:";
const NOTES_CURSOR_STATE_PREFIX: &str = "notes_to_child_cursor:";
const MAX_OPEN_ASKS_PER_CHILD: usize = 5;
const MAX_NOTES_PER_CHILD: usize = 20;
const MAX_NOTE_CHARS: usize = 2_000;
const DEFAULT_TIMEOUT_MINUTES: u64 = 240;
const LOG_PHASE: &str = "parent_child_messaging";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct AskParentRecord {
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
    pub asked_at: u64,
    pub timeout_minutes: u64,
    #[serde(default)]
    pub default: Option<String>,
    /// "open" | "answered" | "timeout_defaulted" | "timeout_unanswered"
    pub state: String,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub answer_delivered: bool,
    /// Short handle the parent passes to `answer_child` (`q1`, `q2`, ...).
    #[serde(default)]
    pub slug: String,
}

/// Pure, I/O-free timeout decision for one ask_parent record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AskTimeoutAction {
    None,
    TimeoutDefaulted,
    TimeoutUnanswered,
}

pub(crate) fn ask_timeout_action(record: &AskParentRecord, now: u64) -> AskTimeoutAction {
    if record.state != "open" {
        return AskTimeoutAction::None;
    }
    let deadline = record
        .asked_at
        .saturating_add(record.timeout_minutes.saturating_mul(60_000));
    if now < deadline {
        return AskTimeoutAction::None;
    }
    let has_default = record
        .default
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if has_default {
        AskTimeoutAction::TimeoutDefaulted
    } else {
        AskTimeoutAction::TimeoutUnanswered
    }
}

/// Whether a task is currently blocked on an open ask_parent (stalled-turn exempt).
pub(crate) fn task_is_awaiting_parent(task: &AgentTask) -> bool {
    task.blocked_reason
        .as_deref()
        .map(|reason| {
            reason
                .trim_start()
                .starts_with(AWAITING_PARENT_BLOCKED_PREFIX)
        })
        .unwrap_or(false)
}

fn ask_state_prefix(child_task_id: &str) -> String {
    format!("{ASK_PARENT_STATE_PREFIX}{child_task_id}:")
}

/// Split an ask state key into (child_task_id, ask_id).
fn split_ask_key(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix(ASK_PARENT_STATE_PREFIX)?
        .rsplit_once(':')
        .filter(|(child_id, ask_id)| !child_id.is_empty() && !ask_id.is_empty())
}

pub(crate) async fn list_ask_records(
    agent: &AgentEngine,
    child_task_id: &str,
) -> Result<Vec<(String, AskParentRecord)>> {
    let rows = agent
        .history
        .list_consolidation_state_by_prefix(&ask_state_prefix(child_task_id))
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(key, value)| {
            serde_json::from_str::<AskParentRecord>(&value)
                .ok()
                .map(|record| (key, record))
        })
        .collect())
}

async fn persist_ask_record(
    agent: &AgentEngine,
    key: &str,
    record: &AskParentRecord,
) -> Result<()> {
    let value = serde_json::to_string(record)?;
    agent
        .history
        .set_consolidation_state(key, &value, now_millis())
        .await
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

async fn resolve_caller_task(
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Option<AgentTask> {
    match task_id {
        Some(task_id) => task_by_id_for_tool_scope(agent, task_id).await,
        None => {
            let tasks = agent.tasks.lock().await;
            tasks
                .iter()
                .find(|task| task.thread_id.as_deref() == Some(thread_id))
                .cloned()
        }
    }
}

/// Validate that the caller (parent task or parent thread surface) may answer
/// or note the given child.
fn caller_is_parent(child: &AgentTask, caller_task: Option<&AgentTask>, thread_id: &str) -> bool {
    if let Some(parent_task_id) = child.parent_task_id.as_deref() {
        if caller_task
            .map(|task| task.id == parent_task_id)
            .unwrap_or(false)
        {
            return true;
        }
    }
    if let Some(parent_thread_id) = child.parent_thread_id.as_deref() {
        if thread_id == parent_thread_id {
            return true;
        }
        if caller_task.and_then(|task| task.thread_id.as_deref()) == Some(parent_thread_id) {
            return true;
        }
    }
    false
}

async fn unblock_child_task(agent: &AgentEngine, child: &mut AgentTask) -> bool {
    if child.status != TaskStatus::Blocked {
        return false;
    }
    let stream_active = {
        let streams = agent.stream_cancellations.lock().await;
        match child.thread_id.as_deref().and_then(|tid| streams.get(tid)) {
            Some(entry) => !entry.token.is_cancelled(),
            None => false,
        }
    };
    child.status = if stream_active {
        TaskStatus::InProgress
    } else {
        TaskStatus::Queued
    };
    child.blocked_reason = None;
    child.completed_at = None;
    child.error = None;
    child.last_error = None;
    child.next_retry_at = None;
    true
}

fn notes_cursor_after_eviction(delivered: usize, evicted: usize) -> usize {
    delivered.saturating_sub(evicted)
}

fn select_open_ask(
    open: &[(String, AskParentRecord)],
    requested_ask_id: Option<&str>,
) -> Result<(String, AskParentRecord)> {
    if let Some(ask_id) = requested_ask_id {
        return open
            .iter()
            .find(|(key, record)| handles::ask_ref_matches(key, record, ask_id))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no open ask_parent record with ask_id {ask_id}; it may already be answered or timed out"
                )
            });
    }
    match open {
        [] => anyhow::bail!(
            "no open ask_parent record exists; it may already be answered or timed out"
        ),
        [single] => Ok(single.clone()),
        rest => anyhow::bail!(
            "{} open ask_parent records exist; pass ask_id to answer a specific question instead of applying one answer to every outstanding ask",
            rest.len()
        ),
    }
}

fn remaining_open_count(records: &[(String, AskParentRecord)], answered_key: &str) -> usize {
    records
        .iter()
        .filter(|(key, record)| key != answered_key && record.state == "open")
        .count()
}

#[cfg(test)]
mod tests;
