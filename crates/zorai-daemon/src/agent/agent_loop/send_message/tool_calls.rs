use super::*;
use crate::agent::metacognitive::introspector::{
    introspect, IntrospectionInput, RecentToolOutcome,
};
use crate::agent::metacognitive::pattern_regulator::{regulate, InterventionAction};
use crate::agent::metacognitive::self_assessment::SelfAssessor;

pub(super) fn inter_tool_call_delay(
    index: usize,
    configured_delay_ms: u64,
) -> Option<std::time::Duration> {
    (index > 0 && configured_delay_ms > 0)
        .then(|| std::time::Duration::from_millis(configured_delay_ms))
}

fn skill_gate_exempt_tool(name: &str) -> bool {
    matches!(
        name,
        zorai_protocol::tool_names::DISCOVER_GUIDELINES
            | zorai_protocol::tool_names::LIST_GUIDELINES
            | zorai_protocol::tool_names::READ_GUIDELINE
            | zorai_protocol::tool_names::DISCOVER_SKILLS
            | zorai_protocol::tool_names::LIST_SKILLS
            | zorai_protocol::tool_names::READ_SKILL
            | zorai_protocol::tool_names::JUSTIFY_SKILL_SKIP
    )
}

fn should_suppress_busy_wait_poll(
    tool_name: &str,
    current_signature: &str,
    previous_signature: Option<&str>,
    consecutive_same_tool_calls: u32,
    has_active_subagents: bool,
    blocking_wait: bool,
) -> bool {
    previous_signature.is_some_and(|value| value == current_signature)
        && consecutive_same_tool_calls >= 1
        && match tool_name {
            zorai_protocol::tool_names::LIST_AGENTS
            | zorai_protocol::tool_names::LIST_SUBAGENTS => has_active_subagents,
            zorai_protocol::tool_names::GET_OPERATION_STATUS
            | zorai_protocol::tool_names::GET_BACKGROUND_TASK_STATUS => !blocking_wait,
            _ => false,
        }
}

fn tool_call_requests_blocking_wait(tc: &ToolCall) -> bool {
    serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
        .ok()
        .and_then(|value| value.get("wait").and_then(|wait| wait.as_bool()))
        .unwrap_or(false)
}

fn busy_wait_poll_suppression_message(tool_name: &str) -> (&'static str, String) {
    match tool_name {
        zorai_protocol::tool_names::LIST_AGENTS => (
            "Repeated list_agents polling suppressed while child subagents are still running.",
            "Repeated list_agents polling suppressed. `list_agents` does not report spawned child progress. Do not busy-wait while child subagents are active; continue unrelated work or send a normal response so zorai can resume you when children finish.".to_string(),
        ),
        zorai_protocol::tool_names::LIST_SUBAGENTS => (
            "Repeated list_subagents polling suppressed while child subagents are still running.",
            "Repeated list_subagents polling suppressed because the agent appears to be busy-waiting on active child subagents. Use list_subagents only for occasional snapshots, then continue other work or send a normal response so zorai can resume you when children finish.".to_string(),
        ),
        _ => (
            "Repeated get_operation_status polling suppressed.",
            "Repeated get_operation_status polling suppressed. Do not poll background operations. Call get_operation_status once with wait=true to block until completion, or continue other work so this thread can auto-resume when the operation finishes.".to_string(),
        ),
    }
}

fn allows_repeated_identical_tool_calls(tool_name: &str) -> bool {
    matches!(
        tool_name,
        zorai_protocol::tool_names::GET_OPERATION_STATUS
            | zorai_protocol::tool_names::GET_BACKGROUND_TASK_STATUS
    )
}

impl<'a> SendMessageRunner<'a> {
    async fn persist_metacognitive_intervention_result(&mut self, tc: &ToolCall, content: String) {
        let synthetic_result = ToolResult {
            tool_call_id: tc.id.clone(),
            name: tc.function.name.clone(),
            content: content.clone(),
            is_error: true,
            weles_review: tc.weles_review.clone(),
            pending_approval: None,
        };
        let current_tool_signature = normalized_tool_signature(tc);
        let repeated = self
            .previous_tool_signature
            .as_deref()
            .is_some_and(|value| value == current_tool_signature.as_str());

        if repeated {
            self.consecutive_same_tool_calls = self.consecutive_same_tool_calls.saturating_add(1);
        } else {
            self.consecutive_same_tool_calls = 1;
        }
        self.previous_tool_signature = Some(current_tool_signature);
        self.previous_tool_outcome = Some((tc.function.name.clone(), true));
        self.last_tool_error = Some((tc.function.name.clone(), content.clone()));
        let mut blocked_outcome_summary =
            summarize_tool_result_for_policy(&tc.function.name, &synthetic_result);
        blocked_outcome_summary.outcome = "blocked".to_string();
        self.recent_policy_tool_outcomes
            .push_back(blocked_outcome_summary);
        while self.recent_policy_tool_outcomes.len() > POLICY_TOOL_OUTCOME_HISTORY_LIMIT {
            self.recent_policy_tool_outcomes.pop_front();
        }
        self.record_tool_trace(tc, &synthetic_result);
        self.persist_denied_tool_result(tc, content).await;
    }

