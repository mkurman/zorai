#![allow(dead_code)]

use super::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const MAX_IDLE_CONTINUATION_DRAIN_MISSES: u32 = 3;

fn continuation_signatures(items: &[DeferredVisibleThreadContinuation]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|item| (item.agent_id.clone(), item.llm_user_content.clone()))
        .collect()
}

fn idle_continuation_drain_misses(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, u32>> {
    static MISSES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, u32>>> =
        std::sync::OnceLock::new();
    MISSES.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantCommandAction {
    Upsert,
    Deactivate,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadParticipantState {
    pub agent_id: String,
    pub agent_name: String,
    pub instruction: String,
    pub status: ThreadParticipantStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deactivated_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_contribution_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_observed_visible_message_at: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub always_auto_response: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_contributed_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "u32_is_zero")]
    pub duplicate_suppression_streak: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_suppressed_at: Option<u64>,
}

fn u32_is_zero(value: &u32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantSuggestionStatus {
    Queued,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantSuggestionKind {
    #[default]
    PreparedMessage,
    AutoResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadParticipantSuggestion {
    pub id: String,
    pub target_agent_id: String,
    pub target_agent_name: String,
    pub instruction: String,
    #[serde(default)]
    pub suggestion_kind: ThreadParticipantSuggestionKind,
    #[serde(default)]
    pub force_send: bool,
    pub status: ThreadParticipantSuggestionStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_send_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_message_timestamp: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

const AUTO_PARTICIPANT_RESPONSE_DELAY_MS: u64 = 60_000;

fn auto_response_request_text() -> &'static str {
    "Respond to the latest main agent message on this thread and continue the same workstream. Keep the reply concrete, useful, and aligned with your participant instruction."
}

fn is_auto_response_request_prompt(instruction: &str) -> bool {
    let trimmed = instruction.trim();
    trimmed == auto_response_request_text()
        || trimmed.starts_with("Respond to the latest main agent message")
}

pub(super) fn normalize_thread_participants(
    participants: Vec<ThreadParticipantState>,
) -> Vec<ThreadParticipantState> {
    participants
        .into_iter()
        .map(|mut participant| {
            if is_known_builtin_agent_alias(&participant.agent_id) {
                let canonical_id = canonical_agent_id(&participant.agent_id).to_string();
                participant.agent_id = canonical_id.clone();
                if participant.agent_name.trim().is_empty() {
                    participant.agent_name = canonical_agent_name(&canonical_id).to_string();
                }
            }
            participant
        })
        .collect()
}

fn is_known_main_agent_alias(alias: &str) -> bool {
    matches!(
        alias.trim().to_ascii_lowercase().as_str(),
        MAIN_AGENT_ID
            | crate::agent::agent_identity::MAIN_AGENT_PUBLIC_ALIAS
            | MAIN_AGENT_ALIAS
            | MAIN_AGENT_LEGACY_ALIAS
            | MAIN_AGENT_FALLBACK_ALIAS
    )
}

fn is_known_builtin_agent_alias(alias: &str) -> bool {
    let trimmed = alias.trim();
    !trimmed.is_empty()
        && (canonical_agent_id(trimmed) != MAIN_AGENT_ID || is_known_main_agent_alias(trimmed))
}

fn is_participant_stop_action(action: &str) -> bool {
    matches!(
        action.trim().to_ascii_lowercase().as_str(),
        "deactivate" | "stop"
    )
}

fn is_participant_remove_action(action: &str) -> bool {
    matches!(
        action.trim().to_ascii_lowercase().as_str(),
        "leave" | "done" | "return" | "remove"
    )
}

pub(crate) fn spawn_continuation_flush_worker(
    engine: Arc<AgentEngine>,
    mut wake_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    tokio::spawn(async move {
        while let Some(thread_id) = wake_rx.recv().await {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move {
                if engine.thread_is_idle_for_subagent_wakeup(&thread_id).await {
                    let _ = engine.stop_stream(&thread_id).await;
                }
                if let Err(error) = engine
                    .flush_deferred_visible_thread_continuations(&thread_id)
                    .await
                {
                    tracing::warn!(
                        thread_id = %thread_id,
                        %error,
                        "failed to flush scheduled thread continuation"
                    );
                }
            });
        }
    });
}

impl AgentEngine {
    async fn ensure_thread_shell_live_or_persisted(&self, thread_id: &str) -> Result<bool> {
        if self.threads.read().await.contains_key(thread_id) {
            return Ok(true);
        }

        let Some(restored) = self.restore_thread_from_db(thread_id).await else {
            return Ok(false);
        };
        self.threads
            .write()
            .await
            .insert(thread_id.to_string(), restored);
        Ok(true)
    }

    async fn set_thread_participant_always_auto_response(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        enabled: bool,
    ) -> Result<bool> {
        if !self
            .ensure_thread_shell_live_or_persisted(thread_id)
            .await?
        {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let mut participants = self.thread_participants.write().await;
        let Some(entry) = participants.get_mut(thread_id) else {
            anyhow::bail!("thread not found: {thread_id}");
        };

        let mut changed = false;
        let mut found_target = false;
        let now = now_millis();
        for participant in entry.iter_mut() {
            let should_enable = enabled
                && participant.status == ThreadParticipantStatus::Active
                && participant.agent_id.eq_ignore_ascii_case(target_agent_id);
            if participant.agent_id.eq_ignore_ascii_case(target_agent_id)
                && participant.status == ThreadParticipantStatus::Active
            {
                found_target = true;
            }
            if participant.always_auto_response != should_enable {
                participant.always_auto_response = should_enable;
                participant.updated_at = now;
                changed = true;
            }
        }
        drop(participants);

        if enabled && !found_target {
            anyhow::bail!("participant is not active on thread: {target_agent_id}");
        }
        if changed {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }
        Ok(changed)
    }

    async fn run_deferred_visible_thread_continuation(
        &self,
        thread_id: &str,
        continuation: DeferredVisibleThreadContinuation,
    ) -> Result<()> {
        if continuation.internal_delegate_sender.is_none()
            && continuation.internal_delegate_message.is_none()
            && !continuation.force_compaction
            && self
                .deferred_visible_thread_continuation_obsoleted(thread_id, &continuation)
                .await
        {
            tracing::info!(
                thread_id = %thread_id,
                agent_id = %continuation.agent_id,
                queued_at_ms = continuation.queued_at_ms,
                "skipping stale deferred visible-thread continuation"
            );
            return Ok(());
        }

        if let (Some(sender), Some(message)) = (
            continuation.internal_delegate_sender.as_deref(),
            continuation.internal_delegate_message.as_deref(),
        ) {
            Box::pin(self.send_internal_agent_message(
                sender,
                &continuation.agent_id,
                message,
                continuation.preferred_session_hint.as_deref(),
            ))
            .await?;
        }
        Box::pin(self.continue_visible_thread_as_agent(
            thread_id,
            &continuation.agent_id,
            continuation.task_id.as_deref(),
            continuation.preferred_session_hint.as_deref(),
            &continuation.llm_user_content,
            continuation.force_compaction,
            continuation.rerun_participant_observers_after_turn,
        ))
        .await?;
        Ok(())
    }

    async fn deferred_visible_thread_continuation_obsoleted(
        &self,
        thread_id: &str,
        continuation: &DeferredVisibleThreadContinuation,
    ) -> bool {
        if continuation.queued_at_ms == 0 {
            return false;
        }
        let target_agent_id = canonical_agent_id(&continuation.agent_id);
        match self
            .history
            .visible_continuation_obsoleted_by_progress(
                thread_id,
                continuation.queued_at_ms,
                target_agent_id,
            )
            .await
        {
            Ok(obsoleted) => obsoleted,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    target_agent_id,
                    "failed to query visible continuation obsolescence from history: {error}"
                );
                false
            }
        }
    }

    async fn clear_thread_participant_suggestions_for_agent(
        &self,
        thread_id: &str,
        agent_id: &str,
    ) -> bool {
        let mut suggestions = self.thread_participant_suggestions.write().await;
        let Some(entry) = suggestions.get_mut(thread_id) else {
            return false;
        };
        let initial_len = entry.len();
        entry.retain(|suggestion| !suggestion.target_agent_id.eq_ignore_ascii_case(agent_id));
        let changed = entry.len() != initial_len;
        if entry.is_empty() {
            suggestions.remove(thread_id);
        }
        changed
    }

    pub(super) async fn mark_thread_participant_observed_visible_message(
        &self,
        thread_id: &str,
        agent_id: &str,
        visible_message_timestamp: u64,
    ) -> bool {
        let mut participants = self.thread_participants.write().await;
        let Some(entry) = participants.get_mut(thread_id) else {
            return false;
        };
        let Some(participant) = entry
            .iter_mut()
            .find(|participant| participant.agent_id.eq_ignore_ascii_case(agent_id))
        else {
            return false;
        };
        if participant
            .last_observed_visible_message_at
            .is_some_and(|timestamp| timestamp >= visible_message_timestamp)
        {
            return false;
        }

        participant.last_observed_visible_message_at = Some(visible_message_timestamp);
        participant.updated_at = now_millis();
        true
    }

    pub(super) async fn mark_thread_participant_epoch_consumed(
        &self,
        thread_id: &str,
        agent_id: &str,
        epoch: u64,
    ) -> bool {
        let mut participants = self.thread_participants.write().await;
        let Some(participant) = participants.get_mut(thread_id).and_then(|entry| {
            entry
                .iter_mut()
                .find(|participant| participant.agent_id.eq_ignore_ascii_case(agent_id))
        }) else {
            return false;
        };
        if participant
            .last_contributed_epoch
            .is_some_and(|existing| existing >= epoch)
        {
            return false;
        }
        participant.last_contributed_epoch = Some(epoch);
        participant.updated_at = now_millis();
        true
    }

    pub(super) async fn record_thread_participant_duplicate_suppression(
        &self,
        thread_id: &str,
        agent_id: &str,
    ) -> bool {
        let mut participants = self.thread_participants.write().await;
        let Some(participant) = participants.get_mut(thread_id).and_then(|entry| {
            entry
                .iter_mut()
                .find(|participant| participant.agent_id.eq_ignore_ascii_case(agent_id))
        }) else {
            return false;
        };
        let now = now_millis();
        participant.duplicate_suppression_streak =
            participant.duplicate_suppression_streak.saturating_add(1);
        participant.last_suppressed_at = Some(now);
        participant.updated_at = now;
        true
    }

    pub(super) async fn reset_thread_participant_duplicate_suppressions(
        &self,
        thread_id: &str,
        agent_id: &str,
    ) -> bool {
        let mut participants = self.thread_participants.write().await;
        let Some(participant) = participants.get_mut(thread_id).and_then(|entry| {
            entry
                .iter_mut()
                .find(|participant| participant.agent_id.eq_ignore_ascii_case(agent_id))
        }) else {
            return false;
        };
        if participant.duplicate_suppression_streak == 0 && participant.last_suppressed_at.is_none()
        {
            return false;
        }
        participant.duplicate_suppression_streak = 0;
        participant.last_suppressed_at = None;
        participant.updated_at = now_millis();
        true
    }

    pub(in crate::agent) async fn resolve_thread_participant_target(
        &self,
        alias: &str,
    ) -> Result<(String, String)> {
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            anyhow::bail!("target agent cannot be empty");
        }

        if is_known_builtin_agent_alias(trimmed) {
            let canonical_id = canonical_agent_id(trimmed).to_string();
            return Ok((
                canonical_id.clone(),
                canonical_agent_name(&canonical_id).to_string(),
            ));
        }

        let normalized = trimmed.to_ascii_lowercase();
        if let Some(sub_agent) = self.list_sub_agents().await.into_iter().find(|entry| {
            entry.id.eq_ignore_ascii_case(&normalized) || entry.name.eq_ignore_ascii_case(trimmed)
        }) {
            return Ok((sub_agent.id, sub_agent.name));
        }

        anyhow::bail!("unknown agent target: {trimmed}");
    }

    pub(crate) async fn latest_visible_user_message_content(
        &self,
        thread_id: &str,
    ) -> Option<String> {
        let from_history = match self.history.latest_user_message_content(thread_id).await {
            Ok(message) => message,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    "failed to load latest visible user message from history: {error}"
                );
                None
            }
        };
        if from_history.is_some() {
            return from_history;
        }
        // Fall back to the live in-memory thread when history has no
        // user message yet — covers the race between a freshly-arrived
        // user turn and the background persist that flushes it to
        // SQLite. Callers (e.g. `force_compact_and_continue`) need the
        // most recent user content to continue the conversation; failing
        // back on "history is empty" rather than only on "history
        // errored" makes that semantic robust to the persistence lag.
        let threads = self.threads.read().await;
        let thread = threads.get(thread_id)?;
        thread
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.content.clone())
    }

    async fn latest_visible_participant_message(
        &self,
        thread_id: &str,
    ) -> Option<(String, String, String)> {
        let (participant_id, participant_name, content) = match self
            .history
            .latest_participant_assistant_message(thread_id)
            .await
        {
            Ok(Some(message)) => message,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    "failed to load latest visible participant message from history: {error}"
                );
                return None;
            }
        };
        let participant_name =
            participant_name.unwrap_or_else(|| canonical_agent_name(&participant_id).to_string());
        Some((participant_id, participant_name, content))
    }

    pub(in crate::agent) async fn build_internal_delegate_payload(
        &self,
        thread_id: Option<&str>,
        content: &str,
        request_visible_thread_continuation: bool,
    ) -> String {
        let mut payload = String::new();
        if let Some(thread_id) = thread_id {
            payload.push_str(&format!(
                "Visible thread id: {thread_id}\nThread delegation mode: internal_hidden\n"
            ));
            payload.push_str(&format!(
                "Continuation requested on visible thread: {}\n",
                if request_visible_thread_continuation {
                    "yes"
                } else {
                    "no"
                }
            ));
            payload.push('\n');
            let loaded_projected_context = match self
                .history
                .thread_delegate_payload_context(thread_id, 8)
                .await
            {
                Ok(Some(context)) => {
                    payload.push_str(&format!("Visible thread title: {}\n", context.title.trim()));
                    if !context.messages.is_empty() {
                        payload.push_str("Recent visible thread messages:\n");
                        for message in context.messages {
                            let role = match message.role.as_str() {
                                "assistant" => "assistant",
                                "system" => "system",
                                "tool" => "tool",
                                _ => "user",
                            };
                            payload.push_str(&format!("- {role}: {}\n", message.content.trim()));
                        }
                        payload.push('\n');
                    }
                    true
                }
                Ok(None) => false,
                Err(error) => {
                    tracing::warn!(
                        thread_id = %thread_id,
                        "failed to load visible thread delegate context from history: {error}"
                    );
                    false
                }
            };
            if !loaded_projected_context {
                if let Some(thread) = self.get_thread(thread_id).await {
                    payload.push_str(&format!("Visible thread title: {}\n", thread.title.trim()));
                    let recent_messages = thread
                        .messages
                        .iter()
                        .rev()
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>();
                    if !recent_messages.is_empty() {
                        payload.push_str("Recent visible thread messages:\n");
                        for message in recent_messages {
                            let role = match message.role {
                                MessageRole::Assistant => "assistant",
                                MessageRole::System => "system",
                                MessageRole::Tool => "tool",
                                _ => "user",
                            };
                            payload.push_str(&format!("- {role}: {}\n", message.content.trim()));
                        }
                        payload.push('\n');
                    }
                }
            }
        }
        payload.push_str("Delegation request:\n");
        payload.push_str(content.trim());
        payload
    }

    pub(in crate::agent) async fn build_visible_thread_continuation_prompt(
        &self,
        thread_id: &str,
        sender: &str,
        target_agent_id: &str,
        content: &str,
    ) -> String {
        let sender_name = canonical_agent_name(sender);
        let target_agent_name = canonical_agent_name(target_agent_id);
        let latest_operator_request = self
            .latest_visible_user_message_content(thread_id)
            .await
            .unwrap_or_default();
        if latest_operator_request.trim().is_empty() {
            format!(
                "Continue the visible operator thread as {}. This continuation was explicitly requested in an internal DM from {}. The internal DM may continue hidden coordination or tool execution in parallel if needed.\n\nInternal delegation request:\n{}",
                target_agent_name,
                sender_name,
                content.trim()
            )
        } else {
            format!(
                "Continue the visible operator thread as {}. This continuation was explicitly requested in an internal DM from {}. The internal DM may continue hidden coordination or tool execution in parallel if needed.\n\nInternal delegation request:\n{}\n\nLatest operator request already on this thread:\n{}",
                target_agent_name,
                sender_name,
                content.trim(),
                latest_operator_request.trim()
            )
        }
    }

    pub(in crate::agent) async fn build_participant_follow_up_continuation_prompt(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        participant_name: &str,
        participant_message: &str,
    ) -> String {
        let target_agent_name = canonical_agent_name(target_agent_id);
        let latest_operator_request = self
            .latest_visible_user_message_content(thread_id)
            .await
            .unwrap_or_default();
        if latest_operator_request.trim().is_empty() {
            format!(
                "Continue the visible operator thread as {}. A thread participant ({}) just posted a visible message. Treat that participant contribution as the latest actionable context and continue the same task flow from there instead of restarting from an older user turn. Keep follow-up work visible from this thread: if specialist help is needed, prefer `spawn_subagent` or other visible thread-native actions over hidden internal DMs. Do not stop after only sending `message_agent`; produce visible progress on this thread. If the participant contribution contains no new actionable information, do not post a status update or acknowledgement in response; end the turn without a visible message.\n\nLatest participant contribution:\n{}",
                target_agent_name,
                participant_name.trim(),
                participant_message.trim()
            )
        } else {
            format!(
                "Continue the visible operator thread as {}. A thread participant ({}) just posted a visible message. Treat that participant contribution as the latest actionable context and continue the same task flow from there instead of restarting from an older user turn. Keep follow-up work visible from this thread: if specialist help is needed, prefer `spawn_subagent` or other visible thread-native actions over hidden internal DMs. Do not stop after only sending `message_agent`; produce visible progress on this thread. If the participant contribution contains no new actionable information, do not post a status update or acknowledgement in response; end the turn without a visible message.\n\nLatest participant contribution:\n{}\n\nLatest operator request already on this thread:\n{}",
                target_agent_name,
                participant_name.trim(),
                participant_message.trim(),
                latest_operator_request.trim()
            )
        }
    }

    pub(in crate::agent) async fn enqueue_visible_thread_continuation(
        &self,
        thread_id: &str,
        mut continuation: DeferredVisibleThreadContinuation,
    ) {
        if continuation.task_id.is_none() {
            let active_statuses = [
                TaskStatus::Queued,
                TaskStatus::InProgress,
                TaskStatus::AwaitingApproval,
                TaskStatus::Blocked,
                TaskStatus::FailedAnalyzing,
                TaskStatus::BudgetExceeded,
            ]
            .into_iter()
            .filter_map(|status| {
                serde_json::to_value(status)
                    .ok()
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
            })
            .collect::<Vec<_>>();
            let query = crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: active_statuses,
                source: None,
                thread_id: Some(thread_id.to_string()),
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: false,
                order_by_recent_activity_desc: true,
                limit: Some(1),
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            };
            continuation.task_id = match self.history.list_agent_task_refs_filtered(&query).await {
                Ok(task_refs) => task_refs.into_iter().next().map(|(task_id, _, _)| task_id),
                Err(error) => {
                    tracing::warn!(
                        thread_id,
                        %error,
                        "failed to query continuation task id from history"
                    );
                    self.list_tasks_filtered(&query)
                        .await
                        .into_iter()
                        .next()
                        .map(|task| task.id)
                }
            };
        }
        let mut queued = self.deferred_visible_thread_continuations.lock().await;
        let entry = queued.entry(thread_id.to_string()).or_default();
        if continuation.queued_at_ms == 0 {
            continuation.queued_at_ms = now_millis();
        }
        if entry
            .iter()
            .any(|queued| queued.same_request_as(&continuation))
        {
            tracing::info!(
                thread_id = %thread_id,
                agent_id = %continuation.agent_id,
                "skipping duplicate deferred visible-thread continuation"
            );
            return;
        }
        entry.push(continuation);
    }

    pub(in crate::agent) async fn clear_deferred_visible_thread_continuations(
        &self,
        thread_id: &str,
    ) -> usize {
        let mut queued = self.deferred_visible_thread_continuations.lock().await;
        let removed = queued
            .remove(thread_id)
            .map(|items| items.len())
            .unwrap_or(0);
        if removed > 0 {
            tracing::info!(
                thread_id = %thread_id,
                removed,
                "cleared deferred visible-thread continuations"
            );
        }
        removed
    }

    pub(in crate::agent) async fn clear_deferred_visible_thread_continuations_for_threads(
        &self,
        thread_ids: &std::collections::HashSet<String>,
    ) -> usize {
        let mut queued = self.deferred_visible_thread_continuations.lock().await;
        thread_ids
            .iter()
            .filter_map(|thread_id| queued.remove(thread_id))
            .map(|items| items.len())
            .sum()
    }

    pub(crate) async fn deferred_visible_thread_continuations_for(
        &self,
        thread_id: &str,
    ) -> Vec<DeferredVisibleThreadContinuation> {
        let queued = self.deferred_visible_thread_continuations.lock().await;
        queued.get(thread_id).cloned().unwrap_or_default()
    }

    async fn goal_blocks_visible_continuation(&self, thread_id: &str) -> bool {
        self.goal_for_execution_thread(thread_id)
            .await
            .is_some_and(|goal| goal.status == GoalRunStatus::Paused || goal.status.is_terminal())
    }

    async fn goal_for_execution_thread(&self, thread_id: &str) -> Option<GoalRun> {
        let refs = self
            .history
            .list_goal_run_thread_refs_for_thread_ids(&[thread_id.to_string()])
            .await
            .ok()?;
        let goal_id = refs.into_iter().next()?.id;
        self.get_goal_run(&goal_id).await
    }

    pub(crate) fn schedule_continuation_flush(&self, thread_id: &str) {
        let _ = self.continuation_flush_tx.send(thread_id.to_string());
    }

    pub(in crate::agent) async fn flush_deferred_visible_thread_continuations(
        &self,
        thread_id: &str,
    ) -> Result<()> {
        let force_compaction_queued = {
            let queued = self.deferred_visible_thread_continuations.lock().await;
            queued.get(thread_id).is_some_and(|items| {
                items
                    .iter()
                    .any(|continuation| continuation.force_compaction)
            })
        };
        if let Some(goal) = self.goal_for_execution_thread(thread_id).await {
            if (goal.status == GoalRunStatus::Paused || goal.status.is_terminal())
                && !force_compaction_queued
            {
                tracing::debug!(goal_run_id = %goal.id, thread_id, "not flushing deferred continuation for paused or terminal goal");
                return Ok(());
            }
            if force_compaction_queued
                && (goal.status == GoalRunStatus::Paused || goal.status.is_terminal())
            {
                tracing::info!(
                    goal_run_id = %goal.id,
                    thread_id,
                    status = ?goal.status,
                    "flushing operator compaction on a paused or finished goal"
                );
            }
        }
        if self.operator_stream_stop_requested(thread_id).await {
            let removed = self
                .clear_deferred_visible_thread_continuations(thread_id)
                .await;
            tracing::info!(
                thread_id,
                removed,
                "dropping deferred continuation because the operator stopped the stream"
            );
            return Ok(());
        }
        let provider = self.config.read().await.provider.clone();
        if !self.provider_circuit_is_closed(&provider).await {
            tracing::info!(
                thread_id,
                provider,
                "skipping deferred continuation flush while the provider circuit breaker is open"
            );
            return Ok(());
        }
        {
            // Mirror the loop's stream-state check below: a live stream
            // blocks the flush, but a cancelled-but-not-yet-removed
            // entry (e.g. set up by a force-send interrupt that called
            // `stop_stream`) should not.
            let streams = self.stream_cancellations.lock().await;
            if let Some(entry) = streams.get(thread_id) {
                if !entry.token.is_cancelled() {
                    tracing::debug!(
                        thread_id = %thread_id,
                        last_progress_at = entry.last_progress_at,
                        "skipping deferred continuation flush because a live stream is active"
                    );
                    return Ok(());
                }
            }
        }

        let acquired_flush_slot = {
            let mut active = lock_flush_slots(&self.active_visible_thread_continuation_flushes);
            active.insert(thread_id.to_string())
        };
        if !acquired_flush_slot {
            tracing::debug!(
                thread_id = %thread_id,
                "skipping deferred continuation flush because another flush holds the slot"
            );
            return Ok(());
        }

        let mut slot_guard = FlushSlotGuard::new(
            &self.active_visible_thread_continuation_flushes,
            thread_id.to_string(),
        );

        let result = async {
            loop {
                // A *live* stream entry blocks the flush — the
                // continuation should wait for the active stream to wrap
                // up before queueing more work onto the same thread.
                // But a *cancelled* entry (e.g. left behind by
                // `stop_stream` because the stream task hasn't returned
                // yet, or registered by a force-send interrupt) must
                // not deadlock the flush forever: the stream the entry
                // referred to is already going away.
                {
                    let streams = self.stream_cancellations.lock().await;
                    if let Some(entry) = streams.get(thread_id) {
                        if !entry.token.is_cancelled() {
                            tracing::debug!(
                                thread_id = %thread_id,
                                last_progress_at = entry.last_progress_at,
                                "stopping deferred continuation drain because a live stream is active"
                            );
                            break;
                        }
                    }
                }

                let continuations = {
                    let mut queued = self.deferred_visible_thread_continuations.lock().await;
                    queued.remove(thread_id).unwrap_or_default()
                };
                if continuations.is_empty() {
                    break;
                }
                for continuation in continuations {
                    Box::pin(
                        self.run_deferred_visible_thread_continuation(thread_id, continuation),
                    )
                    .await?;
                }
            }
            Ok(())
        }
        .await;

        slot_guard.release().await;
        result
    }

    pub(in crate::agent) async fn supervise_idle_deferred_thread_continuations(
        &self,
    ) -> Result<()> {
        let queued_thread_ids = {
            let queued = self.deferred_visible_thread_continuations.lock().await;
            queued
                .iter()
                .filter(|(_, items)| !items.is_empty())
                .map(|(thread_id, _)| thread_id.clone())
                .collect::<Vec<_>>()
        };
        let provider = self.config.read().await.provider.clone();
        let circuit_closed = self.provider_circuit_is_closed(&provider).await;
        for thread_id in queued_thread_ids {
            if !circuit_closed {
                tracing::info!(
                    thread_id,
                    provider,
                    "leaving deferred continuation queued while the provider circuit breaker is open"
                );
                continue;
            }
            if !self.thread_is_idle_for_subagent_wakeup(&thread_id).await {
                continue;
            }
            let queued_before = continuation_signatures(
                &self
                    .deferred_visible_thread_continuations_for(&thread_id)
                    .await,
            );
            tracing::info!(
                thread_id = %thread_id,
                "flushing deferred continuation on idle thread"
            );
            let _ = self.stop_stream(&thread_id).await;
            if let Err(error) = self
                .flush_deferred_visible_thread_continuations(&thread_id)
                .await
            {
                tracing::warn!(
                    thread_id = %thread_id,
                    error = %error,
                    "idle deferred continuation flush failed"
                );
            }
            let queued_after = continuation_signatures(
                &self
                    .deferred_visible_thread_continuations_for(&thread_id)
                    .await,
            );
            let drop_stuck = {
                let mut misses = idle_continuation_drain_misses()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if queued_after.is_empty() || queued_after != queued_before {
                    misses.remove(&thread_id);
                    false
                } else {
                    let count = misses.entry(thread_id.clone()).or_insert(0);
                    *count = count.saturating_add(1);
                    if *count >= MAX_IDLE_CONTINUATION_DRAIN_MISSES {
                        misses.remove(&thread_id);
                        true
                    } else {
                        false
                    }
                }
            };
            if drop_stuck {
                let removed = self
                    .clear_deferred_visible_thread_continuations(&thread_id)
                    .await;
                tracing::warn!(
                    thread_id = %thread_id,
                    removed,
                    "dropped stuck deferred continuations after repeated idle drains"
                );
                let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                    thread_id: thread_id.clone(),
                    kind: "deferred_continuation_dropped".to_string(),
                    message: "Stopped retrying a stuck idle continuation.".to_string(),
                    details: None,
                });
            }
        }
        Ok(())
    }

    async fn continue_visible_thread_as_agent(
        &self,
        thread_id: &str,
        agent_id: &str,
        task_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        llm_user_content: &str,
        force_compaction: bool,
        rerun_participant_observers_after_turn: bool,
    ) -> Result<SendMessageOutcome> {
        if !self.ensure_thread_messages_loaded(thread_id).await {
            anyhow::bail!("thread not found: {thread_id}");
        }

        if force_compaction {
            let config = self.config.read().await.clone();
            let (provider_config, task_override_applied) = if let Some(task_id) = task_id {
                let override_provider = self.task_provider_override_for_compaction(task_id).await;
                if let Some((provider_id, model_override)) = override_provider {
                    let mut provider_config =
                        self.resolve_sub_agent_provider_config(&config, &provider_id)?;
                    if let Some(model_override) = model_override.as_deref() {
                        apply_provider_model_override(
                            &provider_id,
                            &mut provider_config,
                            model_override,
                        );
                    }
                    (provider_config, true)
                } else {
                    (self.resolve_provider_config(&config)?, false)
                }
            } else {
                (self.resolve_provider_config(&config)?, false)
            };
            let provider_config = if task_override_applied {
                provider_config
            } else if let Some((_, profile_provider)) = self
                .thread_profile_compaction_provider(thread_id, &config)
                .await?
            {
                profile_provider
            } else {
                provider_config
            };
            let compacted = self
                .force_persist_compaction_artifact(thread_id, task_id, &config, &provider_config)
                .await?;
            if !compacted {
                tracing::info!(
                    thread_id,
                    "manual compaction skipped; no older context slice"
                );
                let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                    thread_id: thread_id.to_string(),
                    kind: "manual-compaction".to_string(),
                    message:
                        "Manual compaction skipped; there was no older context slice to compact."
                            .to_string(),
                    details: None,
                });
            }
            if self.goal_blocks_visible_continuation(thread_id).await {
                tracing::info!(
                    thread_id,
                    "compaction finished without resuming a paused or finished goal"
                );
                return Ok(SendMessageOutcome {
                    thread_id: thread_id.to_string(),
                    stream_generation: 0,
                    interrupted_for_approval: false,
                    terminated_for_budget: false,
                    subagent_report: None,
                    upstream_message: None,
                    provider_final_result: None,
                    fresh_runner_retry: None,
                    handoff_restart: None,
                });
            }
        }

        let stored_user_content = self
            .latest_visible_user_message_content(thread_id)
            .await
            .unwrap_or_else(|| llm_user_content.to_string());
        let mut current_thread_id = thread_id.to_string();
        let mut current_llm_user_content = llm_user_content.to_string();
        let mut current_agent_scope_id = agent_turn_scope_id(agent_id);

        loop {
            let thread_for_turn = current_thread_id.clone();
            let continuation_thread_id = thread_for_turn.clone();
            let stored_user_content_for_turn = stored_user_content.clone();
            let llm_user_content_for_turn = current_llm_user_content.clone();
            let client_surface_for_turn = self.get_thread_client_surface(&thread_for_turn).await;
            let outcome = match Box::pin(run_with_agent_scope(
                current_agent_scope_id.clone(),
                async move {
                    Box::pin(self.run_internal_send_loop(
                        Some(thread_for_turn.as_str()),
                        &stored_user_content_for_turn,
                        &[],
                        &llm_user_content_for_turn,
                        task_id,
                        preferred_session_hint,
                        None,
                        client_surface_for_turn,
                        false,
                        true,
                    ))
                    .await
                },
            ))
            .await
            {
                Ok(outcome) => outcome,
                Err(error) if force_compaction => {
                    tracing::warn!(
                        thread_id = %continuation_thread_id,
                        error = %error,
                        "post-compaction continuation failed after the artifact was saved"
                    );
                    let _ = self.event_tx.send(AgentEvent::Error {
                        thread_id: continuation_thread_id.clone(),
                        message: error.to_string(),
                    });
                    return Ok(SendMessageOutcome {
                        thread_id: continuation_thread_id,
                        stream_generation: 0,
                        interrupted_for_approval: false,
                        terminated_for_budget: false,
                        subagent_report: None,
                        upstream_message: None,
                        provider_final_result: None,
                        fresh_runner_retry: None,
                        handoff_restart: None,
                    });
                }
                Err(error) => return Err(error),
            };

            if let Some(restart) = outcome.handoff_restart.clone() {
                current_thread_id = outcome.thread_id.clone();
                current_llm_user_content = restart.llm_user_content;
                current_agent_scope_id = self
                    .agent_scope_id_for_turn(Some(&current_thread_id), task_id)
                    .await;
                continue;
            }

            if !outcome.interrupted_for_approval {
                Box::pin(
                    self.maybe_auto_send_next_thread_participant_suggestion(&outcome.thread_id),
                )
                .await?;
                if rerun_participant_observers_after_turn {
                    if let Err(error) =
                        Box::pin(self.run_participant_observers(&outcome.thread_id)).await
                    {
                        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                            thread_id: outcome.thread_id.clone(),
                            kind: "participant_observer_error".to_string(),
                            message: "participant observers failed".to_string(),
                            details: Some(error.to_string()),
                        });
                    }
                }
            }
            return Ok(outcome);
        }
    }

    async fn task_provider_override_for_compaction(
        &self,
        task_id: &str,
    ) -> Option<(String, Option<String>)> {
        match self.history.agent_task_provider_override(task_id).await {
            Ok(provider_override) => provider_override,
            Err(error) => {
                tracing::warn!(
                    task_id,
                    %error,
                    "failed to query task provider override from history"
                );
                self.list_tasks_filtered(&crate::history::AgentTaskListQuery {
                    id: Some(task_id.to_string()),
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
                .into_iter()
                .next()
                .and_then(|task| {
                    task.override_provider
                        .map(|provider_id| (provider_id, task.override_model))
                })
            }
        }
    }

    pub async fn list_thread_participants(&self, thread_id: &str) -> Vec<ThreadParticipantState> {
        self.thread_participants
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn list_thread_participant_suggestions(
        &self,
        thread_id: &str,
    ) -> Vec<ThreadParticipantSuggestion> {
        self.thread_participant_suggestions
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn queue_thread_participant_suggestion(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        instruction: &str,
        force_send: bool,
    ) -> Result<ThreadParticipantSuggestion> {
        self.queue_thread_participant_suggestion_entry(
            thread_id,
            target_agent_id,
            instruction,
            force_send,
            ThreadParticipantSuggestionKind::PreparedMessage,
            None,
            None,
        )
        .await
    }

    pub(crate) async fn queue_thread_auto_response_suggestion(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        source_message_timestamp: u64,
    ) -> Result<Option<ThreadParticipantSuggestion>> {
        let generated = self
            .generate_visible_thread_participant_message(
                thread_id,
                target_agent_id,
                auto_response_request_text(),
            )
            .await?;
        let Some((_, message)) =
            crate::agent::thread_participant_runner::parse_participant_suggestion_response(
                &generated,
            )
        else {
            return Ok(None);
        };
        if crate::agent::thread_participant_runner::participant_message_is_status_ack(&message) {
            return Ok(None);
        }
        let due_at = now_millis().saturating_add(AUTO_PARTICIPANT_RESPONSE_DELAY_MS);
        self.queue_thread_participant_suggestion_entry(
            thread_id,
            target_agent_id,
            &message,
            false,
            ThreadParticipantSuggestionKind::AutoResponse,
            Some(due_at),
            Some(source_message_timestamp),
        )
        .await
        .map(Some)
    }

    pub(crate) async fn request_thread_auto_response_suggestion(
        &self,
        thread_id: &str,
        target_agent_id: &str,
    ) -> Result<bool> {
        let Some(source_message_timestamp) = self
            .latest_visible_main_agent_message_timestamp_for_auto_response(thread_id)
            .await
        else {
            return Ok(false);
        };

        let participants = self.list_thread_participants(thread_id).await;
        let Some(participant) = participants.iter().find(|participant| {
            participant.status == ThreadParticipantStatus::Active
                && participant.agent_id.eq_ignore_ascii_case(target_agent_id)
        }) else {
            anyhow::bail!("participant is not active on thread: {target_agent_id}");
        };

        if participant
            .last_observed_visible_message_at
            .is_some_and(|timestamp| timestamp >= source_message_timestamp)
        {
            return Ok(false);
        }

        let has_matching_auto_response = self
            .list_thread_participant_suggestions(thread_id)
            .await
            .into_iter()
            .any(|suggestion| {
                suggestion.suggestion_kind == ThreadParticipantSuggestionKind::AutoResponse
                    && suggestion.status == ThreadParticipantSuggestionStatus::Queued
                    && suggestion
                        .target_agent_id
                        .eq_ignore_ascii_case(target_agent_id)
                    && suggestion.source_message_timestamp == Some(source_message_timestamp)
            });
        if has_matching_auto_response {
            return Ok(false);
        }

        let Some(_) = self
            .queue_thread_auto_response_suggestion(
                thread_id,
                &participant.agent_id,
                source_message_timestamp,
            )
            .await?
        else {
            let state_changed = self
                .mark_thread_participant_observed_visible_message(
                    thread_id,
                    &participant.agent_id,
                    source_message_timestamp,
                )
                .await;
            if state_changed {
                self.persist_thread_by_id(thread_id).await;
            }
            return Ok(false);
        };
        let state_changed = self
            .mark_thread_participant_observed_visible_message(
                thread_id,
                &participant.agent_id,
                source_message_timestamp,
            )
            .await;
        if state_changed {
            self.persist_thread_by_id(thread_id).await;
        }
        Ok(true)
    }

    async fn queue_thread_participant_suggestion_entry(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        instruction: &str,
        force_send: bool,
        suggestion_kind: ThreadParticipantSuggestionKind,
        auto_send_at: Option<u64>,
        source_message_timestamp: Option<u64>,
    ) -> Result<ThreadParticipantSuggestion> {
        if !self
            .ensure_thread_shell_live_or_persisted(thread_id)
            .await?
        {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_instruction = instruction.trim();
        if trimmed_instruction.is_empty() {
            anyhow::bail!("participant suggestion instruction cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let participant_is_active =
            self.list_thread_participants(thread_id)
                .await
                .iter()
                .any(|participant| {
                    participant.agent_id.eq_ignore_ascii_case(&agent_id)
                        && participant.status == ThreadParticipantStatus::Active
                });
        if !participant_is_active {
            anyhow::bail!("participant is not active on thread: {agent_id}");
        }
        let now = now_millis();
        let suggestion = ThreadParticipantSuggestion {
            id: uuid::Uuid::new_v4().to_string(),
            target_agent_id: agent_id,
            target_agent_name: agent_name,
            instruction: trimmed_instruction.to_string(),
            suggestion_kind,
            force_send,
            status: ThreadParticipantSuggestionStatus::Queued,
            created_at: now,
            updated_at: now,
            auto_send_at,
            source_message_timestamp,
            error: None,
        };

        self.thread_participant_suggestions
            .write()
            .await
            .entry(thread_id.to_string())
            .or_default()
            .push(suggestion.clone());

        // A `force_send` participant suggestion is the user (or
        // orchestrator) asking the participant to interrupt the current
        // turn — cancel any active stream so the existing
        // `if suggestion.force_send` branch below (which calls
        // `send_thread_participant_suggestion`) can post the participant
        // message immediately rather than waiting behind a
        // still-running main agent generation. The post + active-agent
        // follow-up is owned by that existing dispatch, so we only nudge
        // stream state here.
        if force_send {
            self.stop_stream(thread_id).await;
        }

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ParticipantSuggestion {
            thread_id: thread_id.to_string(),
            suggestion: suggestion.clone(),
        });
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });
        let _ = self
            .record_behavioral_event(
                "participant_suggestion",
                BehavioralEventContext {
                    thread_id: Some(thread_id),
                    task_id: None,
                    goal_run_id: None,
                    approval_id: None,
                },
                serde_json::json!({
                    "action": "queued",
                    "suggestion": suggestion,
                }),
            )
            .await;

        if suggestion.force_send {
            self.send_thread_participant_suggestion(thread_id, &suggestion.id, None)
                .await?;
        } else {
            let _ = self
                .maybe_auto_send_next_thread_participant_suggestion(thread_id)
                .await?;
        }

        Ok(suggestion)
    }

    pub async fn dismiss_thread_participant_suggestion(
        &self,
        thread_id: &str,
        suggestion_id: &str,
    ) -> Result<bool> {
        if !self
            .ensure_thread_shell_live_or_persisted(thread_id)
            .await?
        {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let mut suggestions = self.thread_participant_suggestions.write().await;
        let (changed, removed_suggestion, remove_entry) = {
            let Some(entry) = suggestions.get_mut(thread_id) else {
                return Ok(false);
            };
            let initial_len = entry.len();
            let removed_suggestion = entry
                .iter()
                .find(|suggestion| suggestion.id == suggestion_id)
                .cloned();
            entry.retain(|suggestion| suggestion.id != suggestion_id);
            (
                entry.len() != initial_len,
                removed_suggestion,
                entry.is_empty(),
            )
        };
        if remove_entry {
            suggestions.remove(thread_id);
        }
        drop(suggestions);

        if changed {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
            if let Some(suggestion) = removed_suggestion {
                let _ = self
                    .record_behavioral_event(
                        "participant_suggestion",
                        BehavioralEventContext {
                            thread_id: Some(thread_id),
                            task_id: None,
                            goal_run_id: None,
                            approval_id: None,
                        },
                        serde_json::json!({
                            "action": "dismissed",
                            "suggestion": suggestion,
                        }),
                    )
                    .await;
            }
        }

        Ok(changed)
    }

    pub async fn fail_thread_participant_suggestion(
        &self,
        thread_id: &str,
        suggestion_id: &str,
        error: &str,
    ) -> Result<Option<ThreadParticipantSuggestion>> {
        if !self
            .ensure_thread_shell_live_or_persisted(thread_id)
            .await?
        {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_error = error.trim();
        let mut suggestions = self.thread_participant_suggestions.write().await;
        let updated = suggestions
            .get_mut(thread_id)
            .and_then(|entry| {
                entry
                    .iter_mut()
                    .find(|suggestion| suggestion.id == suggestion_id)
            })
            .map(|suggestion| {
                suggestion.status = ThreadParticipantSuggestionStatus::Failed;
                suggestion.updated_at = now_millis();
                suggestion.error = (!trimmed_error.is_empty()).then(|| trimmed_error.to_string());
                suggestion.clone()
            });
        drop(suggestions);

        if updated.is_some() {
            self.persist_thread_by_id(thread_id).await;
            if let Some(suggestion) = updated.clone() {
                let _ = self.event_tx.send(AgentEvent::ParticipantSuggestion {
                    thread_id: thread_id.to_string(),
                    suggestion: suggestion.clone(),
                });
                let _ = self
                    .record_behavioral_event(
                        "participant_suggestion",
                        BehavioralEventContext {
                            thread_id: Some(thread_id),
                            task_id: None,
                            goal_run_id: None,
                            approval_id: None,
                        },
                        serde_json::json!({
                            "action": "failed",
                            "suggestion": suggestion,
                        }),
                    )
                    .await;
            }
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        Ok(updated)
    }

    pub async fn send_thread_participant_suggestion(
        &self,
        thread_id: &str,
        suggestion_id: &str,
        _preferred_session_hint: Option<&str>,
    ) -> Result<bool> {
        if !self
            .ensure_thread_shell_live_or_persisted(thread_id)
            .await?
        {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let suggestion = self
            .list_thread_participant_suggestions(thread_id)
            .await
            .into_iter()
            .find(|entry| entry.id == suggestion_id)
            .ok_or_else(|| anyhow::anyhow!("participant suggestion not found: {suggestion_id}"))?;

        let send_result = match suggestion.suggestion_kind {
            ThreadParticipantSuggestionKind::PreparedMessage => {
                self.append_visible_thread_participant_message(
                    thread_id,
                    &suggestion.target_agent_id,
                    &suggestion.instruction,
                )
                .await
            }
            ThreadParticipantSuggestionKind::AutoResponse => {
                let message = if is_auto_response_request_prompt(&suggestion.instruction) {
                    let generated = self
                        .generate_visible_thread_participant_message(
                            thread_id,
                            &suggestion.target_agent_id,
                            &suggestion.instruction,
                        )
                        .await?;
                    crate::agent::thread_participant_runner::parse_participant_suggestion_response(
                        &generated,
                    )
                    .map(|(_, message)| message)
                    .unwrap_or(generated)
                } else {
                    suggestion.instruction.clone()
                };
                self.append_visible_thread_participant_message(
                    thread_id,
                    &suggestion.target_agent_id,
                    &message,
                )
                .await?;
                self.continue_thread_after_participant_post_or_notice(thread_id)
                    .await;
                Ok(())
            }
        };

        match send_result {
            Ok(()) => {
                let _ = self
                    .dismiss_thread_participant_suggestion(thread_id, suggestion_id)
                    .await?;
                let _ = self
                    .record_behavioral_event(
                        "participant_suggestion",
                        BehavioralEventContext {
                            thread_id: Some(thread_id),
                            task_id: None,
                            goal_run_id: None,
                            approval_id: None,
                        },
                        serde_json::json!({
                            "action": "sent",
                            "suggestion": suggestion,
                        }),
                    )
                    .await;
                if suggestion.suggestion_kind == ThreadParticipantSuggestionKind::PreparedMessage {
                    self.continue_thread_after_participant_post_or_notice(thread_id)
                        .await;
                }
                Ok(true)
            }
            Err(error) => {
                let _ = self
                    .fail_thread_participant_suggestion(
                        thread_id,
                        suggestion_id,
                        &error.to_string(),
                    )
                    .await?;
                Err(error)
            }
        }
    }

    pub async fn agent_thread_detail_json(
        &self,
        thread_id: &str,
        message_limit: Option<usize>,
        message_offset: Option<usize>,
        collapse_tool_calls: bool,
    ) -> String {
        let detail_result = self
            .get_thread_filtered_with_tool_run_pagination(
                thread_id,
                false,
                message_limit,
                message_offset.unwrap_or(0),
                collapse_tool_calls,
            )
            .await;
        let mut value = serde_json::to_value(detail_result.as_ref().map(|result| &result.thread))
            .unwrap_or(serde_json::Value::Null);

        if let Some(detail) = value.as_object_mut() {
            if let Some(result) = detail_result.as_ref() {
                detail.insert(
                    "total_message_count".to_string(),
                    serde_json::Value::from(result.total_message_count),
                );
                detail.insert(
                    "loaded_message_start".to_string(),
                    serde_json::Value::from(result.loaded_message_start),
                );
                detail.insert(
                    "loaded_message_end".to_string(),
                    serde_json::Value::from(result.loaded_message_end),
                );
                detail.insert(
                    "active_context_window_start".to_string(),
                    serde_json::Value::from(result.active_context_window_start),
                );
                detail.insert(
                    "active_context_window_end".to_string(),
                    serde_json::Value::from(result.active_context_window_end),
                );
                detail.insert(
                    "active_context_window_tokens".to_string(),
                    serde_json::Value::from(result.active_context_window_tokens),
                );
                detail.insert(
                    "pinned_messages".to_string(),
                    serde_json::to_value(&result.pinned_messages)
                        .unwrap_or(serde_json::Value::Array(Vec::new())),
                );
            }
            let (participants, suggestions, execution_profile, handoff_state) = tokio::join!(
                self.list_thread_participants(thread_id),
                self.list_thread_participant_suggestions(thread_id),
                async {
                    self.thread_execution_profiles
                        .read()
                        .await
                        .get(thread_id)
                        .cloned()
                },
                self.thread_handoff_state(thread_id),
            );
            let total_cost_usd = self
                .history
                .thread_message_cost_total(thread_id)
                .await
                .ok()
                .flatten();
            detail.insert(
                "total_cost_usd".to_string(),
                total_cost_usd
                    .and_then(serde_json::Number::from_f64)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
            );
            let nonempty = |value: Option<String>| {
                value
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            };
            let derived_from_messages = detail_result.as_ref().and_then(|result| {
                result
                    .thread
                    .messages
                    .iter()
                    .rev()
                    .take_while(|message| {
                        !(message.role == MessageRole::System
                            && message.content.starts_with(
                                crate::agent::thread_handoffs::THREAD_HANDOFF_SYSTEM_MARKER,
                            ))
                    })
                    .find_map(|message| {
                        if message.message_kind
                            == crate::agent::types::AgentMessageKind::CompactionArtifact
                        {
                            return None;
                        }
                        if !matches!(message.role, MessageRole::Assistant) {
                            return None;
                        }
                        let provider = nonempty(message.provider.clone())?;
                        let model = nonempty(message.model.clone())?;
                        Some((provider, model))
                    })
            });
            let resolved_provider = execution_profile
                .as_ref()
                .and_then(|profile| nonempty(profile.provider.clone()))
                .or_else(|| derived_from_messages.as_ref().map(|(p, _)| p.clone()));
            let resolved_model = execution_profile
                .as_ref()
                .and_then(|profile| nonempty(profile.model.clone()))
                .or_else(|| derived_from_messages.as_ref().map(|(_, m)| m.clone()));
            let existing_agent_name = detail
                .get("agent_name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            if existing_agent_name.is_none() {
                let derived_agent_name = detail_result.as_ref().and_then(|result| {
                    result.thread.messages.iter().rev().find_map(|message| {
                        if !matches!(message.role, MessageRole::Assistant) {
                            return None;
                        }
                        nonempty(message.author_agent_name.clone())
                    })
                });
                if let Some(name) = derived_agent_name {
                    detail.insert("agent_name".to_string(), serde_json::Value::String(name));
                }
            }
            detail.insert(
                "thread_handoff_state".to_string(),
                serde_json::to_value(handoff_state).unwrap_or(serde_json::Value::Null),
            );
            detail.insert(
                "thread_participants".to_string(),
                serde_json::to_value(participants).unwrap_or(serde_json::Value::Array(Vec::new())),
            );
            detail.insert(
                "queued_participant_suggestions".to_string(),
                serde_json::to_value(suggestions).unwrap_or(serde_json::Value::Array(Vec::new())),
            );
            detail.insert(
                "profile_provider".to_string(),
                resolved_provider
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            detail.insert(
                "profile_model".to_string(),
                resolved_model
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            detail.insert(
                "profile_reasoning_effort".to_string(),
                execution_profile
                    .as_ref()
                    .and_then(|profile| profile.reasoning_effort.clone())
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            detail.insert(
                "profile_context_window_tokens".to_string(),
                execution_profile
                    .as_ref()
                    .and_then(|profile| profile.context_window_tokens)
                    .map(serde_json::Value::from)
                    .unwrap_or(serde_json::Value::Null),
            );
        }

        serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string())
    }

    pub(crate) async fn continue_thread_after_participant_post_or_notice(&self, thread_id: &str) {
        let Some((
            latest_participant_author_id,
            latest_participant_author_name,
            participant_message,
        )) = self.latest_visible_participant_message(thread_id).await
        else {
            return;
        };

        if crate::agent::thread_participant_runner::participant_message_is_status_ack(
            &participant_message,
        ) {
            tracing::info!(
                thread_id = %thread_id,
                participant = %latest_participant_author_id,
                "skipping responder continuation for status-only participant acknowledgement"
            );
            return;
        }

        let continuation_agent_id = self
            .active_agent_id_for_thread(thread_id)
            .await
            .unwrap_or_else(|| MAIN_AGENT_ID.to_string());

        if continuation_agent_id.eq_ignore_ascii_case(&latest_participant_author_id) {
            tracing::info!(
                thread_id = %thread_id,
                participant = %latest_participant_author_id,
                "skipping participant follow-up continuation because the participant is already the active responder"
            );
            return;
        }

        let continuation_prompt = self
            .build_participant_follow_up_continuation_prompt(
                thread_id,
                &continuation_agent_id,
                &latest_participant_author_name,
                &participant_message,
            )
            .await;
        self.enqueue_visible_thread_continuation(
            thread_id,
            DeferredVisibleThreadContinuation {
                agent_id: continuation_agent_id.clone(),
                task_id: None,
                preferred_session_hint: None,
                llm_user_content: continuation_prompt,
                force_compaction: false,
                queued_at_ms: 0,
                rerun_participant_observers_after_turn: false,
                internal_delegate_sender: None,
                internal_delegate_message: None,
            },
        )
        .await;

        if let Err(error) = self
            .flush_deferred_visible_thread_continuations(thread_id)
            .await
        {
            tracing::warn!(
                thread_id = %thread_id,
                error = %error,
                "participant follow-up continuation failed"
            );
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: "participant_follow_up_error".to_string(),
                message: "participant follow-up failed".to_string(),
                details: Some(error.to_string()),
            });
        }
    }

    pub(crate) async fn maybe_auto_send_next_thread_participant_suggestion(
        &self,
        thread_id: &str,
    ) -> Result<bool> {
        let acquired_drain_slot = {
            let mut active = self
                .active_thread_participant_suggestion_drains
                .lock()
                .await;
            active.insert(thread_id.to_string())
        };
        if !acquired_drain_slot {
            return Ok(false);
        }

        let result = async {
            let mut sent_any = false;

            loop {
                {
                    let streams = self.stream_cancellations.lock().await;
                    if streams.contains_key(thread_id) {
                        break;
                    }
                }

                let participants = self.list_thread_participants(thread_id).await;
                let active_participant_ids = participants
                    .into_iter()
                    .filter(|participant| participant.status == ThreadParticipantStatus::Active)
                    .map(|participant| participant.agent_id)
                    .collect::<HashSet<_>>();
                let current_auto_response_source = self
                    .latest_visible_main_agent_message_timestamp_for_auto_response(thread_id)
                    .await;
                let mut stale_suggestion_ids = Vec::new();
                let next_suggestion = self
                    .list_thread_participant_suggestions(thread_id)
                    .await
                    .into_iter()
                    .find(|suggestion| {
                        if suggestion.status != ThreadParticipantSuggestionStatus::Queued {
                            return false;
                        }
                        let target_is_active = active_participant_ids.iter().any(|agent_id| {
                            agent_id.eq_ignore_ascii_case(&suggestion.target_agent_id)
                        });
                        let stale_auto_response = suggestion.suggestion_kind
                            == ThreadParticipantSuggestionKind::AutoResponse
                            && suggestion.source_message_timestamp != current_auto_response_source;
                        let looks_like_no_suggestion = crate::agent::thread_participant_runner::participant_response_is_no_suggestion(
                            &suggestion.instruction,
                        ) || crate::agent::thread_participant_runner::parse_participant_suggestion_response(
                            &suggestion.instruction,
                        )
                        .is_none()
                            || crate::agent::thread_participant_runner::participant_message_is_status_ack(
                                &suggestion.instruction,
                            );
                        if !target_is_active
                            || stale_auto_response
                            || (suggestion.suggestion_kind
                                == ThreadParticipantSuggestionKind::PreparedMessage
                                && looks_like_no_suggestion)
                        {
                            stale_suggestion_ids.push(suggestion.id.clone());
                            return false;
                        }
                        if suggestion.suggestion_kind == ThreadParticipantSuggestionKind::AutoResponse
                        {
                            return false;
                        }
                        true
                    });
                for stale_suggestion_id in stale_suggestion_ids {
                    let _ = self
                        .dismiss_thread_participant_suggestion(thread_id, &stale_suggestion_id)
                        .await?;
                }

                let Some(next_suggestion) = next_suggestion else {
                    break;
                };

                tracing::info!(
                    thread_id = %thread_id,
                    participant = %next_suggestion.target_agent_id,
                    suggestion_id = %next_suggestion.id,
                    "auto-sending queued participant suggestion after thread became idle"
                );
                sent_any |= self
                    .send_thread_participant_suggestion(thread_id, &next_suggestion.id, None)
                    .await?;
            }

            Ok(sent_any)
        }
        .await;

        self.active_thread_participant_suggestion_drains
            .lock()
            .await
            .remove(thread_id);
        result
    }

    pub async fn upsert_thread_participant(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        instruction: &str,
    ) -> Result<ThreadParticipantState> {
        if !self
            .ensure_thread_shell_live_or_persisted(thread_id)
            .await?
        {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_instruction = instruction.trim();
        if trimmed_instruction.is_empty() {
            anyhow::bail!("participant instruction cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let now = now_millis();
        let mut participants = self.thread_participants.write().await;
        let entry = participants.entry(thread_id.to_string()).or_default();
        let updated = if let Some(existing) = entry
            .iter_mut()
            .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
        {
            existing.agent_id = agent_id.clone();
            existing.agent_name = agent_name.clone();
            existing.instruction = trimmed_instruction.to_string();
            existing.status = ThreadParticipantStatus::Active;
            existing.updated_at = now;
            existing.deactivated_at = None;
            existing.clone()
        } else {
            let state = ThreadParticipantState {
                agent_id: agent_id.clone(),
                agent_name: agent_name.clone(),
                instruction: trimmed_instruction.to_string(),
                status: ThreadParticipantStatus::Active,
                created_at: now,
                updated_at: now,
                deactivated_at: None,
                last_contribution_at: None,
                last_observed_visible_message_at: None,
                always_auto_response: false,
                last_contributed_epoch: None,
                duplicate_suppression_streak: 0,
                last_suppressed_at: None,
            };
            entry.push(state.clone());
            state
        };
        *entry = normalize_thread_participants(entry.clone());
        drop(participants);

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });

        Ok(updated)
    }

    pub async fn deactivate_thread_participant(
        &self,
        thread_id: &str,
        target_agent_id: &str,
    ) -> Result<Option<ThreadParticipantState>> {
        if !self
            .ensure_thread_shell_live_or_persisted(thread_id)
            .await?
        {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let (agent_id, _) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let now = now_millis();
        let updated = {
            let mut participants = self.thread_participants.write().await;
            match participants.get_mut(thread_id) {
                Some(entry) => entry
                    .iter_mut()
                    .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
                    .map(|participant| {
                        participant.status = ThreadParticipantStatus::Inactive;
                        participant.updated_at = now;
                        participant.deactivated_at = Some(now);
                        participant.clone()
                    }),
                None => None,
            }
        };

        let cleared_suggestions = self
            .clear_thread_participant_suggestions_for_agent(thread_id, &agent_id)
            .await;

        if updated.is_none() && !cleared_suggestions {
            tracing::warn!(
                thread_id = %thread_id,
                target_agent_id = %target_agent_id,
                resolved_agent_id = %agent_id,
                "deactivate_thread_participant: no matching active participant or queued suggestions; deactivate is a no-op"
            );
        }

        if updated.is_some() || cleared_suggestions {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        Ok(updated)
    }

    pub async fn remove_thread_participant(
        &self,
        thread_id: &str,
        target_agent_id: &str,
    ) -> Result<Option<ThreadParticipantState>> {
        if !self
            .ensure_thread_shell_live_or_persisted(thread_id)
            .await?
        {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let (agent_id, _) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let removed = {
            let mut participants = self.thread_participants.write().await;
            let (removed, remove_entry) = match participants.get_mut(thread_id) {
                Some(entry) => {
                    let removed = entry
                        .iter()
                        .position(|participant| {
                            participant.agent_id.eq_ignore_ascii_case(&agent_id)
                        })
                        .map(|index| entry.remove(index));
                    (removed, entry.is_empty())
                }
                None => (None, false),
            };
            if remove_entry {
                participants.remove(thread_id);
            }
            removed
        };

        let cleared_suggestions = self
            .clear_thread_participant_suggestions_for_agent(thread_id, &agent_id)
            .await;

        if removed.is_none() && !cleared_suggestions {
            tracing::warn!(
                thread_id = %thread_id,
                target_agent_id = %target_agent_id,
                resolved_agent_id = %agent_id,
                "remove_thread_participant: no matching participant or queued suggestions; remove is a no-op"
            );
        }

        if removed.is_some() || cleared_suggestions {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        Ok(removed)
    }

    pub async fn apply_thread_participant_command(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        action: &str,
        instruction: Option<&str>,
    ) -> Result<()> {
        if action.trim().eq_ignore_ascii_case("auto_response_always") {
            let _ = self
                .set_thread_participant_always_auto_response(thread_id, target_agent_id, true)
                .await?;
            let _ = self
                .request_thread_auto_response_suggestion(thread_id, target_agent_id)
                .await?;
            return Ok(());
        }
        if action.trim().eq_ignore_ascii_case("auto_response_disable") {
            let _ = self
                .set_thread_participant_always_auto_response(thread_id, target_agent_id, false)
                .await?;
            return Ok(());
        }
        if action.trim().eq_ignore_ascii_case("auto_response") {
            let _ = self
                .request_thread_auto_response_suggestion(thread_id, target_agent_id)
                .await?;
            return Ok(());
        }
        if is_participant_stop_action(action) {
            let _ = self
                .deactivate_thread_participant(thread_id, target_agent_id)
                .await?;
            return Ok(());
        }
        if is_participant_remove_action(action) {
            let _ = self
                .remove_thread_participant(thread_id, target_agent_id)
                .await?;
            return Ok(());
        }

        self.upsert_thread_participant(
            thread_id,
            target_agent_id,
            instruction.unwrap_or("").trim(),
        )
        .await?;
        if let Err(error) = self.run_participant_observers(thread_id).await {
            tracing::warn!(
                thread_id = %thread_id,
                participant = %target_agent_id,
                error = %error,
                "participant initial observer failed"
            );
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: "participant_observer_error".to_string(),
                message: "participant observers failed".to_string(),
                details: Some(error.to_string()),
            });
        }
        Ok(())
    }

    pub async fn send_internal_delegate_message(
        &self,
        thread_id: Option<&str>,
        target_agent_id: &str,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<()> {
        if let Some(thread_id) = thread_id {
            if is_internal_dm_thread(thread_id)
                || is_participant_playground_thread(thread_id)
                || is_internal_handoff_thread(thread_id)
            {
                anyhow::bail!(
                    "internal delegate continuation requires a visible operator thread, not an internal thread"
                );
            }
            let budget_exceeded_task = self.budget_exceeded_task_for_thread(thread_id).await;
            if let Some(task_id) = budget_exceeded_task {
                anyhow::bail!(
                    "thread {thread_id} is locked because task {task_id} exhausted its execution budget"
                );
            }
        }
        let (resolved_target_id, _) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let sender = match thread_id {
            Some(thread_id) => self
                .thread_handoff_state(thread_id)
                .await
                .map(|state| state.active_agent_id)
                .unwrap_or_else(|| MAIN_AGENT_ID.to_string()),
            None => MAIN_AGENT_ID.to_string(),
        };
        let payload = self
            .build_internal_delegate_payload(thread_id, content, false)
            .await;
        Box::pin(self.send_internal_agent_message(
            &sender,
            &resolved_target_id,
            &payload,
            preferred_session_hint,
        ))
        .await?;

        Ok(())
    }

    pub async fn append_visible_thread_participant_message(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        content: &str,
    ) -> Result<()> {
        if !self.ensure_thread_messages_loaded(thread_id).await {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_content = content.trim();
        if trimmed_content.is_empty() {
            anyhow::bail!("participant message content cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;

        let participant_exists =
            self.list_thread_participants(thread_id)
                .await
                .iter()
                .any(|participant| {
                    participant.agent_id.eq_ignore_ascii_case(&agent_id)
                        && participant.status == ThreadParticipantStatus::Active
                });
        if !participant_exists {
            anyhow::bail!("participant is not active on thread: {agent_id}");
        }

        let now = now_millis();
        {
            let mut threads = self.threads.write().await;
            let thread = threads
                .get_mut(thread_id)
                .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: trimmed_content.to_string(),
                content_blocks: Vec::new(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                cost: None,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: Some(agent_id.clone()),
                author_agent_name: Some(agent_name.clone()),
                reasoning: None,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: now,
                feedback: None,
            });
            thread.updated_at = now;
        }

        {
            let mut participants = self.thread_participants.write().await;
            if let Some(entry) = participants.get_mut(thread_id) {
                if let Some(participant) = entry
                    .iter_mut()
                    .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
                {
                    participant.last_contribution_at = Some(now);
                    participant.last_observed_visible_message_at = Some(now);
                    participant.updated_at = now;
                    participant.status = ThreadParticipantStatus::Active;
                }
            }
        }

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });

        Ok(())
    }

    pub async fn send_visible_thread_participant_message(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        _preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<()> {
        if !self.ensure_thread_messages_loaded(thread_id).await {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_content = content.trim();
        if trimmed_content.is_empty() {
            anyhow::bail!("participant message content cannot be empty");
        }

        let (agent_id, _agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;

        let participant_exists =
            self.list_thread_participants(thread_id)
                .await
                .iter()
                .any(|participant| {
                    participant.agent_id.eq_ignore_ascii_case(&agent_id)
                        && participant.status == ThreadParticipantStatus::Active
                });
        if !participant_exists {
            anyhow::bail!("participant is not active on thread: {agent_id}");
        }

        let generated_message = self
            .generate_visible_thread_participant_message(thread_id, &agent_id, trimmed_content)
            .await?;
        let should_continue_main_agent =
            !crate::agent::thread_participant_runner::participant_response_is_no_suggestion(
                &generated_message,
            ) && crate::agent::thread_participant_runner::parse_participant_suggestion_response(
                &generated_message,
            )
            .is_some();
        self.append_visible_thread_participant_message(thread_id, &agent_id, &generated_message)
            .await?;
        if should_continue_main_agent {
            self.continue_thread_after_participant_post_or_notice(thread_id)
                .await;
        }

        Ok(())
    }
}

struct FlushSlotGuard<'a> {
    slots: &'a std::sync::Mutex<HashSet<String>>,
    thread_id: String,
    released: bool,
}

fn lock_flush_slots(
    slots: &std::sync::Mutex<HashSet<String>>,
) -> std::sync::MutexGuard<'_, HashSet<String>> {
    slots
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl<'a> FlushSlotGuard<'a> {
    fn new(slots: &'a std::sync::Mutex<HashSet<String>>, thread_id: String) -> Self {
        Self {
            slots,
            thread_id,
            released: false,
        }
    }

    async fn release(&mut self) {
        if self.released {
            return;
        }
        lock_flush_slots(self.slots).remove(&self.thread_id);
        self.released = true;
    }
}

impl Drop for FlushSlotGuard<'_> {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        lock_flush_slots(self.slots).remove(&self.thread_id);
        self.released = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn participant(agent_id: &str, agent_name: &str) -> ThreadParticipantState {
        ThreadParticipantState {
            agent_id: agent_id.to_string(),
            agent_name: agent_name.to_string(),
            instruction: "do the thing".to_string(),
            status: ThreadParticipantStatus::Active,
            created_at: 1,
            updated_at: 1,
            deactivated_at: None,
            last_contribution_at: None,
            last_observed_visible_message_at: None,
            always_auto_response: false,
            last_contributed_epoch: None,
            duplicate_suppression_streak: 0,
            last_suppressed_at: None,
        }
    }

    #[test]
    fn normalize_keeps_user_subagent_id_so_responses_are_not_attributed_to_main_agent() {
        let normalized =
            normalize_thread_participants(vec![participant("subagent-1781954801939", "hf")]);
        assert_eq!(normalized[0].agent_id, "subagent-1781954801939");
        assert_eq!(normalized[0].agent_name, "hf");
    }

    #[test]
    fn normalize_canonicalizes_known_builtin_aliases() {
        let normalized = normalize_thread_participants(vec![participant("veles", "")]);
        assert_eq!(normalized[0].agent_id, WELES_AGENT_ID);
        assert_eq!(normalized[0].agent_name, WELES_AGENT_NAME);
    }

    #[test]
    fn remove_action_accepts_remove_keyword() {
        assert!(is_participant_remove_action("remove"));
        assert!(is_participant_remove_action("leave"));
        assert!(!is_participant_remove_action("deactivate"));
    }

    #[tokio::test]
    async fn task_provider_override_for_compaction_uses_persisted_task_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let mut task = engine
            .enqueue_task(
                "persisted override task".to_string(),
                "force compaction should use this persisted provider override".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some("thread-persisted-override".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        task.override_provider = Some("openai".to_string());
        task.override_model = Some("gpt-5.4-mini".to_string());
        {
            let mut tasks = engine.tasks.lock().await;
            tasks.clear();
            tasks.push_back(task.clone());
        }
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        assert_eq!(
            engine.task_provider_override_for_compaction(&task.id).await,
            Some(("openai".to_string(), Some("gpt-5.4-mini".to_string())))
        );
    }

    #[tokio::test]
    async fn flush_slot_guard_does_not_steal_slot_acquired_after_release() {
        let slots = std::sync::Mutex::new(HashSet::new());
        lock_flush_slots(&slots).insert("thread-a".to_string());
        {
            let mut guard = FlushSlotGuard::new(&slots, "thread-a".to_string());
            guard.release().await;
            assert!(
                lock_flush_slots(&slots).insert("thread-a".to_string()),
                "after release another flush must be able to acquire the slot"
            );
        }
        assert!(
            lock_flush_slots(&slots).contains("thread-a"),
            "drop after release must not steal the next flush's slot"
        );
    }

    #[tokio::test]
    async fn flush_slot_guard_drop_releases_slot_without_explicit_release() {
        let slots = std::sync::Mutex::new(HashSet::new());
        lock_flush_slots(&slots).insert("thread-a".to_string());
        drop(FlushSlotGuard::new(&slots, "thread-a".to_string()));
        assert!(
            lock_flush_slots(&slots).insert("thread-a".to_string()),
            "cancelling a flush before release() must still free the slot; try_lock on a tokio mutex leaves it stuck when the lock is busy"
        );
    }

    #[tokio::test]
    async fn open_circuit_keeps_deferred_continuation_queued_instead_of_starting_a_stream() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let provider = engine.config.read().await.provider.clone();
        for _ in 0..5 {
            engine.record_llm_outcome(&provider, false).await;
        }
        assert!(
            !engine.provider_circuit_is_closed(&provider).await,
            "repeated provider failures must open the breaker before another stream starts"
        );

        let thread_id = "thread-open-circuit";
        engine
            .enqueue_visible_thread_continuation(
                thread_id,
                DeferredVisibleThreadContinuation {
                    agent_id: MAIN_AGENT_ID.to_string(),
                    task_id: None,
                    preferred_session_hint: None,
                    llm_user_content: "continue after the child finished".to_string(),
                    queued_at_ms: 1,
                    force_compaction: false,
                    rerun_participant_observers_after_turn: false,
                    internal_delegate_sender: None,
                    internal_delegate_message: None,
                },
            )
            .await;

        engine
            .flush_deferred_visible_thread_continuations(thread_id)
            .await
            .expect("an open breaker should skip the flush without failing the turn");
        assert_eq!(
            engine
                .deferred_visible_thread_continuations_for(thread_id)
                .await
                .len(),
            1,
            "the continuation must stay queued until the provider circuit closes"
        );
    }

    #[tokio::test]
    async fn tripping_the_circuit_breaker_cancels_the_in_flight_provider_stream() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let provider = engine.config.read().await.provider.clone();
        let (generation, token, _) = engine.begin_stream_cancellation("thread-live").await;
        engine
            .tag_stream_provider("thread-live", generation, &provider)
            .await;

        for _ in 0..5 {
            engine.record_llm_outcome(&provider, false).await;
        }

        assert!(
            token.is_cancelled(),
            "opening the breaker must cancel the live stream instead of letting it keep reading"
        );
    }

    #[tokio::test]
    async fn stream_start_and_provider_abort_do_not_deadlock() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = std::sync::Arc::new(
            AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await,
        );
        let provider = engine.config.read().await.provider.clone();
        let mut tasks = Vec::new();
        for index in 0..8 {
            let engine = std::sync::Arc::clone(&engine);
            let provider = provider.clone();
            tasks.push(tokio::spawn(async move {
                let thread_id = format!("thread-{index}");
                let (generation, _, _) = engine.begin_stream_cancellation(&thread_id).await;
                engine
                    .tag_stream_provider(&thread_id, generation, &provider)
                    .await;
                engine.abort_streams_for_provider(&provider).await;
                engine
                    .finish_stream_cancellation(&thread_id, generation)
                    .await;
            }));
        }
        let finished = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            for task in tasks {
                task.await.expect("stream lock task should finish");
            }
        })
        .await;
        assert!(
            finished.is_ok(),
            "starting a stream while the provider circuit aborts streams must not deadlock"
        );
    }
}
