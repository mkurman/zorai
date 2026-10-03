use super::*;

impl<'a> SendMessageRunner<'a> {
    pub(super) async fn handle_iteration(
        &mut self,
        iteration: StreamIteration,
    ) -> Result<LoopDisposition> {
        let StreamIteration {
            prepared_request: _prepared_request,
            llm_started_at,
            first_token_at,
            effective_transport_for_turn,
            accumulated_content,
            accumulated_reasoning,
            final_chunk,
            stream_timed_out: _stream_timed_out,
            retry_loop: _retry_loop,
        } = iteration;

        match final_chunk {
            Some(CompletionChunk::Done {
                content,
                reasoning,
                input_tokens,
                output_tokens,
                cost_usd,
                stop_reason: _,
                stop_sequence: _,
                response_id,
                request_id: _,
                upstream_model: _,
                upstream_role: _,
                upstream_message_type: _,
                upstream_container: _,
                mut upstream_message,
                mut provider_final_result,
                upstream_thread_id,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                server_tool_use: _,
            }) => {
                crate::agent::types::apply_completion_cache_usage(
                    &mut upstream_message,
                    &mut provider_final_result,
                    cache_creation_input_tokens,
                    cache_read_input_tokens,
                );
                self.handle_done_chunk(
                    llm_started_at,
                    first_token_at,
                    effective_transport_for_turn,
                    accumulated_content,
                    accumulated_reasoning,
                    content,
                    reasoning,
                    input_tokens,
                    output_tokens,
                    cost_usd,
                    response_id,
                    upstream_message,
                    provider_final_result,
                    upstream_thread_id,
                )
                .await
            }
            Some(CompletionChunk::ToolCalls {
                tool_calls,
                content,
                reasoning,
                input_tokens,
                output_tokens,
                stop_reason: _,
                stop_sequence: _,
                response_id,
                request_id: _,
                upstream_model: _,
                upstream_role: _,
                upstream_message_type: _,
                upstream_container: _,
                mut upstream_message,
                mut provider_final_result,
                upstream_thread_id,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                server_tool_use: _,
            }) => {
                crate::agent::types::apply_completion_cache_usage(
                    &mut upstream_message,
                    &mut provider_final_result,
                    cache_creation_input_tokens,
                    cache_read_input_tokens,
                );
                Box::pin(self.handle_tool_calls_chunk(
                    llm_started_at,
                    first_token_at,
                    effective_transport_for_turn,
                    accumulated_content,
                    accumulated_reasoning,
                    tool_calls,
                    content,
                    reasoning,
                    input_tokens,
                    output_tokens,
                    response_id,
                    upstream_message,
                    provider_final_result,
                    upstream_thread_id,
                ))
                .await
            }
            _ => {
                self.engine
                    .record_llm_outcome(&self.config.provider, false)
                    .await;
                let fallback_message = unexpected_stream_end_message(&accumulated_content);
                let final_reasoning = (!accumulated_reasoning.trim().is_empty())
                    .then_some(accumulated_reasoning.clone());
                let persisted_message_id = self
                    .engine
                    .add_assistant_message(
                        &self.tid,
                        &fallback_message,
                        0,
                        0,
                        final_reasoning.clone(),
                        Some(self.config.provider.clone()),
                        Some(self.provider_config.model.clone()),
                        Some(self.provider_config.api_transport),
                        None,
                    )
                    .await;
                if let Err(error) = self
                    .engine
                    .complete_workspace_thread_task_by_thread_id(&self.tid)
                    .await
                {
                    tracing::warn!(
                        thread_id = %self.tid,
                        error = %error,
                        "failed to complete workspace thread task"
                    );
                }
                let _ = self.engine.event_tx.send(AgentEvent::Done {
                    thread_id: self.tid.clone(),
                    input_tokens: 0,
                    output_tokens: 0,
                    cost: None,
                    provider: Some(self.config.provider.clone()),
                    model: Some(self.provider_config.model.clone()),
                    tps: None,
                    generation_ms: None,
                    reasoning: final_reasoning,
                    upstream_message: None,
                    provider_final_result: None,
                    message_id: persisted_message_id,
                });
                self.turn_done_emitted = true;
                Ok(LoopDisposition::Break)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_done_chunk(
        &mut self,
        llm_started_at: Instant,
        first_token_at: Option<Instant>,
        effective_transport_for_turn: ApiTransport,
        accumulated_content: String,
        accumulated_reasoning: String,
        content: String,
        reasoning: Option<String>,
        input_tokens: u64,
        output_tokens: u64,
        reported_cost_usd: Option<f64>,
        response_id: Option<String>,
        upstream_message: Option<CompletionUpstreamMessage>,
        provider_final_result: Option<CompletionProviderFinalResult>,
        upstream_thread_id: Option<String>,
    ) -> Result<LoopDisposition> {
        let turn_cost =
            crate::agent::cost::reported_turn_cost(self.config.cost.enabled, reported_cost_usd);
        let mut final_content = if content.is_empty() {
            accumulated_content
        } else {
            content
        };
        if let Some((tool_name, error_message)) = self.last_tool_error.as_ref() {
            let lower = final_content.to_ascii_lowercase();
            if !lower.contains("failed")
                && !lower.contains("error")
                && !lower.contains("could not")
                && !lower.contains("unable")
            {
                final_content = format!(
                    "The last tool call failed (`{tool_name}`): {error_message}\n\n{final_content}"
                );
            }
        }
        let final_reasoning = reasoning.or(if accumulated_reasoning.is_empty() {
            None
        } else {
            Some(accumulated_reasoning)
        });
        if self.retry_status_visible {
            let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                thread_id: self.tid.clone(),
                phase: "cleared".to_string(),
                attempt: 0,
                max_retries: 0,
                delay_ms: 0,
                failure_class: String::new(),
                message: String::new(),
            });
            self.retry_status_visible = false;
        }

        let persisted_message_id = self
            .engine
            .add_assistant_message_with_upstream_message(
                &self.tid,
                &final_content,
                input_tokens,
                output_tokens,
                final_reasoning.clone(),
                Some(self.config.provider.clone()),
                Some(self.provider_config.model.clone()),
                Some(effective_transport_for_turn),
                response_id,
                upstream_message.clone(),
                provider_final_result.clone(),
                turn_cost,
            )
            .await;
        if self.report_back_phase.is_some() {
            self.mark_last_assistant_report_back().await;
            self.capture_text_report_back(&final_content).await;
        }
        let continue_for_report_back = self.subagent_report.is_none()
            && self.report_back_phase.is_none()
            && self.maybe_force_budget_report_back().await;
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

        let generation_secs = first_token_at
            .unwrap_or(llm_started_at)
            .elapsed()
            .as_secs_f64();
        let (generation_ms, tps) = compute_generation_stats(generation_secs, output_tokens);

        self.engine
            .accumulate_goal_run_cost(
                &self.tid,
                input_tokens,
                output_tokens,
                &self.config.provider,
                &self.provider_config.model,
                generation_ms,
            )
            .await;
        self.provider_final_result = provider_final_result.clone();
        if !super::report_back::should_emit_turn_done_on_text_completion(continue_for_report_back) {
            self.needs_turn_done = true;
            return Ok(LoopDisposition::Continue);
        }
        if let Err(error) = self
            .engine
            .complete_workspace_thread_task_by_thread_id(&self.tid)
            .await
        {
            tracing::warn!(
                thread_id = %self.tid,
                error = %error,
                "failed to complete workspace thread task"
            );
        }

        self.emit_turn_done(
            input_tokens,
            output_tokens,
            turn_cost,
            tps,
            generation_ms,
            final_reasoning,
            upstream_message,
            provider_final_result,
            persisted_message_id,
        );
        Ok(LoopDisposition::Break)
    }

    pub(super) async fn finish(mut self) -> Result<SendMessageOutcome> {
        self.apply_finish_subagent_synthesis().await;
        if super::report_back::should_emit_tool_execution_limit_error(
            self.was_cancelled,
            self.max_loops,
            self.loop_count,
            self.subagent_report.is_some(),
        ) {
            let _ = self.engine.event_tx.send(AgentEvent::Error {
                thread_id: self.tid.clone(),
                message: "Tool execution limit reached".into(),
            });
        }
        if self.was_cancelled {
            if self.retry_status_visible {
                let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                    thread_id: self.tid.clone(),
                    phase: "cleared".to_string(),
                    attempt: 0,
                    max_retries: 0,
                    delay_ms: 0,
                    failure_class: String::new(),
                    message: String::new(),
                });
                self.retry_status_visible = false;
            }
            if !self.turn_done_emitted {
                let _ = self.engine.event_tx.send(AgentEvent::TurnInterrupted {
                    thread_id: self.tid.clone(),
                });
            }
        } else if super::report_back::should_emit_deferred_turn_done(
            self.needs_turn_done,
            self.interrupted_for_approval,
        ) {
            self.emit_turn_done(
                0,
                0,
                None,
                None,
                None,
                None,
                None,
                self.provider_final_result.clone(),
                None,
            );
        }