    async fn current_task_has_active_subagents(&self) -> bool {
        let Some(task_id) = self.task_id else {
            return false;
        };

        self.engine
            .count_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: Some("subagent".to_string()),
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: Some(task_id.to_string()),
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: true,
                order_by_recent_activity_desc: false,
                limit: Some(1),
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await
            > 0
    }

    async fn handle_metacognitive_intervention(
        &mut self,
        tc: &ToolCall,
    ) -> Option<LoopDisposition> {
        let decision_reasoning = {
            let threads = self.engine.threads.read().await;
            threads
                .get(&self.tid)
                .and_then(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .find(|message| message.role == MessageRole::Assistant)
                })
                .and_then(|message| message.reasoning.clone())
                .or_else(|| {
                    threads
                        .get(&self.tid)
                        .and_then(|thread| {
                            thread
                                .messages
                                .iter()
                                .rev()
                                .find(|message| message.role == MessageRole::Assistant)
                        })
                        .map(|message| message.content.trim().to_string())
                        .filter(|content| !content.is_empty())
                })
                .or_else(|| {
                    let trimmed = self.stored_user_content.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                })
        };
        let current_tool_signature = normalized_tool_signature(tc);
        let predicted_repeat_count = if self
            .previous_tool_signature
            .as_deref()
            .is_some_and(|value| value == current_tool_signature.as_str())
        {
            self.consecutive_same_tool_calls.saturating_add(1)
        } else {
            1
        };
        let recent_tool_outcomes = self
            .recent_policy_tool_outcomes
            .iter()
            .cloned()
            .map(|summary| RecentToolOutcome {
                tool_name: summary.tool_name,
                outcome: summary.outcome,
                summary: summary.summary,
            })
            .collect::<Vec<_>>();
        let task_retry_count = self
            .current_task_snapshot
            .as_ref()
            .map(|task| task.retry_count)
            .unwrap_or(0);
        let input = IntrospectionInput {
            proposed_tool_name: tc.function.name.clone(),
            proposed_tool_arguments: tc.function.arguments.clone(),
            normalized_tool_signature: current_tool_signature.clone(),
            predicted_repeat_count,
            recent_tool_outcomes,
            task_retry_count,
            decision_reasoning,
        };
        let self_model = self.engine.meta_cognitive_self_model.read().await.clone();
        let outcome = introspect(&self_model, &input);
        if outcome.signals.is_empty() {
            return None;
        }

        let decision = regulate(&outcome, tc);
        self.engine.emit_workflow_notice(
            &self.tid,
            "metacognitive-check",
            decision.summary.clone(),
            serde_json::to_string(&outcome).ok(),
        );
        if let Some(adjustment) = decision.confidence_adjustment {
            let recent_success_rate = if input.recent_tool_outcomes.is_empty() {
                1.0
            } else {
                input
                    .recent_tool_outcomes
                    .iter()
                    .filter(|outcome| outcome.outcome.eq_ignore_ascii_case("success"))
                    .count() as f64
                    / input.recent_tool_outcomes.len() as f64
            };
            let user_feedback_score = self
                .engine
                .history
                .aggregate_user_feedback_score(&self.tid)
                .await
                .ok()
                .flatten();
            let assessment = SelfAssessor::default().assess(
                &crate::agent::metacognitive::self_assessment::AssessmentInput {
                    progress: crate::agent::metacognitive::self_assessment::ProgressMetrics {
                        goal_distance_pct: 0.0,
                        steps_completed: 0,
                        steps_total: 0,
                        estimated_remaining: 0,
                        momentum: if recent_success_rate >= 0.5 {
                            0.1
                        } else {
                            -0.2
                        },
                    },
                    efficiency: crate::agent::metacognitive::self_assessment::EfficiencyMetrics {
                        token_efficiency: 0.5,
                        tool_success_rate: recent_success_rate,
                        time_efficiency: 0.0,
                        tokens_consumed: 0,
                        elapsed_secs: input.task_retry_count as u64,
                    },
                    quality: crate::agent::metacognitive::self_assessment::QualityMetrics {
                        error_rate: 1.0 - recent_success_rate,
                        revision_count: input.task_retry_count,
                        user_feedback_score,
                    },
                },
            );
            let predicted_band = metacognitive_calibrated_band(&assessment, 0.0);
            self.engine
                .apply_meta_cognitive_calibration_adjustment(adjustment, predicted_band)
                .await;
        }
        for signal in &outcome.signals {
            self.engine
                .reinforce_meta_cognitive_bias_occurrence(&signal.bias_name)
                .await;
        }

