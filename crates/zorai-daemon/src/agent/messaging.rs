//! Message sending API — public interface, thread creation, and session routing.

use super::*;

mod concierge;
mod direct_messages;

pub(in crate::agent) use direct_messages::{spawn_internal_dm_worker, InternalDmJob};

fn clear_message_pin_state(message: &mut AgentMessage) {
    message.pinned_for_compaction = false;
}

pub(super) fn agent_message_from_db(msg: zorai_protocol::AgentDbMessage) -> Option<AgentMessage> {
    let role = match msg.role.as_str() {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "tool" => MessageRole::Tool,
        "system" => MessageRole::System,
        _ => return None,
    };

    let tool_calls: Option<Vec<ToolCall>> = msg
        .tool_calls_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok());
    let metadata = parse_message_metadata(msg.metadata_json.as_deref());

    Some(AgentMessage {
        id: msg.id.clone(),
        role,
        content: msg.content,
        content_blocks: metadata.content_blocks,
        tool_calls,
        tool_call_id: metadata.tool_call_id,
        tool_name: metadata.tool_name,
        tool_arguments: metadata.tool_arguments,
        tool_status: metadata.tool_status,
        weles_review: metadata.weles_review,
        input_tokens: msg.input_tokens.unwrap_or(0) as u64,
        output_tokens: msg.output_tokens.unwrap_or(0) as u64,
        cost: msg.cost_usd,
        provider: msg.provider,
        model: msg.model,
        api_transport: metadata.api_transport,
        response_id: metadata.response_id,
        upstream_message: metadata.upstream_message,
        provider_final_result: metadata.provider_final_result,
        author_agent_id: metadata.author_agent_id,
        author_agent_name: metadata.author_agent_name,
        reasoning: msg.reasoning,
        message_kind: metadata.message_kind,
        compaction_strategy: metadata.compaction_strategy,
        compaction_payload: metadata.compaction_payload,
        offloaded_payload_id: metadata.offloaded_payload_id,
        tool_output_preview_path: metadata.tool_output_preview_path,
        structural_refs: metadata.structural_refs,
        pinned_for_compaction: metadata.pinned_for_compaction,
        timestamp: msg.created_at as u64,
        feedback: metadata.feedback,
    })
}

fn sum_message_token_totals(messages: &[AgentMessage]) -> (u64, u64) {
    messages
        .iter()
        .fold((0u64, 0u64), |(input_acc, output_acc), message| {
            (
                input_acc.saturating_add(message.input_tokens),
                output_acc.saturating_add(message.output_tokens),
            )
        })
}

// Returns true if the in-memory thread is ahead of the DB hydration payload —
// i.e., the live state has messages the DB hasn't received yet. Hydrating would
// wipe out the newer in-memory turn before persistence can write it back.
// Triggers: more in-memory messages than DB delivered, a strictly newer
// timestamp/message id on the in-memory tail, or the same ids with richer live
// payloads (streaming assistant/tool stubs vs. empty DB rows). A trailing
// optimistic message without an id is treated as ahead even when counts match,
// because the daemon just appended it locally.
pub(super) fn thread_in_memory_is_ahead_of_db(
    thread: &AgentThread,
    db_messages: &[AgentMessage],
) -> bool {
    if thread.messages.len() > db_messages.len() {
        return true;
    }
    let Some(in_memory_tail) = thread.messages.last() else {
        return false;
    };
    if in_memory_tail.id.is_empty() {
        return true;
    }
    let Some(db_tail) = db_messages.last() else {
        return true;
    };
    if in_memory_tail.timestamp > db_tail.timestamp {
        return true;
    }
    if in_memory_tail.id != db_tail.id {
        return true;
    }
    memory_messages_are_richer_than_db(&thread.messages, db_messages)
}

fn memory_messages_are_richer_than_db(memory: &[AgentMessage], db: &[AgentMessage]) -> bool {
    memory.iter().any(|mem| {
        if mem.id.is_empty() {
            return true;
        }
        match db.iter().find(|db_msg| db_msg.id == mem.id) {
            None => true,
            Some(db_msg) => message_is_richer_than(mem, db_msg),
        }
    })
}