        if self.task_id.is_some() {
            let trace_outcome = if self.policy_aborted_retry {
                crate::agent::learning::traces::TraceOutcome::Failure {
                    reason: "policy halted repeated retry".to_string(),
                }
            } else if self.terminated_for_budget {
                crate::agent::learning::traces::TraceOutcome::Failure {
                    reason: "budget exceeded".to_string(),
                }
            } else if self.interrupted_for_approval {
                crate::agent::learning::traces::TraceOutcome::Partial {
                    completed_pct: 50.0,
                }
            } else if self.was_cancelled {
                crate::agent::learning::traces::TraceOutcome::Cancelled
            } else {
                crate::agent::learning::traces::TraceOutcome::Success
            };
            let final_success_rate = self.trace_collector.success_rate();
            let trace_started_at_ms = self.trace_collector.started_at_ms();
            let trace = self.trace_collector.finalize(
                trace_outcome,
                None,
                self.task_id.map(str::to_string),
                None,
                now_millis(),
            );
            if self.policy_aborted_retry || !trace.steps.is_empty() {
                let tool_seq = crate::agent::learning::traces::extract_tool_sequence(&trace);
                let tool_fallbacks = trace
                    .steps
                    .windows(2)
                    .filter(|pair| pair[0].tool_name != pair[1].tool_name && !pair[0].succeeded)
                    .count() as u64;
                let synthetic_exit_code = match &trace.outcome {
                    crate::agent::learning::traces::TraceOutcome::Success => Some(0_i64),
                    crate::agent::learning::traces::TraceOutcome::Failure { .. } => Some(1_i64),
                    crate::agent::learning::traces::TraceOutcome::Partial { .. } => Some(2_i64),
                    crate::agent::learning::traces::TraceOutcome::Cancelled => Some(130_i64),
                };
                let tool_seq_json = serde_json::to_string(&tool_seq).unwrap_or_default();
                let metrics_json = serde_json::to_string(&serde_json::json!({
                    "total_duration_ms": trace.total_duration_ms,
                    "total_tokens_used": trace.total_tokens_used,
                    "step_count": trace.steps.len(),
                    "success_rate": final_success_rate,
                    "tool_fallbacks": tool_fallbacks,
                    "operator_revisions": 0,
                    "fast_denials": 0,
                    "exit_code": synthetic_exit_code,
                }))
                .unwrap_or_default();
                let outcome_str = match &trace.outcome {
                    crate::agent::learning::traces::TraceOutcome::Success => "success",
                    crate::agent::learning::traces::TraceOutcome::Failure { .. } => "failure",
                    crate::agent::learning::traces::TraceOutcome::Partial { .. } => "partial",
                    crate::agent::learning::traces::TraceOutcome::Cancelled => "cancelled",
                };
                if let Err(error) = self
                    .engine
                    .history
                    .insert_execution_trace(
                        &trace.trace_id,
                        Some(&self.tid),
                        None,
                        self.task_id,
                        &trace.task_type,
                        outcome_str,
                        trace.quality_score,
                        &tool_seq_json,
                        &metrics_json,
                        trace.total_duration_ms,
                        trace.total_tokens_used,
                        &self.agent_scope_id,
                        trace_started_at_ms,
                        trace.created_at,
                        trace.created_at,
                    )
                    .await
                {
                    tracing::warn!(task_id = ?self.task_id, "failed to persist execution trace: {error}");
                }
            }
        }