        match decision.action {
            InterventionAction::Allow => None,
            InterventionAction::Warn => {
                if let Some(system_message) = decision.system_message.as_deref() {
                    self.engine
                        .append_metacognitive_system_message(&self.tid, system_message)
                        .await;
                }
                None
            }
            InterventionAction::Block => {
                if let Some(system_message) = decision.system_message.as_deref() {
                    self.engine
                        .append_metacognitive_system_message(&self.tid, system_message)
                        .await;
                }
                let denied_content = format!(
                    "Tool call blocked by meta-cognitive regulator before execution. {}",
                    decision.summary
                );
                self.persist_metacognitive_intervention_result(tc, denied_content)
                    .await;
                self.engine.persist_thread_by_id(&self.tid).await;
                Some(LoopDisposition::Continue)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_tool_calls_chunk(
        &mut self,
        _llm_started_at: Instant,
        _first_token_at: Option<Instant>,
        effective_transport_for_turn: ApiTransport,
        accumulated_content: String,
        accumulated_reasoning: String,
        tool_calls: Vec<ToolCall>,
        content: Option<String>,
        reasoning: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        response_id: Option<String>,
        upstream_message: Option<CompletionUpstreamMessage>,
        provider_final_result: Option<CompletionProviderFinalResult>,
        upstream_thread_id: Option<String>,
    ) -> Result<LoopDisposition> {
        self.emit_tool_ack_if_needed(&tool_calls, &accumulated_content);

        let msg_content = content.unwrap_or(accumulated_content);
        let msg_reasoning = reasoning.or(if accumulated_reasoning.is_empty() {
            None
        } else {
            Some(accumulated_reasoning)
        });
        let decision_reasoning = msg_reasoning
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| (!msg_content.trim().is_empty()).then_some(msg_content.clone()));

        let turn_input_tokens = input_tokens.unwrap_or(0);
        let turn_output_tokens = output_tokens.unwrap_or(0);
        self.persist_assistant_tool_calls_message(
            &tool_calls,
            msg_content,
            msg_reasoning,
            input_tokens,
            output_tokens,
            None,
            response_id,
            upstream_message,
            provider_final_result.clone(),
            effective_transport_for_turn,
        )
        .await;
        self.engine
            .accumulate_goal_run_cost(
                &self.tid,
                input_tokens.unwrap_or(0),
                output_tokens.unwrap_or(0),
                &self.config.provider,
                &self.provider_config.model,
                None,
            )
            .await;
        self.engine.persist_thread_by_id(&self.tid).await;
        self.emit_context_window_update_for_current_thread().await;
        Box::pin(
            self.engine
                .maybe_auto_send_gateway_thread_response(&self.tid),
        )
        .await;
        self.engine
            .update_thread_upstream_state(
                &self.tid,
                &self.config.provider,
                &self.provider_config.model,
                effective_transport_for_turn,
                Some(self.provider_config.assistant_id.as_str()),
                upstream_thread_id,
            )
            .await;
        self.provider_final_result = provider_final_result;

        for (index, tc) in tool_calls.iter().enumerate() {
            if self.stream_cancel_token.is_cancelled() {
                self.was_cancelled = true;
                break;
            }

            if let Some(delay) = inter_tool_call_delay(index, self.config.tool_call_delay_ms) {
                tokio::time::sleep(delay).await;
            }

            let args_summary = tool_args_summary(&tc.function.arguments);
            let args_hash = super::episodic::counter_who::compute_approach_hash(
                &tc.function.name,
                &args_summary,
            );
            let now_epoch_secs = current_epoch_secs();

            if let Some(task) = self.current_task_snapshot.as_ref() {
                let scope = policy_scope_for_task(&self.tid, task);
                if let super::orchestrator_policy::PolicyLoopAction::AbortRetry = self
                    .engine
                    .enforce_orchestrator_retry_guard(
                        &self.tid,
                        Some(task.id.as_str()),
                        &scope,
                        &args_hash,
                        now_epoch_secs,
                    )
                    .await?
                {
                    self.policy_aborted_retry = true;
                    return Ok(LoopDisposition::Break);
                }
            }

            let _ = self.engine.event_tx.send(AgentEvent::ToolCall {
                thread_id: self.tid.clone(),
                call_id: tc.id.clone(),
                name: tc.function.name.clone(),
                arguments: tc.function.arguments.clone(),
                weles_review: tc.weles_review.clone(),
                message_id: None,
                turn_input_tokens: if index == 0 {
                    Some(turn_input_tokens)
                } else {
                    None
                },
                turn_output_tokens: if index == 0 {
                    Some(turn_output_tokens)
                } else {
                    None
                },
                turn_cost_usd: None,
            });

            if self.handle_tool_filter_denial(tc).await {
                continue;
            }
            if self.handle_skill_gate_denial(tc).await {
                continue;
            }
            if let Some(disposition) = self.handle_metacognitive_intervention(tc).await {
                return Ok(disposition);
            }

            let tool_file_snapshots = self
                .engine
                .snapshot_tool_file_context(
                    tc.function.name.as_str(),
                    tc.function.arguments.as_str(),
                    self.preferred_session_id,
                )
                .await;
            let result = self.execute_tool_call(tc).await;
            self.record_tool_trace(tc, &result);

            if tc.function.name == zorai_protocol::tool_names::UPDATE_MEMORY && !result.is_error {
                self.engine.refresh_memory_cache().await;
            }

            if let Some((previous_tool_name, previous_was_error)) =
                self.previous_tool_outcome.as_ref()
            {
                if let Err(error) = self
                    .engine
                    .record_tool_hesitation(
                        previous_tool_name,
                        tc.function.name.as_str(),
                        *previous_was_error,
                        result.is_error,
                    )
                    .await
                {
                    tracing::warn!(error = %error, "failed to record implicit tool fallback feedback");
                }
            }
            self.previous_tool_outcome = Some((tc.function.name.clone(), result.is_error));
            if result.is_error {
                self.last_tool_error = Some((tc.function.name.clone(), result.content.clone()));
            } else {
                self.last_tool_error = None;
            }

            self.recent_policy_tool_outcomes
                .push_back(summarize_tool_result_for_policy(&tc.function.name, &result));
            while self.recent_policy_tool_outcomes.len() > POLICY_TOOL_OUTCOME_HISTORY_LIMIT {
                self.recent_policy_tool_outcomes.pop_front();
            }

            if !result.is_error {
                self.engine
                    .capture_tool_work_context(
                        &self.tid,
                        self.task_id,
                        tc.function.name.as_str(),
                        tc.id.as_str(),
                        tc.function.arguments.as_str(),
                        &tool_file_snapshots,
                    )
                    .await;
            }

            match self
                .finalize_tool_call_result(
                    tc,
                    &result,
                    &args_summary,
                    &args_hash,
                    now_epoch_secs,
                    decision_reasoning.as_deref(),
                )
                .await?
            {
                ToolCallDisposition::ContinueTools => {}
                ToolCallDisposition::RestartLoop => return Ok(LoopDisposition::Continue),
                ToolCallDisposition::BreakLoop => return Ok(LoopDisposition::Break),
            }

            if !result.is_error {
                if let Some(disposition) = self.apply_successful_subagent_budget_tool(
                    tc.function.name.as_str(),
                    tc.function.arguments.as_str(),
                    result.content.as_str(),
                ) {
                    return Ok(match disposition {
                        ToolCallDisposition::ContinueTools => LoopDisposition::Continue,
                        ToolCallDisposition::RestartLoop => LoopDisposition::Continue,
                        ToolCallDisposition::BreakLoop => LoopDisposition::Break,
                    });
                }
            }
        }

        if self.was_cancelled {
            return Ok(LoopDisposition::Break);
        }

        if self.should_break_for_termination().await {
            return Ok(LoopDisposition::Break);
        }

        let executed_sequence = self
            .recent_policy_tool_outcomes
            .iter()
            .map(|summary| summary.tool_name.clone())
            .collect::<Vec<_>>();
        let completed = !executed_sequence.is_empty()
            && self
                .recent_policy_tool_outcomes
                .iter()
                .all(|summary| summary.outcome.eq_ignore_ascii_case("success"));
        if !executed_sequence.is_empty() {
            self.engine
                .record_meta_cognitive_workflow_profile(&executed_sequence, completed)
                .await;
        }

        Ok(LoopDisposition::Continue)
    }

    fn emit_tool_ack_if_needed(&mut self, tool_calls: &[ToolCall], accumulated_content: &str) {
        if self.tool_ack_emitted || !accumulated_content.trim().is_empty() {
            return;
        }
        self.tool_ack_emitted = true;
        let tool_names: Vec<&str> = tool_calls
            .iter()
            .map(|tool_call| tool_call.function.name.as_str())
            .collect();
        let ack = match tool_names.as_slice() {
            [single] => format!("On it - using {single}..."),
            names if names.len() <= 3 => {
                format!("Working on it - using {}...", names.join(", "))
            }
            names => format!("Working on it - running {} tools...", names.len()),
        };
        let _ = self.engine.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: self.tid.clone(),
            kind: "tool-ack".to_string(),
            message: ack,
            details: None,
        });
    }

    async fn persist_assistant_tool_calls_message(
        &mut self,
        tool_calls: &[ToolCall],
        msg_content: String,
        msg_reasoning: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        turn_cost: Option<f64>,
        response_id: Option<String>,
        upstream_message: Option<CompletionUpstreamMessage>,
        provider_final_result: Option<CompletionProviderFinalResult>,
        effective_transport_for_turn: ApiTransport,
    ) {
        let (author_agent_id, author_agent_name) = self
            .engine
            .assistant_author_identity_for_thread(&self.tid)
            .await;
        let mut threads = self.engine.threads.write().await;
        if let Some(thread) = threads.get_mut(&self.tid) {
            self.assistant_output_visible = true;
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: msg_content,
                content_blocks: Vec::new(),
                tool_calls: Some(tool_calls.to_vec()),
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: input_tokens.unwrap_or(0),
                output_tokens: output_tokens.unwrap_or(0),
                cost: turn_cost,
                provider: Some(self.config.provider.clone()),
                model: Some(self.provider_config.model.clone()),
                api_transport: Some(effective_transport_for_turn),
                response_id,
                upstream_message,
                provider_final_result,
                author_agent_id: Some(author_agent_id),
                author_agent_name: Some(author_agent_name),
                reasoning: msg_reasoning,
                message_kind: if self.report_back_phase.is_some() {
                    AgentMessageKind::ReportBack
                } else {
                    AgentMessageKind::Normal
                },
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: now_millis(),
                feedback: None,
            });
            thread.total_input_tokens += input_tokens.unwrap_or(0);
            thread.total_output_tokens += output_tokens.unwrap_or(0);
        }
    }

    async fn persist_denied_tool_result(&mut self, tc: &ToolCall, content: String) {
        let persisted_message_id = generate_message_id();
        let _ = self.engine.event_tx.send(AgentEvent::ToolResult {
            thread_id: self.tid.clone(),
            call_id: tc.id.clone(),
            name: tc.function.name.clone(),
            content: content.clone(),
            is_error: true,
            weles_review: tc.weles_review.clone(),
            message_id: Some(persisted_message_id.clone()),
        });
        let mut threads = self.engine.threads.write().await;
        if let Some(thread) = threads.get_mut(&self.tid) {
            self.tool_side_effect_committed = true;
            thread.messages.push(AgentMessage {
                id: persisted_message_id,
                role: MessageRole::Tool,
                content,
                content_blocks: Vec::new(),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
                tool_name: Some(tc.function.name.clone()),
                tool_arguments: Some(tc.function.arguments.clone()),
                tool_status: Some("error".to_string()),
                weles_review: tc.weles_review.clone(),
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
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: now_millis(),
                feedback: None,
            });
        }
    }

    async fn handle_tool_filter_denial(&mut self, tc: &ToolCall) -> bool {
        let Some(filter) = self.task_tool_filter.as_ref() else {
            return false;
        };
        let Some(reason) = filter.deny_reason(&tc.function.name) else {
            return false;
        };

        let denied_content = format!("Tool call denied: {reason}");
        self.persist_denied_tool_result(tc, denied_content).await;
        true
    }

    async fn handle_skill_gate_denial(&mut self, tc: &ToolCall) -> bool {
        if skill_gate_exempt_tool(&tc.function.name) {
            return false;
        }
        let Some(state) = self
            .engine
            .get_thread_skill_discovery_state(&self.tid)
            .await
        else {
            return false;
        };
        if state.compliant {
            return false;
        }

        if state.is_discovery_pending() {
            self.engine.emit_workflow_notice(
                &self.tid,
                "skill-gate",
                format!(
                    "Skill discovery is still running asynchronously before `{}`; allowing the tool call to proceed for now.",
                    tc.function.name
                ),
                serde_json::to_string(&state).ok(),
            );
            return false;
        }

        if state.mesh_requires_approval {
            let denied_content = format!(
                "Tool call blocked by skill discovery governance. Before `{}` you must obtain approval for `{}`.",
                tc.function.name, state.recommended_action
            );
            self.engine.emit_workflow_notice(
                &self.tid,
                "skill-gate",
                format!(
                    "Skill discovery requires approval before `{}`. Required next step: {}.",
                    tc.function.name, state.recommended_action
                ),
                serde_json::to_string(&state).ok(),
            );
            self.persist_denied_tool_result(tc, denied_content).await;
            return true;
        }

        if state.requires_skill_read_before_progress() {
            self.engine.emit_workflow_notice(
                &self.tid,
                "skill-gate",
                format!(
                    "Skill discovery strongly recommends `{}` before `{}`; allowing the tool call to proceed.",
                    state.recommended_action,
                    tc.function.name
                ),
                serde_json::to_string(&state).ok(),
            );
            return false;
        }

        self.engine.emit_workflow_notice(
            &self.tid,
            "skill-gate",
            if state.has_advisory_skill_read() {
                format!(
                    "Weak skill discovery recommends `{}` before `{}`; allowing the tool call to proceed.",
                    state.recommended_action, tc.function.name
                )
            } else {
                format!(
                    "Skill discovery suggests `{}` before `{}`; allowing the tool call to proceed.",
                    state.recommended_action, tc.function.name
                )
            },
            serde_json::to_string(&state).ok(),
        );
        false
    }

    async fn execute_tool_call(&mut self, tc: &ToolCall) -> ToolResult {
        if tc.function.name == zorai_protocol::tool_names::LOAD_TOOLS {
            return self.handle_load_tools(tc);
        }
        self.activate_deferred_tool(&tc.function.name);
        let current_tool_signature = normalized_tool_signature(tc);
        let has_active_subagents = self.current_task_has_active_subagents().await;
        if should_suppress_busy_wait_poll(
            &tc.function.name,
            &current_tool_signature,
            self.previous_tool_signature.as_deref(),
            self.consecutive_same_tool_calls,
            has_active_subagents,
            tool_call_requests_blocking_wait(tc),
        ) {
            let (summary, content) = busy_wait_poll_suppression_message(&tc.function.name);
            self.engine.emit_workflow_notice(
                &self.tid,
                "tool-stall",
                summary,
                Some(format!(
                    "tool={} signature={}",
                    tc.function.name, current_tool_signature
                )),
            );
            self.previous_tool_signature = Some(current_tool_signature);
            return ToolResult {
                tool_call_id: tc.id.clone(),
                name: tc.function.name.clone(),
                content,
                is_error: true,
                weles_review: tc.weles_review.clone(),
                pending_approval: None,
            };
        }
        let repeated = self
            .previous_tool_signature
            .as_deref()
            .is_some_and(|value| value == current_tool_signature.as_str());
        let result = if repeated {
            self.consecutive_same_tool_calls = self.consecutive_same_tool_calls.saturating_add(1);
            if self.consecutive_same_tool_calls >= 3
                && !allows_repeated_identical_tool_calls(&tc.function.name)
            {
                self.engine.emit_workflow_notice(
                    &self.tid,
                    "tool-stall",
                    "Repeated identical tool call suppressed; inspect fresh state or choose a different action.",
                    Some(format!(
                        "tool={} signature={}",
                        tc.function.name, current_tool_signature
                    )),
                );
                ToolResult {
                    tool_call_id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    content: "Repeated identical tool call suppressed because the agent appears stuck. Inspect current state or continue with a different action instead of repeating the same tool input.".to_string(),
                    is_error: true,
                    weles_review: tc.weles_review.clone(),
                    pending_approval: None,
                }
            } else {
                Box::pin(execute_tool(
                    tc,
                    self.engine,
                    &self.tid,
                    self.task_id,
                    &self.engine.session_manager,
                    self.preferred_session_id,
                    &self.engine.event_tx,
                    &self.engine.data_dir,
                    &self.engine.http_client,
                    Some(self.stream_cancel_token.clone()),
                ))
                .await
            }
        } else {
            self.consecutive_same_tool_calls = 1;
            Box::pin(execute_tool(
                tc,
                self.engine,
                &self.tid,
                self.task_id,
                &self.engine.session_manager,
                self.preferred_session_id,
                &self.engine.event_tx,
                &self.engine.data_dir,
                &self.engine.http_client,
                Some(self.stream_cancel_token.clone()),
            ))
            .await
        };
        self.previous_tool_signature = Some(current_tool_signature);
        result
    }

    fn activate_deferred_tool(&mut self, name: &str) {
        if self.tools.iter().any(|tool| tool.function.name == name) {
            return;
        }
        if let Some(def) = self
            .deferred_tool_pool
            .iter()
            .find(|tool| tool.function.name == name)
            .cloned()
        {
            self.tools.push(def);
        }
    }

    fn handle_load_tools(&mut self, tc: &ToolCall) -> ToolResult {
        let names: Vec<String> = serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
            .ok()
            .and_then(|value| {
                value
                    .get("names")
                    .and_then(|names| names.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(str::to_string))
                            .collect()
                    })
            })
            .unwrap_or_default();

        let mut loaded = Vec::new();
        let mut already = Vec::new();
        let mut unknown = Vec::new();
        for name in &names {
            if self.tools.iter().any(|tool| &tool.function.name == name) {
                already.push(name.clone());
            } else if let Some(def) = self
                .deferred_tool_pool
                .iter()
                .find(|tool| &tool.function.name == name)
                .cloned()
            {
                self.tools.push(def);
                loaded.push(name.clone());
            } else {
                unknown.push(name.clone());
            }
        }

        let mut parts = Vec::new();
        if !loaded.is_empty() {
            parts.push(format!(
                "Activated and now callable: {}.",
                loaded.join(", ")
            ));
        }
        if !already.is_empty() {
            parts.push(format!("Already active: {}.", already.join(", ")));
        }
        if !unknown.is_empty() {
            parts.push(format!(
                "Unknown or not deferrable: {}. Use tool_search to find exact names.",
                unknown.join(", ")
            ));
        }
        if parts.is_empty() {
            parts.push("No tool names provided.".to_string());
        }

        ToolResult {
            tool_call_id: tc.id.clone(),
            name: tc.function.name.clone(),
            content: parts.join(" "),
            is_error: loaded.is_empty() && already.is_empty(),
            weles_review: tc.weles_review.clone(),
            pending_approval: None,
        }
    }

    fn record_tool_trace(&mut self, tc: &ToolCall, result: &ToolResult) {
        self.termination_tool_calls += 1;
        if result.is_error {
            self.termination_consecutive_errors += 1;
            self.termination_total_errors += 1;
        } else {
            self.termination_consecutive_errors = 0;
            self.termination_tool_successes += 1;
        }
        if self.task_id.is_some() {
            self.trace_collector.record_step(
                &tc.function.name,
                &crate::agent::learning::traces::hash_arguments(&tc.function.arguments),
                !result.is_error,
                0,
                0,
                if result.is_error {
                    Some(result.content.clone())
                } else {
                    None
                },
                now_millis(),
            );
        }
    }

    async fn should_break_for_termination(&mut self) -> bool {
        if let Some(evaluator) = self.task_termination_eval.as_ref() {
            let elapsed = now_millis().saturating_sub(self.loop_started_at) / 1000;
            let metrics = crate::agent::subagent::termination::TerminationMetrics {
                elapsed_secs: elapsed,
                tool_calls_total: self.termination_tool_calls,
                tool_calls_succeeded: self.termination_tool_successes,
                consecutive_errors: self.termination_consecutive_errors,
                total_errors: self.termination_total_errors,
            };
            let (should_stop, reason) = evaluator.should_terminate(&metrics);
            if should_stop {
                tracing::info!(thread_id = %self.tid, reason = ?reason, "sub-agent terminated by condition");
                self.engine.emit_workflow_notice(
                    &self.tid,
                    "termination-triggered",
                    &format!(
                        "Sub-agent terminated: {}",
                        reason.as_deref().unwrap_or("condition met")
                    ),
                    None,
                );
                return true;
            }
        }

        if self.task_context_budget.is_some() {
            self.sync_task_context_budget_from_live_task().await;
            let current_tokens = {
                let threads = self.engine.threads.read().await;
                threads
                    .get(&self.tid)
                    .map(|thread| {
                        crate::agent::subagent::context_budget::visible_output_budget_tokens(
                            &thread.messages,
                        )
                    })
                    .unwrap_or(0)
            };
            let overflow_action = self.task_context_budget.as_mut().and_then(|budget| {
                budget.set_consumed(current_tokens);
                match budget.check() {
                    crate::agent::subagent::context_budget::BudgetStatus::Exceeded {
                        overflow_action,
                        ..
                    } => Some(overflow_action),
                    crate::agent::subagent::context_budget::BudgetStatus::Warning {
                        consumed,
                        max,
                    } => {
                        tracing::debug!(thread_id = %self.tid, consumed, max, "context budget warning");
                        None
                    }
                    _ => None,
                }
            });
            if let Some(overflow_action) = overflow_action {
                let is_spawned = self.spawned_subagent_turn_active();
                let already_in_report_back = self.report_back_phase.is_some();
                match super::report_back::budget_overflow_decision(
                    is_spawned,
                    already_in_report_back,
                    overflow_action,
                ) {
                    super::report_back::BudgetOverflowDecision::EnterReportBack => {
                        tracing::warn!(
                            thread_id = %self.tid,
                            "output token budget exceeded - forcing budget-exempt report-back"
                        );
                        if self.enter_execution_budget_report_back().await {
                            return false;
                        }
                        self.terminated_for_budget = true;
                        return true;
                    }
                    super::report_back::BudgetOverflowDecision::Stop => {
                        tracing::warn!(thread_id = %self.tid, "output token budget exceeded - stopping");
                        self.terminated_for_budget = true;
                        self.engine.emit_workflow_notice(
                            &self.tid,
                            "budget-exceeded",
                            "Output token budget exceeded, execution stopped.",
                            None,
                        );
                        return true;
                    }
                    super::report_back::BudgetOverflowDecision::RelyOnCompaction => {
                        tracing::info!(thread_id = %self.tid, "output token budget exceeded - relying on compaction");
                    }
                    super::report_back::BudgetOverflowDecision::Continue => {}
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::inter_tool_call_delay;
    use std::time::Duration;

    #[test]
    fn inter_tool_call_delay_uses_configured_delay_between_consecutive_tools() {
        assert_eq!(inter_tool_call_delay(0, 500), None);
        assert_eq!(
            inter_tool_call_delay(1, 500),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            inter_tool_call_delay(2, 500),
            Some(Duration::from_millis(500))
        );
        assert_eq!(inter_tool_call_delay(1, 0), None);
    }

    #[test]
    fn busy_wait_poll_detection_flags_repeated_list_agents_with_active_subagents() {
        assert!(super::should_suppress_busy_wait_poll(
            zorai_protocol::tool_names::LIST_AGENTS,
            "list_agents:{}",
            Some("list_agents:{}"),
            1,
            true,
            false,
        ));
    }

    #[test]
    fn busy_wait_poll_detection_allows_first_status_probe() {
        assert!(!super::should_suppress_busy_wait_poll(
            zorai_protocol::tool_names::LIST_SUBAGENTS,
            "list_subagents:{}",
            Some("spawn_subagent:{\"title\":\"review\"}"),
            1,
            true,
            false,
        ));
    }

    #[test]
    fn busy_wait_poll_detection_ignores_non_poll_tools() {
        assert!(!super::should_suppress_busy_wait_poll(
            zorai_protocol::tool_names::READ_FILE,
            "read_file:{\"path\":\"PROJECT.md\"}",
            Some("read_file:{\"path\":\"PROJECT.md\"}"),
            1,
            true,
            false,
        ));
    }

    #[test]
    fn busy_wait_poll_detection_flags_repeated_operation_status_without_wait() {
        assert!(super::should_suppress_busy_wait_poll(
            zorai_protocol::tool_names::GET_OPERATION_STATUS,
            r#"get_operation_status:{"operation_id":"op-1"}"#,
            Some(r#"get_operation_status:{"operation_id":"op-1"}"#),
            1,
            false,
            false,
        ));
    }

    #[test]
    fn busy_wait_poll_detection_allows_blocking_operation_status_wait() {
        assert!(!super::should_suppress_busy_wait_poll(
            zorai_protocol::tool_names::GET_OPERATION_STATUS,
            r#"get_operation_status:{"operation_id":"op-1","wait":true}"#,
            Some(r#"get_operation_status:{"operation_id":"op-1","wait":true}"#),
            1,
            false,
            true,
        ));
    }
}
