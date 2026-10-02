use super::super::super::*;
use super::super::events_activity::{
    auto_compaction_reload_window, compaction_artifact_from_notice, compaction_token_snapshot,
    normalized_skill_workflow_notice, parse_collaboration_sessions,
};
use super::super::*;

impl TuiModel {
    pub(in crate::app) fn handle_operator_model_reset_event(&mut self, ok: bool) {
        if ok {
            self.last_error = None;
            self.error_active = false;
            self.status_line = "Operator model reset".to_string();
        } else {
            self.last_error = Some("Operator model reset failed".to_string());
            self.error_active = true;
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
            self.status_line = "Operator model reset failed".to_string();
        }
    }

    pub(in crate::app) fn handle_collaboration_sessions_event(&mut self, sessions_json: String) {
        let sessions = serde_json::from_str::<serde_json::Value>(&sessions_json)
            .ok()
            .and_then(parse_collaboration_sessions)
            .unwrap_or_default();
        let escalation_notice = sessions.iter().find_map(|session| {
            session
                .escalation
                .as_ref()
                .map(|escalation| escalation.reason.clone())
        });
        self.main_pane_view = MainPaneView::Collaboration;
        self.collaboration
            .reduce(CollaborationAction::SessionsLoaded(sessions));
        if self
            .collaboration
            .rows()
            .get(1)
            .and_then(CollaborationRowVm::disagreement_id)
            .is_some()
        {
            self.collaboration.reduce(CollaborationAction::SelectRow(1));
        }
        self.last_error = None;
        self.error_active = false;
        self.status_line = "Collaboration sessions loaded".to_string();
        if let Some(reason) = escalation_notice {
            self.show_input_notice(
                format!("Collaboration escalation: {reason}"),
                InputNoticeKind::Warning,
                120,
                true,
            );
        }
    }

    pub(in crate::app) fn handle_collaboration_vote_result_event(&mut self, report_json: String) {
        let resolution = serde_json::from_str::<serde_json::Value>(&report_json)
            .ok()
            .and_then(|value| {
                value
                    .get("resolution")
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| "updated".to_string());
        self.status_line = format!("Vote recorded: {resolution}.");
        self.send_daemon_command(DaemonCommand::GetCollaborationSessions);
    }

    pub(in crate::app) fn handle_generated_tools_event(&mut self, tools_json: String) {
        let pretty = serde_json::from_str::<serde_json::Value>(&tools_json)
            .ok()
            .and_then(|value| serde_json::to_string_pretty(&value).ok())
            .unwrap_or(tools_json);
        self.last_error = Some(pretty);
        self.error_active = true;
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
        self.status_line = "Generated tools loaded".to_string();
    }

    pub(in crate::app) fn handle_operator_profile_session_completed_event(
        &mut self,
        session_id: String,
        updated_fields: Vec<String>,
    ) {
        self.operator_profile.loading = false;
        self.operator_profile.question = None;
        self.operator_profile.bool_answer = None;
        self.operator_profile.warning = None;
        self.operator_profile.visible = false;
        self.close_operator_profile_onboarding_modal();
        self.operator_profile.session_id = Some(session_id);
        self.operator_profile.progress = Some(super::OperatorProfileProgressVm {
            answered: updated_fields.len() as u32,
            remaining: 0,
            completion_ratio: 1.0,
        });
        self.input.reduce(input::InputAction::Clear);
        self.status_line = "Operator profile onboarding complete".to_string();
        self.show_input_notice(
            "Operator profile updated",
            InputNoticeKind::Success,
            120,
            true,
        );
        if !self.concierge.has_active_welcome() {
            self.request_concierge_welcome();
        }
    }

    pub(in crate::app) fn handle_error_event(&mut self, message: String) {
        if self.status_modal_loading {
            self.status_modal_loading = false;
            self.status_modal_error = Some(message.clone());
        }
        if self.prompt_modal_loading {
            self.prompt_modal_loading = false;
            self.prompt_modal_error = Some(message.clone());
            self.prompt_modal_scroll = 0;
        }
        if self.statistics_modal_loading {
            self.statistics_modal_loading = false;
            self.statistics_modal_error = Some(message.clone());
            self.statistics_modal_scroll = 0;
        }
        let should_refresh_subagents = {
            let lowercase = message.to_ascii_lowercase();
            lowercase.contains("sub-agent")
                || lowercase.contains("subagent")
                || lowercase.contains("protected mutation")
                || lowercase.contains("reserved built-in")
                || lowercase.contains("weles")
                || lowercase.contains("invalid target agent provider/model selection")
        };
        if message.contains("Invalid target agent provider/model selection") {
            self.pending_new_thread_execution_profile = None;
        }
        let busy = self.assistant_busy();
        if busy {
            self.chat.reduce(chat::ChatAction::ForceStopStreaming);
        }
        self.bootstrap_pending_activity_threads.clear();
        self.pending_prompt_response_threads.clear();
        self.clear_active_thread_activity();
        self.clear_pending_stop();
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
        self.last_error = Some(message.clone());
        self.error_active = true;
        self.error_tick = self.tick_counter;
        if busy && self.modal.top().is_none() {
            if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                let active_thread_id = thread_id.clone();
                self.reduce_chat_for_thread(
                    Some(active_thread_id.as_str()),
                    chat::ChatAction::AppendMessage {
                        thread_id,
                        message: chat::AgentMessage {
                            role: chat::MessageRole::System,
                            content: format!("Error: {}", message),
                            ..Default::default()
                        },
                    },
                );
            }
        } else {
            self.status_line = "Error recorded. Press Ctrl+E for details".to_string();
        }
        if should_refresh_subagents {
            self.send_daemon_command(DaemonCommand::ListSubAgents);
        }
    }

