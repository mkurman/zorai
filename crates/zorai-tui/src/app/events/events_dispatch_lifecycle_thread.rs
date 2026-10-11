use super::*;

impl TuiModel {
    pub(in crate::app) fn handle_lifecycle_thread_client_event(
        &mut self,
        event: ClientEvent,
    ) -> Option<ClientEvent> {
        match event {
            ClientEvent::Connected => {
                self.handle_connected_event();
                None
            }
            ClientEvent::Disconnected => {
                self.handle_disconnected_event();
                None
            }
            ClientEvent::Reconnecting { delay_secs } => {
                self.handle_reconnecting_event(delay_secs);
                None
            }
            ClientEvent::SessionSpawned { session_id } => {
                self.handle_session_spawned_event(session_id);
                None
            }
            ClientEvent::ApprovalRequired {
                approval_id,
                command,
                rationale,
                reasons,
                risk_level,
                blast_radius,
            } => {
                self.handle_approval_required_event(
                    approval_id,
                    command,
                    rationale,
                    reasons,
                    risk_level,
                    blast_radius,
                );
                None
            }
            ClientEvent::ApprovalResolved {
                approval_id,
                decision,
            } => {
                self.handle_approval_resolved_event(approval_id, decision);
                None
            }
            ClientEvent::TaskApprovalRules(rules) => {
                self.handle_task_approval_rules_event(rules);
                None
            }
            ClientEvent::ThreadList(threads) => {
                self.handle_thread_list_event(threads);
                None
            }
            ClientEvent::ThreadDetail(Some(thread)) => {
                if !self.deleted_thread_ids.contains(&thread.id) {
                    self.missing_runtime_thread_ids.remove(&thread.id);
                    self.handle_thread_detail_event(thread);
                }
                None
            }
            ClientEvent::ThreadDetail(None) => {
                if let Some(thread_id) = self.thread_loading_id.clone() {
                    if self.chat.active_thread_id() == Some(thread_id.as_str()) {
                        self.missing_runtime_thread_ids.insert(thread_id.clone());
                        self.finish_thread_loading(&thread_id);
                        self.send_daemon_command(DaemonCommand::Refresh);
                        self.status_line =
                            "Thread is not loaded yet; refreshing runtime context".to_string();
                    }
                }
                let _ = self.fallback_pending_reconnect_restore();
                None
            }
            ClientEvent::ThreadCreated {
                thread_id,
                title,
                agent_name,
            } => {
                self.handle_thread_created_event(thread_id, title, agent_name);
                None
            }
            ClientEvent::ThreadTitleUpdated { thread_id, title } => {
                if !title.trim().is_empty() {
                    self.chat
                        .reduce(chat::ChatAction::ThreadTitleUpdated { thread_id, title });
                    self.sync_open_thread_picker();
                }
                None
            }
            ClientEvent::ThreadDeleted { thread_id, deleted } => {
                if deleted {
                    self.deleted_thread_ids.insert(thread_id.clone());
                    self.pending_local_message_delete_reload_suppression
                        .remove(&thread_id);
                    self.pending_local_message_delete_backfills
                        .remove(&thread_id);
                    self.pending_local_message_delete_fetches.remove(&thread_id);
                    self.chat.reduce(chat::ChatAction::ThreadDeleted {
                        thread_id: thread_id.clone(),
                    });
                    self.sync_open_thread_picker();
                    self.send_daemon_command(DaemonCommand::Refresh);
                    self.status_line = "Thread deleted".to_string();
                } else {
                    self.deleted_thread_ids.remove(&thread_id);
                    self.status_line = "Thread delete failed".to_string();
                }
                None
            }
            ClientEvent::ThreadMessagePinResult(result) => {
                self.handle_thread_message_pin_result_event(result);
                None
            }
            ClientEvent::ThreadHandoffResult(result) => {
                self.handle_thread_handoff_result(result);
                None
            }
            ClientEvent::GatewayChats(chats) => {
                self.handle_gateway_chats(chats);
                None
            }
            ClientEvent::ThreadReloadRequired { thread_id } => {
                if !self.deleted_thread_ids.contains(&thread_id) {
                    self.handle_thread_reload_required_event(thread_id);
                }
                None
            }
            ClientEvent::ThreadExported {
                thread_id,
                file_path,
            } => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_millis().min(u128::from(u64::MAX)) as u64)
                    .unwrap_or(0);
                let message = chat::AgentMessage {
                    role: chat::MessageRole::System,
                    content: format!("Thread exported to {file_path}"),
                    message_kind: "system".to_string(),
                    timestamp,
                    ..Default::default()
                };
                self.chat
                    .reduce(chat::ChatAction::AppendMessage { thread_id, message });
                self.status_line = format!("Thread exported to {file_path}");
                None
            }
            ClientEvent::MessageFeedbackUpdated {
                thread_id,
                message_id,
                reaction,
            } => {
                if !self
                    .chat
                    .set_message_feedback(&thread_id, &message_id, reaction)
                {
                    self.request_latest_thread_page(thread_id, false);
                }
                None
            }
            ClientEvent::ContextWindowUpdate {
                thread_id,
                active_context_window_start,
                active_context_window_end,
                active_context_window_tokens,
            } => {
                if !self.deleted_thread_ids.contains(&thread_id) {
                    self.chat.reduce(chat::ChatAction::ContextWindowUpdated {
                        thread_id,
                        active_context_window_start,
                        active_context_window_end,
                        active_context_window_tokens,
                    });
                }
                None
            }
            ClientEvent::PromptQueue { thread_id, prompts } => {
                self.apply_daemon_prompt_queue(thread_id, prompts);
                None
            }
            ClientEvent::ParticipantSuggestion {
                thread_id,
                suggestion,
            } => {
                if suggestion
                    .suggestion_kind
                    .eq_ignore_ascii_case("auto_response")
                {
                    if self.chat.active_thread_id() == Some(thread_id.as_str())
                        && self.active_always_auto_response_participant().is_some_and(
                            |participant| {
                                participant
                                    .agent_id
                                    .eq_ignore_ascii_case(&suggestion.target_agent_id)
                            },
                        )
                        && !self.assistant_busy()
                    {
                        self.hidden_auto_response_suggestion_ids
                            .insert(suggestion.id.clone());
                        self.send_daemon_command(DaemonCommand::SendParticipantSuggestion {
                            thread_id,
                            suggestion_id: suggestion.id,
                        });
                    }
                } else {
                    self.queue_participant_suggestion(
                        thread_id,
                        suggestion.id,
                        suggestion.target_agent_id,
                        suggestion.target_agent_name,
                        suggestion.instruction,
                        suggestion.force_send,
                    );
                }
                None
            }
            other => Some(other),
        }
    }
}
