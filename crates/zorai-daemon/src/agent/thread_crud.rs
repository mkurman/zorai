#![allow(dead_code)]

//! Thread CRUD operations — list, get, delete, planner detection.

use super::*;
use serde::{Deserialize, Serialize};

const SESSION_ABANDON_WINDOW_MS: u64 = 30_000;
const LAZY_CAPPED_IPC_MESSAGE_WINDOW: usize = 64;

fn reaction_str(reaction: zorai_protocol::Reaction) -> &'static str {
    match reaction {
        zorai_protocol::Reaction::Up => "up",
        zorai_protocol::Reaction::Down => "down",
    }
}

fn parse_reaction(value: Option<&str>) -> Option<zorai_protocol::Reaction> {
    match value? {
        "up" => Some(zorai_protocol::Reaction::Up),
        "down" => Some(zorai_protocol::Reaction::Down),
        _ => None,
    }
}

enum ThreadMetadataPatch {
    ClientSurface(Option<zorai_protocol::ClientSurface>),
    ExecutionProfile(Option<ThreadExecutionProfile>),
    LatestSkillDiscoveryState(Option<LatestSkillDiscoveryState>),
    PromptMemoryInjectionState(Option<PromptMemoryInjectionState>),
    WorkspaceContext(Option<ThreadWorkspaceContext>),
}