        if let Some(task) = self.current_task_snapshot.as_ref() {
            self.engine.persist_subagent_runtime_metrics(&task.id).await;
        }

        self.engine.persist_thread_by_id(&self.tid).await;
        let final_upstream_message = {
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
                .and_then(|message| message.upstream_message.clone())
        };
        let outcome = SendMessageOutcome {
            thread_id: self.tid,
            stream_generation: self.stream_generation,
            interrupted_for_approval: self.interrupted_for_approval,
            terminated_for_budget: self.terminated_for_budget,
            subagent_report: self.subagent_report,
            upstream_message: final_upstream_message,
            provider_final_result: self.provider_final_result,
            fresh_runner_retry: self.fresh_runner_retry,
            handoff_restart: self.handoff_restart,
        };
        if outcome.fresh_runner_retry.is_none() {
            self.engine
                .finish_stream_cancellation(&outcome.thread_id, outcome.stream_generation)
                .await;
        }
        if let Err(error) = Box::pin(
            self.engine
                .flush_deferred_visible_thread_continuations(&outcome.thread_id),
        )
        .await
        {
            tracing::warn!(
                thread_id = %outcome.thread_id,
                %error,
                "failed to flush deferred visible-thread continuations"
            );
        }
        Ok(outcome)
    }
}