fn message_is_richer_than(memory: &AgentMessage, db: &AgentMessage) -> bool {
    if memory.content.len() > db.content.len() {
        return true;
    }
    if memory.content_blocks.len() > db.content_blocks.len() {
        return true;
    }
    let mem_args = memory.tool_arguments.as_deref().unwrap_or("").len();
    let db_args = db.tool_arguments.as_deref().unwrap_or("").len();
    if mem_args > db_args {
        return true;
    }
    let mem_calls = memory.tool_calls.as_ref().map(Vec::len).unwrap_or(0);
    let db_calls = db.tool_calls.as_ref().map(Vec::len).unwrap_or(0);
    if mem_calls > db_calls {
        return true;
    }
    memory.reasoning.as_deref().unwrap_or("").len() > db.reasoning.as_deref().unwrap_or("").len()
}

impl AgentEngine {
    pub(super) async fn budget_exceeded_task_for_thread(&self, thread_id: &str) -> Option<String> {
        let budget_exceeded_status = serde_json::to_value(TaskStatus::BudgetExceeded)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "budget_exceeded".to_string());
        let query = crate::history::AgentTaskListQuery {
            id: None,
            status: Some(budget_exceeded_status),
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
            limit: Some(1),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        };
        match self.history.list_agent_task_refs_filtered(&query).await {
            Ok(task_refs) => task_refs.into_iter().next().map(|(task_id, _, _)| task_id),
            Err(error) => {
                tracing::warn!(
                    "failed to query budget-exceeded task id for thread {thread_id}: {error}"
                );
                self.list_tasks_filtered(&query)
                    .await
                    .into_iter()
                    .next()
                    .map(|task| task.id)
            }
        }
    }

    #[cfg(test)]
    pub(crate) async fn set_thread_message_hydration_test_delay(&self, delay: Duration) {
        *self.thread_message_hydration_test_delay.lock().await = Some(delay);
    }

    pub(super) async fn clear_thread_message_hydration_pending(&self, thread_id: &str) {
        self.thread_message_hydration_pending
            .write()
            .await
            .remove(thread_id);
    }

    pub(crate) async fn messages_for_read_only_context(
        &self,
        thread_id: &str,
    ) -> Option<Vec<AgentMessage>> {
        let pending = self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id);
        if !pending {
            return self
                .threads
                .read()
                .await
                .get(thread_id)
                .map(|thread| thread.messages.clone());
        }
        let (rows, _, _) = self
            .history
            .list_active_context_window(thread_id)
            .await
            .ok()?;
        Some(
            rows.into_iter()
                .filter_map(agent_message_from_db)
                .collect(),
        )
    }

    pub(crate) async fn ensure_thread_messages_loaded(&self, thread_id: &str) -> bool {
        let needs_hydration = self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id);
        if !needs_hydration {
            return self
                .ensure_thread_messages_loaded_from_live_or_db(thread_id)
                .await;
        }

        let _guard = self.thread_message_hydration_lock.lock().await;
        let still_needs_hydration = self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id);
        if !still_needs_hydration {
            return self
                .ensure_thread_messages_loaded_from_live_or_db(thread_id)
                .await;
        }

        #[cfg(test)]
        if let Some(delay) = *self.thread_message_hydration_test_delay.lock().await {
            tokio::time::sleep(delay).await;
        }

        let (db_messages_result, totals_result) = tokio::join!(
            self.history.list_messages(thread_id, None),
            self.history.thread_message_token_totals(thread_id),
        );
        let Some(db_messages) = db_messages_result.ok() else {
            return false;
        };
        let messages: Vec<AgentMessage> = db_messages
            .into_iter()
            .filter_map(agent_message_from_db)
            .collect();
        let (total_input_tokens, total_output_tokens) =
            totals_result.unwrap_or_else(|_| sum_message_token_totals(&messages));

        let updated = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return false;
            };
            let in_memory_ahead = thread_in_memory_is_ahead_of_db(thread, &messages);
            if !in_memory_ahead {
                thread.messages = messages;
                thread.total_input_tokens = total_input_tokens;
                thread.total_output_tokens = total_output_tokens;
            }
            true
        };
        if updated {
            self.clear_thread_message_hydration_pending(thread_id).await;
        }
        updated
    }

    async fn ensure_thread_messages_loaded_from_live_or_db(&self, thread_id: &str) -> bool {
        if self.threads.read().await.contains_key(thread_id) {
            return true;
        }

        let Some(restored) = self.restore_thread_with_messages_from_db(thread_id).await else {
            return false;
        };
        self.threads
            .write()
            .await
            .insert(thread_id.to_string(), restored);
        true
    }

    async fn prepare_internal_dm_thread(
        &self,
        sender: &str,
        recipient: &str,
        wrapped_content: &str,
    ) -> String {
        let dm_thread_id = internal_dm_thread_id(sender, recipient);
        let _ = self
            .get_or_create_thread_with_target(Some(&dm_thread_id), wrapped_content, Some(recipient))
            .await;
        self.set_thread_handoff_state(
            &dm_thread_id,
            initial_thread_handoff_state(
                &dm_thread_id,
                Some(canonical_agent_name(recipient)),
                now_millis(),
            ),
        )
        .await;
        dm_thread_id
    }

    pub async fn delete_thread_messages(
        &self,
        thread_id: &str,
        message_ids: &[String],
    ) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }

        let id_set: std::collections::HashSet<&str> =
            message_ids.iter().map(String::as_str).collect();

        let removed = {
            let threads = self.threads.read().await;
            threads
                .get(thread_id)
                .map(|thread| {
                    thread
                        .messages
                        .iter()
                        .filter(|message| id_set.contains(message.id.as_str()))
                        .count()
                })
                .unwrap_or(0)
        };

        let id_refs: Vec<&str> = message_ids.iter().map(String::as_str).collect();
        let db_removed = self
            .history
            .delete_messages(thread_id, &id_refs)
            .await
            .unwrap_or(0);

        let total = removed.max(db_removed);
        if total > 0 {
            let existing_pinned = {
                let threads = self.threads.read().await;
                threads.get(thread_id).map(|thread| thread.pinned)
            };

            if let Some(mut restored) = self.restore_thread_with_messages_from_db(thread_id).await {
                if let Some(pinned) = existing_pinned {
                    restored.pinned = pinned;
                }

                let mut threads = self.threads.write().await;
                threads.insert(thread_id.to_string(), restored);
            } else {
                let mut threads = self.threads.write().await;
                if let Some(thread) = threads.get_mut(thread_id) {
                    for message in &mut thread.messages {
                        if id_set.contains(message.id.as_str()) {
                            clear_message_pin_state(message);
                        }
                    }
                    thread
                        .messages
                        .retain(|message| !id_set.contains(message.id.as_str()));
                    thread.updated_at = now_millis();
                    thread.total_input_tokens = thread
                        .messages
                        .iter()
                        .map(|message| message.input_tokens)
                        .sum();
                    thread.total_output_tokens = thread
                        .messages
                        .iter()
                        .map(|message| message.output_tokens)
                        .sum();
                }
            }

            self.repair_tool_call_sequence(thread_id).await;
            self.clear_thread_continuation_state(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
            tracing::info!(
                thread_id,
                in_memory = removed,
                sqlite = db_removed,
                "deleted messages and reconciled thread state"
            );
        }
        Ok(total)
    }

    pub async fn restore_thread_messages(
        &self,
        thread_id: &str,
        message_ids: &[String],
    ) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }

        let id_refs: Vec<&str> = message_ids.iter().map(String::as_str).collect();
        let restored = self
            .history
            .restore_messages(thread_id, &id_refs)
            .await
            .unwrap_or(0);

        if restored > 0 {
            let existing_pinned = {
                let threads = self.threads.read().await;
                threads.get(thread_id).map(|thread| thread.pinned)
            };

            if let Some(mut restored_thread) =
                self.restore_thread_with_messages_from_db(thread_id).await
            {
                if let Some(pinned) = existing_pinned {
                    restored_thread.pinned = pinned;
                }

                let mut threads = self.threads.write().await;
                threads.insert(thread_id.to_string(), restored_thread);
            }

            self.repair_tool_call_sequence(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
            tracing::info!(thread_id, restored, "restored soft-deleted messages");
        }

        Ok(restored)
    }

    pub async fn seed_thread_context(
        &self,
        thread_id: Option<&str>,
        context: &[zorai_protocol::AgentDbMessage],
    ) {
        let tid = match thread_id {
            Some(id) => id.to_string(),
            None => return,
        };

        let has_pending_persisted_messages = self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(&tid);
        if has_pending_persisted_messages {
            tracing::debug!(
                thread_id = %tid,
                "skipped frontend context seeding because persisted thread history is pending lazy hydration"
            );
            return;
        }

        let mut threads = self.threads.write().await;
        let needs_seeding = match threads.get(&tid) {
            None => true,
            Some(t) => t.messages.is_empty(),
        };
        if !needs_seeding || context.is_empty() {
            return;
        }

        let messages: Vec<AgentMessage> = context
            .iter()
            .filter_map(|msg| {
                let role = match msg.role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "tool" => MessageRole::Tool,
                    "system" => MessageRole::System,
                    _ => return None,
                };
                let tool_calls: Option<Vec<ToolCall>> = msg
                    .tool_calls_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok());
                let metadata = parse_message_metadata(msg.metadata_json.as_deref());
                Some(AgentMessage {
                    id: msg.id.clone(),
                    role,
                    content: msg.content.clone(),
                    content_blocks: metadata.content_blocks,
                    tool_calls,
                    tool_call_id: metadata.tool_call_id,
                    tool_name: metadata.tool_name,
                    tool_arguments: metadata.tool_arguments,
                    tool_status: metadata.tool_status,
                    weles_review: metadata.weles_review.clone(),
                    input_tokens: msg.input_tokens.unwrap_or(0) as u64,
                    output_tokens: msg.output_tokens.unwrap_or(0) as u64,
                    cost: None,
                    provider: msg.provider.clone(),
                    model: msg.model.clone(),
                    api_transport: metadata.api_transport,
                    response_id: metadata.response_id,
                    upstream_message: metadata.upstream_message,
                    provider_final_result: metadata.provider_final_result,
                    author_agent_id: metadata.author_agent_id,
                    author_agent_name: metadata.author_agent_name,
                    reasoning: msg.reasoning.clone(),
                    message_kind: metadata.message_kind,
                    compaction_strategy: metadata.compaction_strategy,
                    compaction_payload: metadata.compaction_payload,
                    offloaded_payload_id: metadata.offloaded_payload_id,
                    tool_output_preview_path: None,
                    structural_refs: metadata.structural_refs,
                    pinned_for_compaction: metadata.pinned_for_compaction,
                    timestamp: msg.created_at as u64,
                    feedback: None,
                })
            })
            .collect();

        if messages.is_empty() {
            return;
        }

        let total_in: u64 = messages.iter().map(|m| m.input_tokens).sum();
        let total_out: u64 = messages.iter().map(|m| m.output_tokens).sum();
        let title = messages
            .iter()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.content.chars().take(50).collect::<String>())
            .unwrap_or_else(|| "Continued conversation".into());

        tracing::info!(thread_id = %tid, context_messages = messages.len(), "seeding thread with frontend context");

        let active_agent_id = current_agent_scope_id();
        let created_at = now_millis();
        threads.insert(
            tid.clone(),
            AgentThread {
                id: tid.clone(),
                agent_name: Some(canonical_agent_name(&active_agent_id).to_string()),
                title,
                messages,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at,
                updated_at: created_at,
                total_input_tokens: total_in,
                total_output_tokens: total_out,
            },
        );
        drop(threads);
        self.clear_thread_message_hydration_pending(&tid).await;
        self.thread_handoff_states.write().await.insert(
            tid.clone(),
            initial_thread_handoff_state(
                &tid,
                Some(canonical_agent_name(&active_agent_id)),
                created_at,
            ),
        );
    }

    pub(super) async fn reserve_unique_thread_id(&self) -> String {
        loop {
            let candidate = format!("thread_{}", Uuid::new_v4());
            if self.task_thread_id_conflicts(&candidate).await {
                continue;
            }

            if self.threads.read().await.contains_key(&candidate) {
                continue;
            }

            if self
                .history
                .has_thread_id(&candidate)
                .await
                .unwrap_or(false)
            {
                continue;
            }

            return candidate;
        }
    }

    async fn task_thread_id_conflicts(&self, thread_id: &str) -> bool {
        match self.history.has_agent_task_for_thread(thread_id).await {
            Ok(conflicts) => conflicts,
            Err(error) => {
                tracing::warn!(
                    thread_id,
                    "failed to query task thread id conflict from sqlite: {error}"
                );
                false
            }
        }
    }

    /// Get or create a thread, returning the thread ID and whether it was newly created.
    pub(super) async fn get_or_create_thread(
        &self,
        thread_id: Option<&str>,
        content: &str,
    ) -> (String, bool) {
        self.get_or_create_thread_with_target(thread_id, content, None)
            .await
    }

    pub(crate) async fn get_or_create_thread_with_target(
        &self,
        thread_id: Option<&str>,
        content: &str,
        target_agent_id: Option<&str>,
    ) -> (String, bool) {
        let given_id = thread_id.map(|s| s.to_string());
        let id = match given_id {
            Some(id) => id,
            None => self.reserve_unique_thread_id().await,
        };
        let title = super::thread_title::placeholder_thread_title(content);
        let mut created = false;
        let resolved_target = if let Some(target_agent_id) = target_agent_id {
            Some(crate::agent::agent_identity::resolve_agent_target(
                target_agent_id,
                &self.list_sub_agents().await,
            ))
        } else {
            None
        };

        let exists = self.threads.read().await.contains_key(&id);
        if !exists {
            if let Some(restored) = self.restore_thread_from_db(&id).await {
                tracing::info!(thread_id = %id, "restored thread shell from history");
                self.threads.write().await.insert(id.clone(), restored);
            } else {
                let mut threads = self.threads.write().await;
                created = true;
                let created_at = now_millis();
                let active_agent_id = resolved_target
                    .as_ref()
                    .map(|target| target.scope_id.clone())
                    .unwrap_or_else(current_agent_scope_id);
                let active_agent_name = resolved_target
                    .as_ref()
                    .map(|target| target.agent_name.clone())
                    .unwrap_or_else(|| canonical_agent_name(&active_agent_id).to_string());
                threads.insert(
                    id.clone(),
                    AgentThread {
                        id: id.clone(),
                        agent_name: Some(active_agent_name.clone()),
                        title: title.clone(),
                        messages: Vec::new(),
                        pinned: false,
                        upstream_thread_id: None,
                        upstream_transport: None,
                        upstream_provider: None,
                        upstream_model: None,
                        upstream_assistant_id: None,
                        created_at,
                        updated_at: created_at,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                    },
                );
                self.thread_handoff_states.write().await.insert(
                    id.clone(),
                    ThreadHandoffState {
                        origin_agent_id: active_agent_id.clone(),
                        active_agent_id: active_agent_id.clone(),
                        responder_stack: vec![ThreadResponderFrame {
                            agent_id: active_agent_id.clone(),
                            agent_name: active_agent_name.clone(),
                            entered_at: created_at,
                            entered_via_handoff_event_id: None,
                            linked_thread_id: None,
                        }],
                        events: Vec::new(),
                        pending_approval_id: None,
                    },
                );
                let _ = self.event_tx.send(AgentEvent::ThreadCreated {
                    thread_id: id.clone(),
                    title,
                    agent_name: Some(active_agent_name),
                });
            }
        }
        if let Some(target) = resolved_target.as_ref().filter(|_| !created) {
            self.retarget_existing_thread_to_agent(&id, target).await;
        }
        if created {
            self.clear_thread_message_hydration_pending(&id).await;
            let pending_workspace_context =
                self.pending_thread_workspace_contexts.write().await.remove(&id);
            if let Some(context) = pending_workspace_context {
                self.set_thread_workspace_context(&id, Some(context)).await;
            }
            self.queue_auto_thread_title_if_enabled(&id, content).await;
        }
        (id, created)
    }

    async fn retarget_existing_thread_to_agent(
        &self,
        thread_id: &str,
        target: &crate::agent::agent_identity::ResolvedAgentTarget,
    ) {
        let entered_at = now_millis();
        let previous_state = self.thread_handoff_state(thread_id).await;
        let mut state = previous_state.unwrap_or_else(|| {
            initial_thread_handoff_state(thread_id, Some(target.agent_name.as_str()), entered_at)
        });
        state.origin_agent_id = target.scope_id.clone();
        state.active_agent_id = target.scope_id.clone();
        state.responder_stack = vec![ThreadResponderFrame {
            agent_id: target.scope_id.clone(),
            agent_name: target.agent_name.clone(),
            entered_at,
            entered_via_handoff_event_id: None,
            linked_thread_id: None,
        }];
        state.pending_approval_id = None;
        self.thread_handoff_states
            .write()
            .await
            .insert(thread_id.to_string(), state);
        if let Some(thread) = self.threads.write().await.get_mut(thread_id) {
            thread.agent_name = Some(target.agent_name.clone());
            thread.updated_at = entered_at;
        }
        self.thread_execution_profiles
            .write()
            .await
            .remove(thread_id);
    }

    /// Restore a thread shell from SQLite without loading its message payloads.
    pub(super) async fn restore_thread_from_db(&self, thread_id: &str) -> Option<AgentThread> {
        let db_thread = self.history.get_thread(thread_id).await.ok().flatten()?;
        let thread_metadata = parse_thread_metadata(db_thread.metadata_json.as_deref());

        let has_thread_participants = !thread_metadata.thread_participants.is_empty();
        let handoff_state = normalized_thread_handoff_state(
            thread_id,
            db_thread.agent_name.as_deref(),
            db_thread.created_at as u64,
            thread_metadata.handoff_state,
        );
        let hydrated_agent_name = visible_thread_owner_agent_name_for_handoff_state(
            thread_id,
            &handoff_state,
            has_thread_participants,
        )
        .or_else(|| {
            // The DB row's `agent_name` is the most authoritative source for
            // custom sub-agent threads: the canonical helper only knows the
            // built-in personas and collapses unknown ids (e.g. user-defined
            // sub-agent UUIDs) to "Swarog".
            db_thread
                .agent_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            // Fall back to the responder stack's most recent frame name, which
            // preserves the sub-agent's display name (set at handoff time).
            handoff_state
                .responder_stack
                .last()
                .map(|frame| frame.agent_name.trim())
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| canonical_agent_name(&handoff_state.active_agent_id).to_string());

        let ParsedThreadMetadata {
            identity: meta_identity,
            client_surface: meta_client_surface,
            execution_profile: meta_execution_profile,
            workspace_context: _,
            thread_participants: meta_thread_participants,
            thread_participant_suggestions: meta_thread_participant_suggestions,
            latest_skill_discovery_state: meta_skill_discovery,
            prompt_memory_injection_state: meta_memory_injection,
            handoff_state: _,
            pinned: _,
            upstream_thread_id: meta_upstream_thread_id,
            upstream_transport: meta_upstream_transport,
            upstream_provider: meta_upstream_provider,
            upstream_model: meta_upstream_model,
            upstream_assistant_id: meta_upstream_assistant_id,
        } = thread_metadata;

        let thread_id_owned = thread_id.to_string();

        let totals_fut = async {
            self.history
                .thread_message_token_totals(thread_id)
                .await
                .unwrap_or_else(|_| (db_thread.total_tokens.max(0) as u64, 0))
        };
        let client_surface_fut = async {
            if let Some(client_surface) = meta_client_surface {
                self.thread_client_surfaces
                    .write()
                    .await
                    .insert(thread_id_owned.clone(), client_surface);
            }
        };
        let identity_fut = async {
            if let Some(identity) = meta_identity {
                self.thread_identity_metadata
                    .write()
                    .await
                    .insert(thread_id_owned.clone(), identity);
            }
            // Skip the DB-None branch: a stale persist may have captured an
            // empty row before the latest in-memory write, and `remove` on
            // an absent key is a no-op anyway.
        };
        let execution_profile_fut = async {
            if let Some(profile) = meta_execution_profile {
                let mut profiles = self.thread_execution_profiles.write().await;
                profiles.entry(thread_id_owned.clone()).or_insert(profile);
            }
        };
        let participants_fut = async {
            let mut guard = self.thread_participants.write().await;
            if meta_thread_participants.is_empty() {
                // Don't downgrade non-empty in-memory state with an empty DB
                // snapshot: a concurrent agent-loop persist may have committed
                // a snapshot it captured before the latest upsert wrote, so
                // the DB row briefly lacks the participant even though the
                // engine's map is the authoritative live state.
                if !guard.get(thread_id).is_some_and(|entry| !entry.is_empty()) {
                    guard.remove(thread_id);
                }
            } else {
                guard.insert(thread_id_owned.clone(), meta_thread_participants);
            }
        };
        let participant_suggestions_fut = async {
            let mut guard = self.thread_participant_suggestions.write().await;
            if meta_thread_participant_suggestions.is_empty() {
                if !guard.get(thread_id).is_some_and(|entry| !entry.is_empty()) {
                    guard.remove(thread_id);
                }
            } else {
                guard.insert(thread_id_owned.clone(), meta_thread_participant_suggestions);
            }
        };
        let skill_discovery_fut = async {
            if let Some(state) = meta_skill_discovery {
                self.thread_skill_discovery_states
                    .write()
                    .await
                    .insert(thread_id_owned.clone(), state);
            }
        };
        let memory_injection_fut = async {
            if let Some(state) = meta_memory_injection {
                self.thread_memory_injection_state_map()
                    .write()
                    .await
                    .insert(thread_id_owned.clone(), state);
            }
        };
        let handoff_fut = async {
            // Don't downgrade a live handoff state with a stale DB snapshot.
            // Handoff events and the responder stack are append-only during a
            // thread's lifetime, so a strictly longer log or stack in memory
            // means a write happened after the snapshot we're about to insert
            // was captured — keep the live state.
            let mut guard = self.thread_handoff_states.write().await;
            let in_memory_is_fresher = guard.get(thread_id).is_some_and(|existing| {
                existing.events.len() > handoff_state.events.len()
                    || existing.responder_stack.len() > handoff_state.responder_stack.len()
            });
            if !in_memory_is_fresher {
                guard.insert(thread_id_owned.clone(), handoff_state.clone());
            }
        };
        let pending_fut = async {
            let mut pending = self.thread_message_hydration_pending.write().await;
            if db_thread.message_count > 0 {
                pending.insert(thread_id_owned.clone());
            } else {
                pending.remove(thread_id);
            }
        };

        let ((total_input, total_output), _, _, _, _, _, _, _, _, _) = tokio::join!(
            totals_fut,
            client_surface_fut,
            identity_fut,
            execution_profile_fut,
            participants_fut,
            participant_suggestions_fut,
            skill_discovery_fut,
            memory_injection_fut,
            handoff_fut,
            pending_fut,
        );

        Some(AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(hydrated_agent_name),
            title: db_thread.title,
            messages: Vec::new(),
            pinned: false,
            upstream_thread_id: meta_upstream_thread_id,
            upstream_transport: meta_upstream_transport,
            upstream_provider: meta_upstream_provider,
            upstream_model: meta_upstream_model,
            upstream_assistant_id: meta_upstream_assistant_id,
            created_at: db_thread.created_at as u64,
            updated_at: db_thread.updated_at as u64,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
        })
    }

    /// Restore a thread and its complete message payloads from SQLite.
    pub(super) async fn restore_thread_with_messages_from_db(
        &self,
        thread_id: &str,
    ) -> Option<AgentThread> {
        let (thread_opt, db_messages_result) = tokio::join!(
            self.restore_thread_from_db(thread_id),
            self.history.list_messages(thread_id, None),
        );
        let mut thread = thread_opt?;
        let db_messages = db_messages_result.ok()?;
        let messages: Vec<AgentMessage> = db_messages
            .into_iter()
            .filter_map(agent_message_from_db)
            .collect();
        thread.messages = messages;
        self.clear_thread_message_hydration_pending(thread_id).await;
        Some(thread)
    }

    /// Run a complete agent turn in a thread.
    pub async fn send_message(&self, thread_id: Option<&str>, content: &str) -> Result<String> {
        Ok(Box::pin(self.send_message_inner(
            thread_id, content, None, None, None, None, None, None, None, true,
        ))
        .await?
        .thread_id)
    }

    pub(super) async fn send_internal_message_as(
        &self,
        thread_id: Option<&str>,
        target_agent_id: &str,
        content: &str,
    ) -> Result<String> {
        let effective_thread_id =
            if crate::agent::agent_identity::canonical_agent_id(target_agent_id)
                != crate::agent::agent_identity::MAIN_AGENT_ID
            {
                let (thread_id, _) = self
                    .get_or_create_thread_with_target(thread_id, content, Some(target_agent_id))
                    .await;
                Some(thread_id)
            } else {
                thread_id.map(str::to_string)
            };

        Ok(Box::pin(self.send_message_inner(
            effective_thread_id.as_deref(),
            content,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
        ))
        .await?
        .thread_id)
    }

    pub async fn send_message_with_ephemeral_user_override(
        &self,
        thread_id: Option<&str>,
        stored_content: &str,
        llm_user_override: &str,
        stream_chunk_timeout: std::time::Duration,
    ) -> Result<String> {
        Ok(Box::pin(self.send_message_inner(
            thread_id,
            stored_content,
            None,
            None,
            None,
            None,
            Some(llm_user_override),
            Some(stream_chunk_timeout),
            None,
            true,
        ))
        .await?
        .thread_id)
    }

    pub async fn send_message_with_agent_scope_override(
        &self,
        thread_id: Option<&str>,
        stored_content: &str,
        llm_user_override: &str,
        agent_scope: Option<&str>,
        stream_chunk_timeout: std::time::Duration,
    ) -> Result<String> {
        Ok(Box::pin(self.send_message_inner_with_options(
            thread_id,
            stored_content,
            None,
            None,
            None,
            None,
            Some(llm_user_override),
            Some(stream_chunk_timeout),
            None,
            true,
            false,
            true,
            true,
            agent_scope,
        ))
        .await?
        .thread_id)
    }

    pub async fn send_message_with_session(
        &self,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<String> {
        self.send_message_with_session_and_surface(
            thread_id,
            preferred_session_hint,
            content,
            None,
            None,
        )
        .await
    }

    pub async fn send_message_with_session_and_surface(
        &self,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
        content_blocks_json: Option<&str>,
        client_surface: Option<zorai_protocol::ClientSurface>,
    ) -> Result<String> {
        if let Some(thread_id) = thread_id {
            if let Some(task_id) = self.budget_exceeded_task_for_thread(thread_id).await {
                anyhow::bail!(
                    "thread {thread_id} is locked because task {task_id} exhausted its execution budget"
                );
            }
        }
        let outcome = Box::pin(self.send_message_inner(
            thread_id,
            content,
            content_blocks_json,
            None,
            preferred_session_hint,
            None,
            None,
            None,
            client_surface,
            true,
        ))
        .await?;
        Ok(outcome.thread_id)
    }

    pub async fn send_message_with_session_surface_and_target(
        &self,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
        content_blocks_json: Option<&str>,
        client_surface: Option<zorai_protocol::ClientSurface>,
        target_agent_id: Option<&str>,
    ) -> Result<String> {
        let effective_thread_id = if target_agent_id.is_some() {
            let (thread_id, _) = self
                .get_or_create_thread_with_target(thread_id, content, target_agent_id)
                .await;
            Some(thread_id)
        } else {
            thread_id.map(str::to_string)
        };

        self.send_message_with_session_and_surface(
            effective_thread_id.as_deref(),
            preferred_session_hint,
            content,
            content_blocks_json,
            client_surface,
        )
        .await
    }

    pub(super) async fn send_task_message(
        &self,
        task_id: &str,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        backend_override: Option<&str>,
        content: &str,
    ) -> Result<SendMessageOutcome> {
        if let Some(thread_id) = thread_id {
            if self
                .thread_has_task_dispatch_user_message(thread_id, task_id)
                .await
            {
                return self
                    .resend_existing_user_message_for_task(thread_id, content, task_id)
                    .await;
            }
        }
        let client_surface = if let Some(thread_id) = thread_id {
            self.get_thread_client_surface(thread_id).await
        } else {
            let goal_run_id = match self.history.agent_task_goal_context(task_id).await {
                Ok(Some(task)) => task.goal_run_id,
                Ok(None) | Err(_) => {
                    let tasks = self.tasks.lock().await;
                    tasks
                        .iter()
                        .find(|task| task.id == task_id)
                        .and_then(|task| task.goal_run_id.clone())
                }
            };
            match goal_run_id {
                Some(goal_run_id) => self.get_goal_run_client_surface(&goal_run_id).await,
                None => None,
            }
        };
        Box::pin(self.send_message_inner(
            thread_id,
            content,
            None,
            Some(task_id),
            preferred_session_hint,
            backend_override,
            None,
            None,
            client_surface,
            false,
        ))
        .await
    }

    pub(super) async fn thread_has_task_dispatch_user_message(
        &self,
        thread_id: &str,
        task_id: &str,
    ) -> bool {
        if !self.ensure_thread_messages_loaded(thread_id).await {
            return false;
        }
        let handle = crate::agent::tool_executor::stored_task_slug(self, task_id).await;
        let threads = self.threads.read().await;
        let Some(thread) = threads.get(thread_id) else {
            return false;
        };
        thread.messages.iter().any(|message| {
            message.role == MessageRole::User
                && (task_prompt::content_has_task_dispatch_prompt(&message.content, task_id)
                    || handle.as_deref().is_some_and(|handle| {
                        message
                            .content
                            .contains(&task_prompt::task_dispatch_prompt_marker(handle))
                    }))
        })
    }

    pub(super) async fn send_internal_task_message(
        &self,
        sender: &str,
        recipient: &str,
        task_id: &str,
        preferred_session_hint: Option<&str>,
        backend_override: Option<&str>,
        content: &str,
    ) -> Result<SendMessageOutcome> {
        let wrapped = wrap_internal_message(sender, recipient, content);
        let dm_thread_id = self
            .prepare_internal_dm_thread(sender, recipient, &wrapped)
            .await;
        let outcome = Box::pin(self.send_message_inner(
            Some(&dm_thread_id),
            &wrapped,
            None,
            Some(task_id),
            preferred_session_hint,
            backend_override,
            None,
            None,
            None,
            false,
        ))
        .await?;
        self.ensure_thread_identity(
            &dm_thread_id,
            &internal_dm_thread_title(sender, recipient),
            false,
        )
        .await;
        Ok(outcome)
    }

    pub(super) async fn ensure_thread_identity(&self, thread_id: &str, title: &str, pinned: bool) {
        let mut threads = self.threads.write().await;
        if let Some(thread) = threads.get_mut(thread_id) {
            thread.title = title.to_string();
            thread.pinned = pinned;
            thread.updated_at = now_millis();
        }
        drop(threads);
        self.persist_thread_by_id(thread_id).await;
    }
}

#[cfg(test)]
#[path = "tests/messaging/mod.rs"]
mod tests;