impl ThreadMetadataPatch {
    fn apply(self, metadata: &mut serde_json::Map<String, serde_json::Value>) {
        match self {
            ThreadMetadataPatch::ClientSurface(Some(client_surface)) => {
                if let Ok(value) = serde_json::to_value(client_surface) {
                    metadata.insert("client_surface".to_string(), value.clone());
                    metadata.insert("clientSurface".to_string(), value);
                }
            }
            ThreadMetadataPatch::ClientSurface(None) => {
                metadata.remove("client_surface");
                metadata.remove("clientSurface");
            }
            ThreadMetadataPatch::ExecutionProfile(profile) => {
                match profile.and_then(|profile| serde_json::to_value(profile).ok()) {
                    Some(value) => {
                        metadata.insert("execution_profile".to_string(), value.clone());
                        metadata.insert("thread_profile".to_string(), value);
                    }
                    None => {
                        metadata.remove("execution_profile");
                        metadata.remove("thread_profile");
                    }
                }
            }
            ThreadMetadataPatch::LatestSkillDiscoveryState(Some(state)) => {
                if let Ok(value) = serde_json::to_value(state) {
                    metadata.insert("latest_skill_discovery_state".to_string(), value);
                    metadata.remove("latestSkillDiscoveryState");
                }
            }
            ThreadMetadataPatch::LatestSkillDiscoveryState(None) => {
                metadata.remove("latest_skill_discovery_state");
                metadata.remove("latestSkillDiscoveryState");
            }
            ThreadMetadataPatch::PromptMemoryInjectionState(Some(state)) => {
                if let Ok(value) = serde_json::to_value(state) {
                    metadata.insert("prompt_memory_injection_state".to_string(), value);
                    metadata.remove("promptMemoryInjectionState");
                }
            }
            ThreadMetadataPatch::PromptMemoryInjectionState(None) => {
                metadata.remove("prompt_memory_injection_state");
                metadata.remove("promptMemoryInjectionState");
            }
            ThreadMetadataPatch::WorkspaceContext(Some(context)) => {
                if let Ok(value) = serde_json::to_value(context) {
                    metadata.insert("workspace_context".to_string(), value);
                    metadata.remove("workspaceContext");
                }
            }
            ThreadMetadataPatch::WorkspaceContext(None) => {
                metadata.remove("workspace_context");
                metadata.remove("workspaceContext");
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ThreadListFilter {
    pub created_after: Option<u64>,
    pub created_before: Option<u64>,
    pub updated_after: Option<u64>,
    pub updated_before: Option<u64>,
    pub agent_name: Option<String>,
    pub title_query: Option<String>,
    pub pinned: Option<bool>,
    pub include_internal: bool,
    pub limit: Option<usize>,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ThreadDetailResult {
    pub thread: AgentThread,
    pub messages_truncated: bool,
    pub total_message_count: usize,
    pub loaded_message_start: usize,
    pub loaded_message_end: usize,
    pub active_context_window_start: usize,
    pub active_context_window_end: usize,
    pub active_context_window_tokens: u64,
    #[serde(default)]
    pub pinned_messages: Vec<PinnedThreadMessageSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PinnedThreadMessageSummary {
    pub message_id: String,
    pub absolute_index: usize,
    pub role: MessageRole,
    pub content: String,
}

fn pinned_message_summaries(thread: &AgentThread) -> Vec<PinnedThreadMessageSummary> {
    thread
        .messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.pinned_for_compaction)
        .map(|(absolute_index, message)| PinnedThreadMessageSummary {
            message_id: message.id.clone(),
            absolute_index,
            role: message.role,
            content: message.content.clone(),
        })
        .collect()
}

fn thread_context_window_summary(messages: &[AgentMessage]) -> (usize, usize, u64) {
    let (start, active_messages) = active_compaction_window(messages);
    (
        start,
        messages.len(),
        estimate_message_tokens(active_messages) as u64,
    )
}

async fn persisted_pinned_message_summaries(
    engine: &AgentEngine,
    thread_id: &str,
) -> Vec<PinnedThreadMessageSummary> {
    let Ok(rows) = engine
        .history
        .list_pinned_messages_for_compaction(thread_id)
        .await
    else {
        return Vec::new();
    };

    rows.into_iter()
        .filter_map(|(absolute_index, row)| {
            super::messaging::agent_message_from_db(row).map(|message| PinnedThreadMessageSummary {
                message_id: message.id,
                absolute_index,
                role: message.role,
                content: message.content,
            })
        })
        .collect()
}

fn thread_detail_frame_fits_ipc(thread: &Option<AgentThread>) -> bool {
    let Ok(thread_json) = serde_json::to_string(thread) else {
        return false;
    };

    zorai_protocol::daemon_message_fits_ipc(&zorai_protocol::DaemonMessage::AgentThreadDetail {
        thread_json,
    })
}

fn cap_thread_detail_for_ipc(detail: ThreadDetailResult) -> ThreadDetailResult {
    if thread_detail_frame_fits_ipc(&Some(detail.thread.clone())) {
        return detail;
    }

    let mut low = 0usize;
    let mut high = detail.thread.messages.len();
    while low < high {
        let mid = low + (high - low) / 2;
        let mut candidate = detail.thread.clone();
        candidate.messages = candidate.messages[mid..].to_vec();
        if thread_detail_frame_fits_ipc(&Some(candidate)) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    let mut thread = detail.thread;
    thread.messages = thread.messages[low..].to_vec();

    ThreadDetailResult {
        thread,
        messages_truncated: detail.messages_truncated || low > 0,
        total_message_count: detail.total_message_count,
        loaded_message_start: detail.loaded_message_start + low,
        loaded_message_end: detail.loaded_message_end,
        active_context_window_start: detail.active_context_window_start,
        active_context_window_end: detail.active_context_window_end,
        active_context_window_tokens: detail.active_context_window_tokens,
        pinned_messages: detail.pinned_messages,
    }
}

impl AgentEngine {
    pub(crate) async fn persist_stored_thread_execution_profile(&self, thread_id: &str) {
        let profile = self.get_thread_execution_profile(thread_id).await;
        let _ = self
            .persist_thread_metadata_patch(thread_id, ThreadMetadataPatch::ExecutionProfile(profile))
            .await;
    }

    async fn persisted_thread_metadata(&self, thread_id: &str) -> Option<ParsedThreadMetadata> {
        let metadata_json = match self.history.thread_metadata_json(thread_id).await {
            Ok(Some(metadata_json)) => Some(metadata_json),
            Ok(None) => match self.history.has_thread_id(thread_id).await {
                Ok(true) => None,
                Ok(false) => return None,
                Err(error) => {
                    tracing::warn!(
                        thread_id = %thread_id,
                        %error,
                        "failed to check persisted thread existence for metadata lookup"
                    );
                    return None;
                }
            },
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to read persisted thread metadata"
                );
                return None;
            }
        };

        Some(parse_thread_metadata(metadata_json.as_deref()))
    }

    async fn persist_thread_metadata_patch(
        &self,
        thread_id: &str,
        patch: ThreadMetadataPatch,
    ) -> bool {
        let metadata_json = match self.history.thread_metadata_json(thread_id).await {
            Ok(Some(metadata_json)) => Some(metadata_json),
            Ok(None) => match self.history.has_thread_id(thread_id).await {
                Ok(true) => None,
                Ok(false) => {
                    if !self.threads.read().await.contains_key(thread_id) {
                        return false;
                    }
                    self.persist_thread_by_id(thread_id).await;
                    match self.history.thread_metadata_json(thread_id).await {
                        Ok(metadata_json) => metadata_json,
                        Err(error) => {
                            tracing::warn!(
                                thread_id = %thread_id,
                                %error,
                                "failed to read thread metadata after initial persistence"
                            );
                            return false;
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        thread_id = %thread_id,
                        %error,
                        "failed to check persisted thread existence"
                    );
                    return false;
                }
            },
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to read persisted thread metadata"
                );
                return false;
            }
        };

        let mut metadata = metadata_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|value| match value {
                serde_json::Value::Object(metadata) => Some(metadata),
                _ => None,
            })
            .unwrap_or_default();
        patch.apply(&mut metadata);

        let metadata_json = match serde_json::to_string(&serde_json::Value::Object(metadata)) {
            Ok(metadata_json) => Some(metadata_json),
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to serialize persisted thread metadata"
                );
                return false;
            }
        };

        match self
            .history
            .update_thread_metadata_json(thread_id, metadata_json)
            .await
        {
            Ok(true) => true,
            Ok(false) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    "thread disappeared before persisted metadata update"
                );
                false
            }
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to update persisted thread metadata"
                );
                false
            }
        }
    }

    pub async fn get_thread_workspace_context(
        &self,
        thread_id: &str,
    ) -> Option<ThreadWorkspaceContext> {
        self.persisted_thread_metadata(thread_id)
            .await
            .and_then(|metadata| metadata.workspace_context)
    }

    pub async fn set_thread_workspace_context(
        &self,
        thread_id: &str,
        context: Option<ThreadWorkspaceContext>,
    ) -> bool {
        let exists = self.threads.read().await.contains_key(thread_id)
            || self.history.has_thread_id(thread_id).await.unwrap_or(false);
        if !exists {
            return false;
        }
        self.persist_thread_metadata_patch(
            thread_id,
            ThreadMetadataPatch::WorkspaceContext(context),
        )
        .await
    }

    pub(super) async fn append_system_thread_message(
        &self,
        thread_id: &str,
        content: impl Into<String>,
    ) -> bool {
        if !self.ensure_thread_messages_loaded(thread_id).await {
            return false;
        }

        let content = content.into();
        let appended = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return false;
            };
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::System,
                content,
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
                author_agent_id: None,
                author_agent_name: None,
                reasoning: None,
                message_kind: crate::agent::types::AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: now_millis(),
                feedback: None,
            });
            thread.updated_at = now_millis();
            true
        };

        if appended {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        appended
    }

    pub(crate) async fn pin_thread_message_for_compaction(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> ThreadMessagePinMutationResult {
        let config = self.config.read().await.clone();
        let provider_config = match resolve_active_provider_config(&config) {
            Ok(provider_config) => provider_config,
            Err(error) => {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    format!("provider_config_unavailable:{error}"),
                    0,
                    0,
                    None,
                );
            }
        };
        let budget_chars = pinned_for_compaction_budget_chars(&config, &provider_config);

        if !self
            .ensure_thread_persisted_for_message_pin(thread_id)
            .await
        {
            return ThreadMessagePinMutationResult::failure(
                thread_id,
                message_id,
                "thread_not_found",
                0,
                budget_chars,
                None,
            );
        }

        let pin_state = match self
            .history
            .thread_message_pin_state(thread_id, message_id)
            .await
        {
            Ok(Some(pin_state)) => pin_state,
            Ok(None) => {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    "message_not_found",
                    0,
                    budget_chars,
                    None,
                );
            }
            Err(error) => {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    format!("message_pin_state_unavailable:{error}"),
                    0,
                    budget_chars,
                    None,
                );
            }
        };

        if pin_state.pinned_for_compaction {
            return ThreadMessagePinMutationResult::success(
                thread_id,
                message_id,
                pin_state.current_pinned_chars,
                budget_chars,
            );
        }

        let candidate_chars = pin_state
            .current_pinned_chars
            .saturating_add(pin_state.message_chars);
        if candidate_chars > budget_chars {
            return ThreadMessagePinMutationResult::failure(
                thread_id,
                message_id,
                "pinned_budget_exceeded",
                pin_state.current_pinned_chars,
                budget_chars,
                Some(candidate_chars),
            );
        }

        match self
            .history
            .set_message_pinned_for_compaction(thread_id, message_id, true)
            .await
        {
            Ok(true) => {
                self.set_live_thread_message_pin_if_loaded(thread_id, message_id, true)
                    .await;
                let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                    thread_id: thread_id.to_string(),
                });
                ThreadMessagePinMutationResult::success(
                    thread_id,
                    message_id,
                    candidate_chars,
                    budget_chars,
                )
            }
            Ok(false) => ThreadMessagePinMutationResult::failure(
                thread_id,
                message_id,
                "message_not_found",
                pin_state.current_pinned_chars,
                budget_chars,
                None,
            ),
            Err(error) => ThreadMessagePinMutationResult::failure(
                thread_id,
                message_id,
                format!("message_pin_update_failed:{error}"),
                pin_state.current_pinned_chars,
                budget_chars,
                None,
            ),
        }
    }

    pub(crate) async fn unpin_thread_message_for_compaction(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> ThreadMessagePinMutationResult {
        let config = self.config.read().await.clone();
        let provider_config = match resolve_active_provider_config(&config) {
            Ok(provider_config) => provider_config,
            Err(error) => {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    format!("provider_config_unavailable:{error}"),
                    0,
                    0,
                    None,
                );
            }
        };
        let budget_chars = pinned_for_compaction_budget_chars(&config, &provider_config);

        if !self
            .ensure_thread_persisted_for_message_pin(thread_id)
            .await
        {
            return ThreadMessagePinMutationResult::failure(
                thread_id,
                message_id,
                "thread_not_found",
                0,
                budget_chars,
                None,
            );
        }

        let pin_state = match self
            .history
            .thread_message_pin_state(thread_id, message_id)
            .await
        {
            Ok(Some(pin_state)) => pin_state,
            Ok(None) => {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    "message_not_found",
                    0,
                    budget_chars,
                    None,
                );
            }
            Err(error) => {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    format!("message_pin_state_unavailable:{error}"),
                    0,
                    budget_chars,
                    None,
                );
            }
        };

        let current_after_unpin =
            if pin_state.pinned_for_compaction && pin_state.counts_toward_pinned_chars {
                pin_state
                    .current_pinned_chars
                    .saturating_sub(pin_state.message_chars)
            } else {
                pin_state.current_pinned_chars
            };

        match self
            .history
            .set_message_pinned_for_compaction(thread_id, message_id, false)
            .await
        {
            Ok(true) => {
                self.set_live_thread_message_pin_if_loaded(thread_id, message_id, false)
                    .await;
                let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                    thread_id: thread_id.to_string(),
                });
                ThreadMessagePinMutationResult::success(
                    thread_id,
                    message_id,
                    current_after_unpin,
                    budget_chars,
                )
            }
            Ok(false) => ThreadMessagePinMutationResult::failure(
                thread_id,
                message_id,
                "message_not_found",
                pin_state.current_pinned_chars,
                budget_chars,
                None,
            ),
            Err(error) => ThreadMessagePinMutationResult::failure(
                thread_id,
                message_id,
                format!("message_pin_update_failed:{error}"),
                pin_state.current_pinned_chars,
                budget_chars,
                None,
            ),
        }
    }

    /// Apply (or clear) an operator's thumbs-up/down reaction on a single
    /// assistant or tool message. Returns the resolved state after toggle
    /// semantics, or `Err` with a short reason code if the operation cannot
    /// be applied.
    ///
    /// Toggle rules: setting the same reaction clears it; setting the
    /// opposite switches it; `None` always clears.
    pub(crate) async fn apply_message_feedback(
        &self,
        thread_id: &str,
        message_id: &str,
        absolute_message_index: Option<usize>,
        desired: Option<zorai_protocol::Reaction>,
    ) -> std::result::Result<Option<zorai_protocol::Reaction>, &'static str> {
        let (message_id, state) = match self
            .history
            .message_feedback_state(thread_id, message_id)
            .await
        {
            Ok(Some(state)) => (message_id.to_string(), state),
            Ok(None) => {
                let Some(absolute_index) = absolute_message_index else {
                    return Err("message_not_found");
                };
                let resolved_message_id = match self
                    .history
                    .message_id_at_absolute_index(thread_id, absolute_index)
                    .await
                {
                    Ok(Some(message_id)) => message_id,
                    Ok(None) => return Err("message_not_found"),
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            thread_id,
                            absolute_index,
                            "message_id_at_absolute_index failed"
                        );
                        return Err("message_feedback_state_unavailable");
                    }
                };
                match self
                    .history
                    .message_feedback_state(thread_id, &resolved_message_id)
                    .await
                {
                    Ok(Some(state)) => (resolved_message_id, state),
                    Ok(None) => return Err("message_not_found"),
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            thread_id,
                            message_id = %resolved_message_id,
                            "message_feedback_state failed after index resolution"
                        );
                        return Err("message_feedback_state_unavailable");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, thread_id, message_id, "message_feedback_state failed");
                return Err("message_feedback_state_unavailable");
            }
        };

        if !matches!(state.role.as_str(), "assistant" | "tool") {
            return Err("role_not_feedbackable");
        }

        let prior = parse_reaction(state.reaction.as_deref());
        let resolved = match (prior, desired) {
            (Some(p), Some(d)) if p == d => None,
            (_, Some(d)) => Some(d),
            (_, None) => None,
        };

        let resolved_str = resolved.map(reaction_str);
        match self
            .history
            .set_message_feedback(thread_id, &message_id, resolved_str)
            .await
        {
            Ok(true) => {}
            Ok(false) => return Err("message_not_found"),
            Err(error) => {
                tracing::warn!(error = %error, thread_id, message_id, "set_message_feedback failed");
                return Err("set_message_feedback_failed");
            }
        }

        let signal = match resolved {
            Some(zorai_protocol::Reaction::Up) => 1.0_f64,
            Some(zorai_protocol::Reaction::Down) => -1.0_f64,
            None => 0.0_f64,
        };
        let prior_str = prior.map(reaction_str);
        let payload = serde_json::json!({
            "message_id": message_id,
            "signal": signal,
            "prior": prior_str,
            "resolved": resolved_str,
        });
        if let Err(error) = self
            .record_behavioral_event(
                "user_feedback",
                crate::agent::behavioral_events::BehavioralEventContext {
                    thread_id: Some(thread_id),
                    task_id: None,
                    goal_run_id: None,
                    approval_id: None,
                },
                payload,
            )
            .await
        {
            tracing::warn!(error = %error, thread_id, message_id, "record_behavioral_event(user_feedback) failed");
        }

        let _ = self.event_tx.send(AgentEvent::MessageFeedbackUpdated {
            thread_id: thread_id.to_string(),
            message_id,
            reaction: resolved,
        });

        Ok(resolved)
    }

    async fn ensure_thread_persisted_for_message_pin(&self, thread_id: &str) -> bool {
        match self.history.has_thread_id(thread_id).await {
            Ok(true) => return true,
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to check persisted thread before message pin mutation"
                );
            }
        }

        if self.threads.read().await.contains_key(thread_id) {
            self.persist_thread_by_id(thread_id).await;
            return self
                .history
                .has_thread_id(thread_id)
                .await
                .ok()
                .unwrap_or(false);
        }

        false
    }

    async fn set_live_thread_message_pin_if_loaded(
        &self,
        thread_id: &str,
        message_id: &str,
        pinned: bool,
    ) {
        let mut threads = self.threads.write().await;
        let Some(thread) = threads.get_mut(thread_id) else {
            return;
        };
        let Some(message) = thread
            .messages
            .iter_mut()
            .find(|message| message.id == message_id)
        else {
            return;
        };

        message.pinned_for_compaction = pinned;
        thread.updated_at = now_millis();
    }

    pub async fn set_thread_client_surface(
        &self,
        thread_id: &str,
        client_surface: zorai_protocol::ClientSurface,
    ) {
        self.thread_client_surfaces
            .write()
            .await
            .insert(thread_id.to_string(), client_surface);
        self.persist_thread_metadata_patch(
            thread_id,
            ThreadMetadataPatch::ClientSurface(Some(client_surface)),
        )
        .await;
    }

    pub async fn get_thread_client_surface(
        &self,
        thread_id: &str,
    ) -> Option<zorai_protocol::ClientSurface> {
        if let Some(client_surface) = self
            .thread_client_surfaces
            .read()
            .await
            .get(thread_id)
            .copied()
        {
            return Some(client_surface);
        }

        let client_surface = self
            .persisted_thread_metadata(thread_id)
            .await?
            .client_surface?;
        self.thread_client_surfaces
            .write()
            .await
            .insert(thread_id.to_string(), client_surface);
        Some(client_surface)
    }

    pub async fn clear_thread_client_surface(&self, thread_id: &str) {
        self.thread_client_surfaces.write().await.remove(thread_id);
        self.persist_thread_metadata_patch(thread_id, ThreadMetadataPatch::ClientSurface(None))
            .await;
    }

    pub async fn set_thread_skill_discovery_state(
        &self,
        thread_id: &str,
        state: LatestSkillDiscoveryState,
    ) {
        let persisted_state = state.clone();
        self.thread_skill_discovery_states
            .write()
            .await
            .insert(thread_id.to_string(), state);
        self.persist_thread_metadata_patch(
            thread_id,
            ThreadMetadataPatch::LatestSkillDiscoveryState(Some(persisted_state)),
        )
        .await;
    }

    pub async fn get_thread_skill_discovery_state(
        &self,
        thread_id: &str,
    ) -> Option<LatestSkillDiscoveryState> {
        if let Some(state) = self
            .thread_skill_discovery_states
            .read()
            .await
            .get(thread_id)
            .cloned()
        {
            return Some(state);
        }

        let state = self
            .persisted_thread_metadata(thread_id)
            .await?
            .latest_skill_discovery_state?;
        self.thread_skill_discovery_states
            .write()
            .await
            .insert(thread_id.to_string(), state.clone());
        Some(state)
    }

    pub async fn clear_thread_skill_discovery_state(&self, thread_id: &str) {
        self.thread_skill_discovery_states
            .write()
            .await
            .remove(thread_id);
        self.persist_thread_metadata_patch(
            thread_id,
            ThreadMetadataPatch::LatestSkillDiscoveryState(None),
        )
        .await;
    }

    pub async fn set_thread_memory_injection_state(
        &self,
        thread_id: &str,
        state: PromptMemoryInjectionState,
    ) {
        let persisted_state = state.clone();
        self.thread_memory_injection_state_map()
            .write()
            .await
            .insert(thread_id.to_string(), state);
        self.persist_thread_metadata_patch(
            thread_id,
            ThreadMetadataPatch::PromptMemoryInjectionState(Some(persisted_state)),
        )
        .await;
    }

    pub async fn get_thread_memory_injection_state(
        &self,
        thread_id: &str,
    ) -> Option<PromptMemoryInjectionState> {
        if let Some(state) = self
            .thread_memory_injection_state_map()
            .read()
            .await
            .get(thread_id)
            .cloned()
        {
            return Some(state);
        }

        let state = self
            .persisted_thread_metadata(thread_id)
            .await?
            .prompt_memory_injection_state?;
        self.thread_memory_injection_state_map()
            .write()
            .await
            .insert(thread_id.to_string(), state.clone());
        Some(state)
    }

    pub async fn clear_thread_memory_injection_state(&self, thread_id: &str) {
        self.thread_memory_injection_state_map()
            .write()
            .await
            .remove(thread_id);
        self.persist_thread_metadata_patch(
            thread_id,
            ThreadMetadataPatch::PromptMemoryInjectionState(None),
        )
        .await;
    }

    pub async fn set_goal_run_client_surface(
        &self,
        goal_run_id: &str,
        client_surface: zorai_protocol::ClientSurface,
    ) {
        self.goal_run_client_surfaces
            .write()
            .await
            .insert(goal_run_id.to_string(), client_surface);
    }

    pub async fn get_goal_run_client_surface(
        &self,
        goal_run_id: &str,
    ) -> Option<zorai_protocol::ClientSurface> {
        self.goal_run_client_surfaces
            .read()
            .await
            .get(goal_run_id)
            .copied()
    }

    pub async fn list_threads(&self) -> Vec<AgentThread> {
        self.list_threads_filtered(&ThreadListFilter::default())
            .await
    }

    pub async fn list_threads_paginated(
        &self,
        limit: Option<usize>,
        offset: usize,
        include_internal: bool,
    ) -> Vec<AgentThread> {
        self.list_threads_filtered(&ThreadListFilter {
            limit,
            offset,
            include_internal,
            ..ThreadListFilter::default()
        })
        .await
    }

    pub(crate) async fn list_threads_filtered(
        &self,
        filter: &ThreadListFilter,
    ) -> Vec<AgentThread> {
        let config = self.config.read().await.clone();
        let query = persisted_thread_list_query(filter, &config);
        match self.history.list_threads_filtered(&query).await {
            Ok(rows) if !rows.is_empty() || self.threads.read().await.is_empty() => {
                return rows
                    .into_iter()
                    .map(summarize_persisted_thread_for_list)
                    .collect();
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "failed to list persisted thread summaries with SQL");
            }
        }

        self.list_threads_filtered_from_memory(filter).await
    }

    async fn list_threads_filtered_from_memory(
        &self,
        filter: &ThreadListFilter,
    ) -> Vec<AgentThread> {
        let config = self.config.read().await.clone();
        let (agent_names, include_empty_agent_name) =
            persisted_thread_agent_name_filter(filter.agent_name.as_deref(), &config);
        let threads = self.threads.read().await;
        let mut list: Vec<AgentThread> = threads
            .values()
            .filter(|thread| {
                thread_matches_list_filter(thread, filter, &agent_names, include_empty_agent_name)
            })
            .map(summarize_thread_for_list)
            .collect();

        list.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let limit = filter.limit.unwrap_or(usize::MAX);
        list.into_iter().skip(filter.offset).take(limit).collect()
    }

    pub async fn get_thread(&self, thread_id: &str) -> Option<AgentThread> {
        self.get_thread_filtered(thread_id, false, None, 0)
            .await
            .map(|result| result.thread)
    }

    pub(crate) async fn live_thread(&self, thread_id: &str) -> Option<AgentThread> {
        self.threads.read().await.get(thread_id).cloned()
    }

    pub(crate) async fn get_thread_filtered(
        &self,
        thread_id: &str,
        include_internal: bool,
        message_limit: Option<usize>,
        message_offset: usize,
    ) -> Option<ThreadDetailResult> {
        self.get_thread_filtered_with_tool_run_pagination(
            thread_id,
            include_internal,
            message_limit,
            message_offset,
            false,
        )
        .await
    }

    pub(crate) async fn get_thread_filtered_with_tool_run_pagination(
        &self,
        thread_id: &str,
        include_internal: bool,
        message_limit: Option<usize>,
        message_offset: usize,
        collapse_tool_calls: bool,
    ) -> Option<ThreadDetailResult> {
        if let Some(limit) = message_limit {
            if let Some(detail) = self
                .get_persisted_thread_window(
                    thread_id,
                    include_internal,
                    limit,
                    message_offset,
                    collapse_tool_calls,
                )
                .await
            {
                tracing::info!(
                    thread_id,
                    message_limit = limit,
                    message_offset,
                    loaded_message_start = detail.loaded_message_start,
                    loaded_message_end = detail.loaded_message_end,
                    total_message_count = detail.total_message_count,
                    returned_messages = detail.thread.messages.len(),
                    source = "db",
                    "daemon loaded thread message window"
                );
                return Some(detail);
            }
        }

        if let Some(detail) = self
            .get_authoritative_persisted_thread_detail(
                thread_id,
                include_internal,
                message_limit,
                message_offset,
            )
            .await
        {
            tracing::info!(
                thread_id,
                message_limit,
                message_offset,
                loaded_message_start = detail.loaded_message_start,
                loaded_message_end = detail.loaded_message_end,
                total_message_count = detail.total_message_count,
                returned_messages = detail.thread.messages.len(),
                source = "db",
                "daemon loaded thread message window"
            );
            return Some(detail);
        }

        self.ensure_thread_messages_loaded(thread_id).await;
        let mut thread = self.threads.read().await.get(thread_id).cloned()?;
        if !thread_is_query_visible(&thread, include_internal) {
            return None;
        }
        let pinned_messages = pinned_message_summaries(&thread);
        let (active_context_window_start, active_context_window_end, active_context_window_tokens) =
            thread_context_window_summary(&thread.messages);

        let total_messages = thread.messages.len();
        let end = total_messages.saturating_sub(message_offset);
        let start = message_limit
            .map(|limit| end.saturating_sub(limit))
            .unwrap_or(0);
        let messages_truncated = start > 0 || end < total_messages;

        if messages_truncated {
            thread.messages = thread
                .messages
                .into_iter()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect();
        }

        let detail = ThreadDetailResult {
            thread,
            messages_truncated,
            total_message_count: total_messages,
            loaded_message_start: start,
            loaded_message_end: end,
            active_context_window_start,
            active_context_window_end,
            active_context_window_tokens,
            pinned_messages,
        };
        tracing::info!(
            thread_id,
            message_limit,
            message_offset,
            loaded_message_start = detail.loaded_message_start,
            loaded_message_end = detail.loaded_message_end,
            total_message_count = detail.total_message_count,
            returned_messages = detail.thread.messages.len(),
            source = "memory",
            "daemon loaded thread message window"
        );
        Some(detail)
    }

    async fn get_authoritative_persisted_thread_detail(
        &self,
        thread_id: &str,
        include_internal: bool,
        message_limit: Option<usize>,
        message_offset: usize,
    ) -> Option<ThreadDetailResult> {
        let existing_pinned = self
            .threads
            .read()
            .await
            .get(thread_id)
            .map(|thread| thread.pinned);

        let mut thread_shell = self.restore_thread_from_db(thread_id).await?;
        if let Some(pinned) = existing_pinned {
            thread_shell.pinned = pinned;
        }
        if !thread_is_query_visible(&thread_shell, include_internal) {
            return None;
        }

        let hidden_check = async {
            if include_internal {
                false
            } else {
                self.persisted_thread_hidden_by_message_content(thread_id)
                    .await
            }
        };
        let messages_fut = self.history.list_messages(thread_id, None);
        let context_window_fut = self.history.list_active_context_window(thread_id);
        let pinned_fut = persisted_pinned_message_summaries(self, thread_id);

        let (hidden, db_messages_result, context_window_result, pinned_messages) =
            tokio::join!(hidden_check, messages_fut, context_window_fut, pinned_fut);

        if hidden {
            return None;
        }
        let db_messages = db_messages_result.ok()?;
        thread_shell.messages = db_messages
            .into_iter()
            .filter_map(super::messaging::agent_message_from_db)
            .collect();

        let (active_context_window_start, active_context_window_end, active_context_window_tokens) =
            match context_window_result {
                Ok((rows, absolute_start, absolute_end)) => {
                    let messages = rows
                        .into_iter()
                        .filter_map(super::messaging::agent_message_from_db)
                        .collect::<Vec<_>>();
                    let (relative_start, _, tokens) = thread_context_window_summary(&messages);
                    (
                        absolute_start.saturating_add(relative_start),
                        absolute_end,
                        tokens,
                    )
                }
                Err(_) => thread_context_window_summary(&thread_shell.messages),
            };

        let mut adopted_db = true;
        let mut thread = thread_shell;
        {
            let mut threads = self.threads.write().await;
            if let Some(existing) = threads.get(thread_id) {
                if super::messaging::thread_in_memory_is_ahead_of_db(existing, &thread.messages) {
                    thread = existing.clone();
                    adopted_db = false;
                } else {
                    threads.insert(thread_id.to_string(), thread.clone());
                }
            } else {
                threads.insert(thread_id.to_string(), thread.clone());
            }
        }
        if adopted_db || !thread.messages.is_empty() {
            self.clear_thread_message_hydration_pending(thread_id).await;
        }

        let total_messages = thread.messages.len();

        let end = total_messages.saturating_sub(message_offset);
        let start = message_limit
            .map(|limit| end.saturating_sub(limit))
            .unwrap_or(0);
        let messages_truncated = start > 0 || end < total_messages;
        if messages_truncated {
            thread.messages = thread
                .messages
                .into_iter()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect();
        }

        Some(ThreadDetailResult {
            thread,
            messages_truncated,
            total_message_count: total_messages,
            loaded_message_start: start,
            loaded_message_end: end,
            active_context_window_start,
            active_context_window_end,
            active_context_window_tokens,
            pinned_messages,
        })
    }

    async fn get_lazy_persisted_thread_window(
        &self,
        thread_id: &str,
        include_internal: bool,
        message_limit: usize,
        message_offset: usize,
    ) -> Option<ThreadDetailResult> {
        if !self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id)
        {
            return None;
        }

        let _guard = self.thread_message_hydration_lock.lock().await;
        if !self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id)
        {
            return None;
        }

        self.get_persisted_thread_window(
            thread_id,
            include_internal,
            message_limit,
            message_offset,
            false,
        )
        .await
    }

    async fn get_persisted_thread_window(
        &self,
        thread_id: &str,
        include_internal: bool,
        message_limit: usize,
        message_offset: usize,
        collapse_tool_calls: bool,
    ) -> Option<ThreadDetailResult> {
        let existing_pinned = self
            .threads
            .read()
            .await
            .get(thread_id)
            .map(|thread| thread.pinned);
        let mut thread = self.restore_thread_from_db(thread_id).await?;
        if let Some(pinned) = existing_pinned {
            thread.pinned = pinned;
        }
        if !thread_is_query_visible(&thread, include_internal) {
            return None;
        }

        let hidden_check = async {
            if include_internal {
                false
            } else {
                self.persisted_thread_hidden_by_message_content(thread_id)
                    .await
            }
        };
        let window_fut = async {
            if collapse_tool_calls {
                self.history
                    .list_message_window_collapsing_tool_runs(
                        thread_id,
                        message_limit,
                        message_offset,
                    )
                    .await
            } else {
                self.history
                    .list_message_window(thread_id, message_limit, message_offset)
                    .await
            }
        };
        let pinned_fut = persisted_pinned_message_summaries(self, thread_id);
        let context_window_fut = self.history.list_active_context_window(thread_id);

        let (hidden, window_result, pinned_messages, context_window_result) =
            tokio::join!(hidden_check, window_fut, pinned_fut, context_window_fut);

        if hidden {
            return None;
        }
        let (db_messages, total_message_count, loaded_message_start, loaded_message_end) =
            window_result.ok()?;
        thread.messages = db_messages
            .into_iter()
            .filter_map(super::messaging::agent_message_from_db)
            .collect();

        let (active_context_window_start, active_context_window_end, active_context_window_tokens) =
            match context_window_result {
                Ok((rows, absolute_start, absolute_end)) => {
                    let messages = rows
                        .into_iter()
                        .filter_map(super::messaging::agent_message_from_db)
                        .collect::<Vec<_>>();
                    let (relative_start, _, tokens) = thread_context_window_summary(&messages);
                    (
                        absolute_start.saturating_add(relative_start),
                        absolute_end,
                        tokens,
                    )
                }
                Err(_) => thread_context_window_summary(&thread.messages),
            };

        let detail = ThreadDetailResult {
            thread,
            messages_truncated: loaded_message_start > 0
                || loaded_message_end < total_message_count,
            total_message_count,
            loaded_message_start,
            loaded_message_end,
            active_context_window_start,
            active_context_window_end,
            active_context_window_tokens,
            pinned_messages,
        };
        tracing::info!(
            thread_id,
            message_limit,
            message_offset,
            loaded_message_start = detail.loaded_message_start,
            loaded_message_end = detail.loaded_message_end,
            total_message_count = detail.total_message_count,
            returned_messages = detail.thread.messages.len(),
            source = "db",
            "daemon loaded thread message window"
        );
        Some(detail)
    }

    pub(crate) async fn get_thread_capped_for_ipc(
        &self,
        thread_id: &str,
        include_internal: bool,
    ) -> Option<ThreadDetailResult> {
        if let Some(detail) = self
            .get_persisted_thread_window(
                thread_id,
                include_internal,
                LAZY_CAPPED_IPC_MESSAGE_WINDOW,
                0,
                false,
            )
            .await
        {
            return Some(cap_thread_detail_for_ipc(detail));
        }

        let detail = self
            .get_thread_filtered(thread_id, include_internal, None, 0)
            .await?;

        Some(cap_thread_detail_for_ipc(detail))
    }

    pub async fn planner_required_for_thread(&self, thread_id: &str) -> bool {
        let latest_user_message = self
            .history
            .latest_user_message_content(thread_id)
            .await
            .ok()
            .flatten();
        planner_required_for_message(latest_user_message.as_deref().unwrap_or(""))
    }

    pub async fn delete_thread(&self, thread_id: &str) -> bool {
        let _ = self.stop_stream(thread_id).await;
        let live_thread_exists = self.threads.read().await.contains_key(thread_id);
        let persisted_thread_exists = match self.history.has_thread_id(thread_id).await {
            Ok(exists) => exists,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to check persisted thread before deletion"
                );
                false
            }
        };
        let thread_exists = live_thread_exists || persisted_thread_exists;
        if !thread_exists {
            return false;
        }

        if live_thread_exists && !persisted_thread_exists {
            self.persist_thread_by_id(thread_id).await;
        }
        self.maybe_record_session_abandon_on_thread_delete(thread_id)
            .await;
        self.threads.write().await.remove(thread_id);
        self.clear_thread_client_surface(thread_id).await;
        self.clear_thread_skill_discovery_state(thread_id).await;
        self.clear_thread_memory_injection_state(thread_id).await;
        self.clear_thread_structural_memory(thread_id).await;
        self.thread_identity_metadata
            .write()
            .await
            .remove(thread_id);
        self.thread_handoff_states.write().await.remove(thread_id);
        self.thread_participants.write().await.remove(thread_id);
        self.thread_participant_suggestions
            .write()
            .await
            .remove(thread_id);
        self.clear_thread_message_hydration_pending(thread_id).await;
        self.remove_repo_watcher(thread_id).await;
        self.thread_todos.write().await.remove(thread_id);
        self.thread_work_contexts.write().await.remove(thread_id);
        self.release_agent_terminals_for_thread(thread_id).await;
        if let Err(error) = self.history.delete_thread(thread_id).await {
            tracing::warn!(thread_id = %thread_id, %error, "failed to delete thread history");
        }
        self.persist_todos().await;
        self.persist_work_context().await;
        true
    }

    async fn maybe_record_session_abandon_on_thread_delete(&self, thread_id: &str) {
        let now = now_millis();
        let Some((last_assistant_message, last_assistant_timestamp)) = self
            .history
            .latest_unanswered_assistant_message(thread_id)
            .await
            .unwrap_or(None)
        else {
            return;
        };

        if now.saturating_sub(last_assistant_timestamp) > SESSION_ABANDON_WINDOW_MS {
            return;
        }

        if self
            .history
            .implicit_signal_exists(thread_id, "session_abandon")
            .await
            .unwrap_or(false)
        {
            return;
        }

        if let Err(error) = self
            .record_session_abandon_feedback(
                thread_id,
                last_assistant_message.trim(),
                last_assistant_timestamp,
                now,
            )
            .await
        {
            tracing::warn!(
                thread_id = %thread_id,
                error = %error,
                "failed to record session abandonment feedback on thread delete"
            );
        }
    }
}