    pub(in crate::app) fn handle_thread_message_pin_result_event(
        &mut self,
        result: crate::client::ThreadMessagePinResultVm,
    ) {
        if result.ok {
            return;
        }

        if result.error.as_deref() == Some("pinned_budget_exceeded") {
            if let Some(candidate_pinned_chars) = result.candidate_pinned_chars {
                self.open_pinned_budget_exceeded_modal(PendingPinnedBudgetExceeded {
                    current_pinned_chars: result.current_pinned_chars,
                    pinned_budget_chars: result.pinned_budget_chars,
                    candidate_pinned_chars,
                });
                return;
            }
        }

        self.status_line = format!(
            "Pin failed: {}",
            result.error.unwrap_or_else(|| "unknown_error".to_string())
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn handle_retry_status_event(
        &mut self,
        thread_id: String,
        phase: String,
        attempt: u32,
        max_retries: u32,
        delay_ms: u64,
        failure_class: String,
        message: String,
    ) {
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.clear_pending_prompt_response_thread(thread_id.as_str());
        if phase == "cleared" {
            self.reduce_chat_for_thread(
                Some(thread_id.as_str()),
                chat::ChatAction::ClearRetryStatus {
                    thread_id: thread_id.clone(),
                },
            );
            self.retry_wait_start_selected = false;
            self.clear_agent_activity_for(Some(thread_id.as_str()));
            return;
        }
        if phase != "waiting" && !self.should_accept_retry_status_event(thread_id.as_str()) {
            return;
        }
        self.retry_wait_start_selected = false;
        self.set_agent_activity_for(
            Some(thread_id.clone()),
            match phase.as_str() {
                "waiting" => "retry wait",
                _ => "retrying",
            },
        );
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::SetRetryStatus {
                thread_id,
                phase: if phase == "waiting" {
                    chat::RetryPhase::Waiting
                } else {
                    chat::RetryPhase::Retrying
                },
                attempt,
                max_retries,
                delay_ms,
                failure_class,
                message,
                received_at_tick: self.tick_counter,
            },
        );
    }

    fn should_apply_workflow_agent_activity(&self, thread_id: Option<&str>) -> bool {
        let Some(thread_id) = thread_id else {
            return self.chat.is_streaming() || self.current_thread_agent_activity().is_some();
        };

        if self.chat.is_thread_streaming(thread_id)
            || self.thread_agent_activity.contains_key(thread_id)
            || self.pending_prompt_response_threads.contains(thread_id)
            || self.bootstrap_pending_activity_threads.contains(thread_id)
        {
            return true;
        }

        let latest_role = self
            .chat
            .threads()
            .iter()
            .find(|thread| thread.id == thread_id)
            .and_then(|thread| {
                thread
                    .messages
                    .iter()
                    .rev()
                    .find(|message| {
                        !message.content.trim().is_empty()
                            && !matches!(message.role, chat::MessageRole::System)
                    })
                    .map(|message| message.role)
            });

        !matches!(
            latest_role,
            Some(chat::MessageRole::Assistant | chat::MessageRole::Tool)
        )
    }

    pub(in crate::app) fn handle_workflow_notice_event(
        &mut self,
        thread_id: Option<String>,
        kind: String,
        message: String,
        details: Option<String>,
    ) {
        let details_ref = details.as_deref();
        if kind == "transport-fallback" {
            if let Some(details) = details_ref {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(details) {
                    if let Some(to) = parsed.get("to").and_then(|value| value.as_str()) {
                        self.config.api_transport = to.to_string();
                    }
                }
            }
        }
        if let Some((_normalized_kind, status_line, agent_activity)) =
            normalized_skill_workflow_notice(&kind, &message, details_ref)
        {
            self.status_line = status_line;
            if let Some(agent_activity) = agent_activity {
                if self.should_apply_workflow_agent_activity(thread_id.as_deref()) {
                    self.set_agent_activity_for(thread_id.clone(), agent_activity);
                }
            }
        } else {
            self.status_line = if let Some(details) = details_ref {
                format!("{message} ({details})")
            } else {
                message.clone()
            };
        }
        if kind == "auto-compaction" || kind == "manual-compaction" {
            if let (
                Some(thread_id),
                Some(active_thread_id),
                Some((post_compaction_tail_len, _legacy_offset, split_at, total_message_count)),
            ) = (
                thread_id.as_deref(),
                self.chat.active_thread_id(),
                auto_compaction_reload_window(details_ref),
            ) {
                if thread_id == active_thread_id {
                    self.chat.reduce(chat::ChatAction::CompactionApplied {
                        thread_id: thread_id.to_string(),
                        active_compaction_window_start: split_at,
                        total_message_count,
                    });
                    if let Some(snapshot) = compaction_token_snapshot(details_ref) {
                        // Surface the post-compaction token count immediately so
                        // the header drops from its (often clamped-to-100%)
                        // pre-compaction value rather than waiting for the
                        // separate ContextWindowUpdate event.
                        self.chat.reduce(chat::ChatAction::ContextWindowUpdated {
                            thread_id: thread_id.to_string(),
                            active_context_window_start: snapshot.post_compaction_window_start,
                            active_context_window_end: snapshot.post_compaction_window_end,
                            active_context_window_tokens: snapshot.post_compaction_total_tokens,
                        });
                    }
                    let span_start = self
                        .chat
                        .threads()
                        .iter()
                        .find(|t| t.id == thread_id)
                        .filter(|t| t.loaded_message_end > t.loaded_message_start)
                        .map(|t| t.loaded_message_start.min(split_at))
                        .unwrap_or(split_at);
                    let message_limit = total_message_count
                        .saturating_sub(span_start)
                        .max(post_compaction_tail_len);
                    self.request_thread_page(thread_id.to_string(), message_limit, 0, false);
                }
            }
            if let (Some(thread_id), Some(artifact)) = (
                thread_id.as_deref(),
                compaction_artifact_from_notice(details_ref),
            ) {
                let already_present = self.chat.threads().iter().any(|thread| {
                    thread.id == thread_id
                        && thread.messages.iter().any(|message| {
                            message.id.as_deref() == Some(artifact.id.as_str())
                                || (message.message_kind == "compaction_artifact"
                                    && message.content == artifact.content)
                        })
                });
                if !already_present
                    && self
                        .chat
                        .threads()
                        .iter()
                        .any(|thread| thread.id == thread_id)
                {
                    self.chat.reduce(chat::ChatAction::AppendMessage {
                        thread_id: thread_id.to_string(),
                        message: chat::AgentMessage {
                            id: Some(artifact.id),
                            role: chat::MessageRole::Assistant,
                            content: artifact.content,
                            message_kind: "compaction_artifact".to_string(),
                            compaction_strategy: artifact.strategy,
                            compaction_payload: artifact.payload,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|duration| duration.as_millis() as u64)
                                .unwrap_or(0),
                            ..Default::default()
                        },
                    });
                }
            }
        }
        if kind == "operator-profile-warning" {
            let warning = if let Some(details) = details_ref {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(details) {
                    parsed
                        .get("error")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| message.clone())
                } else {
                    details.to_string()
                }
            } else {
                message.clone()
            };
            self.operator_profile.warning = Some(warning);
            self.operator_profile.loading = false;
            self.open_operator_profile_onboarding_modal();
            self.show_input_notice(
                "operator profile warning (Ctrl+R to retry)",
                InputNoticeKind::Warning,
                120,
                false,
            );
        }
    }

    pub(in crate::app) fn handle_weles_health_update_event(
        &mut self,
        state: String,
        reason: Option<String>,
        checked_at: u64,
    ) {
        let degraded = state.eq_ignore_ascii_case("degraded");
        self.weles_health = Some(crate::client::WelesHealthVm {
            state,
            reason: reason.clone(),
            checked_at,
        });
        if degraded {
            let detail = reason
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "daemon vitality checks require attention".to_string());
            self.status_line = format!("WELES degraded: {detail}");
            let thread_id = self
                .chat
                .active_thread_id()
                .map(str::to_string)
                .unwrap_or_else(|| "local-weles-health".to_string());
            let active_thread_id = thread_id.clone();
            self.reduce_chat_for_thread(
                Some(active_thread_id.as_str()),
                chat::ChatAction::AppendMessage {
                    thread_id,
                    message: chat::AgentMessage {
                        role: chat::MessageRole::System,
                        content: format!("WELES degraded\n\n{detail}"),
                        ..Default::default()
                    },
                },
            );
        }
    }

    pub(in crate::app) fn handle_status_diagnostics_event(
        &mut self,
        operator_profile_sync_state: String,
        operator_profile_sync_dirty: bool,
        operator_profile_scheduler_fallback: bool,
        diagnostics_json: String,
    ) {
        let parsed = serde_json::from_str::<serde_json::Value>(&diagnostics_json).ok();
        self.status_modal_diagnostics_json = Some(diagnostics_json);
        if operator_profile_sync_dirty {
            self.status_line = format!(
                "Operator profile sync state: {} (retry with Ctrl+R)",
                operator_profile_sync_state
            );
            self.show_input_notice(
                format!(
                    "operator profile sync={} (Ctrl+R to retry)",
                    operator_profile_sync_state
                ),
                InputNoticeKind::Warning,
                120,
                false,
            );
        } else if operator_profile_scheduler_fallback {
            self.status_line =
                "Operator profile scheduler fallback active (contextual-only)".to_string();
            self.show_input_notice(
                "operator profile scheduler fallback active",
                InputNoticeKind::Warning,
                120,
                false,
            );
        } else if let Some(mesh_state) = parsed
            .as_ref()
            .and_then(|value| value.get("skill_mesh"))
            .and_then(|value| value.get("state"))
            .and_then(|value| value.as_str())
            .filter(|state| *state != "fresh" && *state != "legacy")
        {
            self.status_line = format!("skill mesh: {mesh_state}");
        }
    }

    pub(in crate::app) fn handle_agent_explanation_event(&mut self, payload: serde_json::Value) {
        let thread_id = self
            .chat
            .active_thread_id()
            .map(str::to_string)
            .unwrap_or_else(|| "local-explain".to_string());
        let content =
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::AppendMessage {
                thread_id,
                message: chat::AgentMessage {
                    role: chat::MessageRole::System,
                    content: format!("Explainability\n\n{}", content),
                    ..Default::default()
                },
            },
        );
        self.status_line = "Explainability result received".to_string();
    }

    pub(in crate::app) fn handle_divergent_session_started_event(
        &mut self,
        payload: serde_json::Value,
    ) {
        let session_id = payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let thread_id = self
            .chat
            .active_thread_id()
            .map(str::to_string)
            .unwrap_or_else(|| "local-divergent".to_string());
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::AppendMessage {
                thread_id,
                message: chat::AgentMessage {
                    role: chat::MessageRole::System,
                    content: if session_id.is_empty() {
                        "Divergent session started".to_string()
                    } else {
                        format!(
                            "Divergent session started: `{}`\nUse `/diverge-get {}` to fetch results.",
                            session_id, session_id
                        )
                    },
                    ..Default::default()
                },
            },
        );
        self.status_line = "Divergent session started".to_string();
    }

    pub(in crate::app) fn handle_divergent_session_event(&mut self, payload: serde_json::Value) {
        let thread_id = self
            .chat
            .active_thread_id()
            .map(str::to_string)
            .unwrap_or_else(|| "local-divergent".to_string());
        let content =
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::AppendMessage {
                thread_id,
                message: chat::AgentMessage {
                    role: chat::MessageRole::System,
                    content: format!("Divergent session payload\n\n{}", content),
                    ..Default::default()
                },
            },
        );
        self.status_line = "Divergent session payload received".to_string();
    }
}