impl AgentEngine {
    async fn persisted_thread_hidden_by_message_content(&self, thread_id: &str) -> bool {
        match self
            .history
            .thread_has_message_substring(thread_id, &hidden_thread_message_substrings())
            .await
        {
            Ok(hidden) => hidden,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    error = %error,
                    "failed to query persisted thread hidden-message markers"
                );
                true
            }
        }
    }
}

fn thread_is_visible_by_default(thread: &AgentThread) -> bool {
    !crate::agent::concierge::is_user_visible_thread(thread)
        && !crate::agent::agent_identity::is_participant_playground_thread(&thread.id)
        && !crate::agent::is_internal_handoff_thread(&thread.id)
}

fn thread_is_query_visible(thread: &AgentThread, include_internal: bool) -> bool {
    include_internal || thread_is_visible_by_default(thread)
}

fn list_filter_timestamp(value: Option<u64>) -> Option<i64> {
    value.map(|value| value.min(i64::MAX as u64) as i64)
}

fn hidden_thread_message_substrings() -> Vec<String> {
    vec![
        format!(
            "{} {}",
            crate::agent::agent_identity::PERSONA_ID_MARKER,
            crate::agent::agent_identity::WELES_AGENT_ID
        ),
        format!(
            "{} {}",
            crate::agent::agent_identity::PERSONA_ID_MARKER,
            crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE
        ),
        format!(
            "{} {}",
            crate::agent::agent_identity::PERSONA_ID_MARKER,
            crate::agent::agent_identity::WELES_VITALITY_SCOPE
        ),
    ]
}

fn persisted_thread_list_query(
    filter: &ThreadListFilter,
    config: &AgentConfig,
) -> crate::history::AgentThreadListQuery {
    let (agent_names, include_empty_agent_name) =
        persisted_thread_agent_name_filter(filter.agent_name.as_deref(), config);

    crate::history::AgentThreadListQuery {
        created_after: list_filter_timestamp(filter.created_after),
        created_before: list_filter_timestamp(filter.created_before),
        updated_after: list_filter_timestamp(filter.updated_after),
        updated_before: list_filter_timestamp(filter.updated_before),
        agent_names,
        include_empty_agent_name,
        title_query: filter
            .title_query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        title_excluded_prefixes: Vec::new(),
        pinned: filter.pinned,
        min_message_count: None,
        include_internal: filter.include_internal,
        excluded_ids: Vec::new(),
        hidden_id_prefixes: vec![
            crate::agent::agent_identity::PARTICIPANT_PLAYGROUND_THREAD_PREFIX.to_string(),
            crate::agent::thread_handoffs::INTERNAL_HANDOFF_THREAD_PREFIX.to_string(),
        ],
        hidden_message_substrings: hidden_thread_message_substrings(),
        limit: filter.limit,
        offset: filter.offset,
    }
}

fn persisted_thread_agent_name_filter(
    agent_name: Option<&str>,
    config: &AgentConfig,
) -> (Vec<String>, bool) {
    let Some(agent_name) = agent_name.map(str::trim).filter(|value| !value.is_empty()) else {
        return (Vec::new(), false);
    };

    let target = crate::agent::agent_identity::resolve_agent_target(agent_name, &config.sub_agents);
    let explicit_main = crate::agent::agent_identity::is_main_agent_scope(agent_name);
    let resolved_unknown_as_main = !explicit_main
        && target
            .scope_id
            .eq_ignore_ascii_case(crate::agent::agent_identity::MAIN_AGENT_ID);

    let mut names = vec![agent_name.to_string()];
    if !resolved_unknown_as_main {
        names.push(target.agent_name);
        names.push(target.scope_id);
    }

    let include_empty_agent_name = explicit_main;
    if include_empty_agent_name {
        names.extend([
            crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
            crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            crate::agent::agent_identity::MAIN_AGENT_PUBLIC_ALIAS.to_string(),
            crate::agent::agent_identity::MAIN_AGENT_ALIAS.to_string(),
            crate::agent::agent_identity::MAIN_AGENT_LEGACY_ALIAS.to_string(),
            crate::agent::agent_identity::MAIN_AGENT_FALLBACK_ALIAS.to_string(),
        ]);
    }

    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    (names, include_empty_agent_name)
}

fn summarize_persisted_thread_for_list(thread: zorai_protocol::AgentDbThread) -> AgentThread {
    let metadata = parse_thread_metadata(thread.metadata_json.as_deref());
    AgentThread {
        id: thread.id,
        agent_name: thread
            .agent_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        title: thread.title,
        messages: Vec::new(),
        pinned: metadata.pinned,
        upstream_thread_id: metadata.upstream_thread_id,
        upstream_transport: metadata.upstream_transport,
        upstream_provider: metadata.upstream_provider,
        upstream_model: metadata.upstream_model,
        upstream_assistant_id: metadata.upstream_assistant_id,
        created_at: thread.created_at.max(0) as u64,
        updated_at: thread.updated_at.max(0) as u64,
        total_input_tokens: thread.total_tokens.max(0) as u64,
        total_output_tokens: 0,
    }
}

fn thread_matches_agent_name_filter(
    thread: &AgentThread,
    agent_names: &[String],
    include_empty_agent_name: bool,
) -> bool {
    let actual = thread.agent_name.as_deref().map(str::trim).unwrap_or("");
    if actual.is_empty() {
        return include_empty_agent_name;
    }
    agent_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(actual))
}

fn thread_matches_list_filter(
    thread: &AgentThread,
    filter: &ThreadListFilter,
    agent_names: &[String],
    include_empty_agent_name: bool,
) -> bool {
    if !thread_is_query_visible(thread, filter.include_internal) {
        return false;
    }

    if let Some(created_after) = filter.created_after {
        if thread.created_at < created_after {
            return false;
        }
    }

    if let Some(created_before) = filter.created_before {
        if thread.created_at > created_before {
            return false;
        }
    }

    if let Some(updated_after) = filter.updated_after {
        if thread.updated_at < updated_after {
            return false;
        }
    }

    if let Some(updated_before) = filter.updated_before {
        if thread.updated_at > updated_before {
            return false;
        }
    }

    if let Some(pinned) = filter.pinned {
        if thread.pinned != pinned {
            return false;
        }
    }

    if filter.agent_name.is_some()
        && !thread_matches_agent_name_filter(thread, agent_names, include_empty_agent_name)
    {
        return false;
    }

    if let Some(title_query) = filter
        .title_query
        .as_deref()
        .map(str::trim)
        .filter(|query| !query.is_empty())
    {
        if !thread
            .title
            .to_ascii_lowercase()
            .contains(&title_query.to_ascii_lowercase())
        {
            return false;
        }
    }

    true
}

fn summarize_thread_for_list(thread: &AgentThread) -> AgentThread {
    AgentThread {
        id: thread.id.clone(),
        agent_name: thread.agent_name.clone(),
        title: thread.title.clone(),
        messages: Vec::new(),
        pinned: thread.pinned,
        upstream_thread_id: thread.upstream_thread_id.clone(),
        upstream_transport: thread.upstream_transport,
        upstream_provider: thread.upstream_provider.clone(),
        upstream_model: thread.upstream_model.clone(),
        upstream_assistant_id: thread.upstream_assistant_id.clone(),
        created_at: thread.created_at,
        updated_at: thread.updated_at,
        total_input_tokens: thread.total_input_tokens,
        total_output_tokens: thread.total_output_tokens,
    }
}

#[cfg(test)]
#[path = "tests/thread_crud.rs"]
mod tests;
