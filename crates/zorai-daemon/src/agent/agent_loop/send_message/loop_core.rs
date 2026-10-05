use super::*;
use crate::agent::llm_client::CopilotInitiator;

pub(super) fn inter_request_delay(
    loop_count: u32,
    configured_delay_ms: u64,
) -> Option<std::time::Duration> {
    (loop_count > 1 && configured_delay_ms > 0)
        .then(|| std::time::Duration::from_millis(configured_delay_ms))
}

fn provider_uses_anthropic_api(
    config: &AgentConfig,
    provider_config: &ProviderConfig,
    transport: ApiTransport,
) -> bool {
    transport == ApiTransport::AnthropicMessages
        || crate::agent::types::get_provider_api_type(
            &config.provider,
            &provider_config.model,
            &provider_config.base_url,
        ) == ApiType::Anthropic
}

const MAX_OUTER_AUTO_RETRY_WAITS_BEFORE_FRESH_RUNNER: u32 = 2;
const MAX_WAITING_TIMEOUTS_BEFORE_FRESH_RUNNER: u32 = 2;

fn stream_timeout_retry_delay_ms(stream_timeout_count: u32) -> u64 {
    if cfg!(test) {
        if stream_timeout_count >= 3 {
            50
        } else {
            20
        }
    } else if stream_timeout_count >= 3 {
        30_000
    } else {
        2_000
    }
}

fn is_context_window_exceeded_failure(message: &str) -> bool {
    if parse_structured_upstream_failure(message)
        .as_ref()
        .is_some_and(|failure| failure.class == "context_window_exceeded")
    {
        return true;
    }

    let lower = message.to_ascii_lowercase();
    lower.contains("context_length_exceeded")
        || lower.contains("context window")
        || lower.contains("context length")
        || (lower.contains("input") && lower.contains("exceeds") && lower.contains("context"))
}

impl<'a> SendMessageRunner<'a> {
    fn clear_retry_status_on_progress(&mut self) {
        if !self.retry_status_visible {
            return;
        }
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

    fn resolve_claude_permission_mode(&self) -> Option<String> {
        let nonempty = |value: Option<&str>| {
            value
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        };
        let target = crate::agent::agent_identity::resolve_agent_target(
            &self.agent_scope_id,
            &self.config.sub_agents,
        );
        if let Some(mode) = target
            .matched_sub_agent
            .as_ref()
            .and_then(|def| nonempty(def.claude_permission_mode.as_deref()))
        {
            return Some(mode);
        }
        if target.scope_id == crate::agent::agent_identity::CONCIERGE_AGENT_ID {
            if let Some(mode) = nonempty(self.config.concierge.claude_permission_mode.as_deref()) {
                return Some(mode);
            }
        }
        nonempty(self.config.claude_permission_mode.as_deref())
    }

    async fn execution_profile_supersedes_runner(&self) -> bool {
        let Some(profile) = self.engine.get_thread_execution_profile(&self.tid).await else {
            return false;
        };
        let provider = profile
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let model = profile
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        provider.is_some_and(|value| value != self.active_provider_id)
            || model.is_some_and(|value| value != self.provider_config.model.trim())
    }

    fn fresh_runner_retry_err(&self) -> anyhow::Error {
        FreshRunnerRetrySignal {
            scheduled_retry_cycles: self.scheduled_retry_cycles,
        }
        .into()
    }

    async fn fail_for_open_circuit(&self) -> anyhow::Error {
        let outage_context = self
            .engine
            .suggest_alternative_provider(&self.config.provider)
            .await
            .unwrap_or_else(|| {
                "No healthy fallback providers are currently available.".to_string()
            });
        let error_msg = format!(
            "Provider '{}' is temporarily unavailable (circuit breaker open). {}",
            self.config.provider, outage_context
        );
        let _ = self.engine.event_tx.send(AgentEvent::Error {
            thread_id: self.tid.clone(),
            message: error_msg.clone(),
        });
        self.stream_cancel_token.cancel();
        self.engine
            .finish_stream_cancellation(&self.tid, self.stream_generation)
            .await;
        anyhow::anyhow!(
            "Circuit breaker open for provider '{}' — the in-flight stream was stopped. {error_msg}",
            self.config.provider
        )
    }

    async fn prepare_request(&mut self) -> Result<PreparedLlmRequest> {
        let compaction_inserted = self
            .engine
            .maybe_persist_compaction_artifact(
                &self.tid,
                self.task_id,
                &self.config,
                &self.provider_config,
            )
            .await?;
        if compaction_inserted {
            self.engine.clear_thread_continuation_state(&self.tid).await;
            self.recorded_compaction_provenance = true;
        }
        let threads = self.engine.threads.read().await;
        let thread = match threads.get(&self.tid) {
            Some(thread) => thread,
            None => {
                drop(threads);
                self.engine
                    .finish_stream_cancellation(&self.tid, self.stream_generation)
                    .await;
                anyhow::bail!("thread not found");
            }
        };
        // Avoid cloning the entire AgentThread (which duplicates the full Vec<AgentMessage>)
        // on every LLM loop. Hold the read guard and only clone messages when we must patch
        // the last user message content. This cuts per-turn heap churn from O(thread len)
        // to O(1) in the common case (stored == llm content).
        let needs_patch =
            self.llm_user_content != self.stored_user_content && !self.reuse_existing_user_message;
        // Megathread warning — surfaces unbounded growth before it hits 13 GB again.
        if thread.messages.len() > 2048 && self.loop_count <= 2 {
            tracing::warn!(
                thread_id = %self.tid,
                thread_messages = thread.messages.len(),
                "large in-memory thread (consider windowed hydration)"
            );
        }
        // Materialize patched messages only if needed.
        let patched_messages: Option<Vec<AgentMessage>> = if needs_patch {
            let idx = thread
                .messages
                .iter()
                .rposition(|m| m.role == MessageRole::User);
            if let Some(i) = idx {
                let mut v = thread.messages.clone();
                v[i].content = self.llm_user_content.to_string();
                Some(v)
            } else {
                None
            }
        } else {
            None
        };
        let effective_messages: &[AgentMessage] = patched_messages
            .as_deref()
            .unwrap_or(thread.messages.as_slice());
        self.emit_context_window_update_for_messages(effective_messages);
        let mut prepared = if let Some(ref pm) = patched_messages {
            crate::agent::compaction::prepare_llm_request_from_messages(
                pm.as_slice(),
                thread,
                &self.config,
                &self.provider_config,
                self.reuse_existing_user_message
                    .then_some(self.llm_user_content),
            )
        } else {
            prepare_llm_request_with_reused_user_message(
                thread,
                &self.config,
                &self.provider_config,
                self.reuse_existing_user_message
                    .then_some(self.llm_user_content),
            )
        };
        prepared.force_connection_close = compaction_inserted;
        let effective_len = effective_messages.len();
        let api_len = prepared.messages.len();
        let api_transport = prepared.transport;
        if !self.recorded_compaction_provenance {
            if let Some(candidate) =
                compaction_candidate(effective_messages, &self.config, &self.provider_config)
            {
                let thread_len = thread.messages.len();
                // Need to drop the read guard before awaiting (borrow checker + deadlock avoidance)
                drop(threads);
                self.engine
                    .record_provenance_event(
                        "context_compressed",
                        "thread context was compacted for an LLM request",
                        serde_json::json!({
                            "thread_id": self.tid.as_str(),
                            "split_at": candidate.split_at,
                            "target_tokens": candidate.target_tokens,
                            "message_count": thread_len,
                        }),
                        None,
                        self.task_id,
                        Some(self.tid.as_str()),
                        None,
                        None,
                    )
                    .await;
                self.recorded_compaction_provenance = true;
            } else {
                drop(threads);
            }
        } else {
            drop(threads);
        }
        tracing::info!(
            thread_id = %self.tid,
            thread_messages = effective_len,
            api_messages = api_len,
            transport = ?api_transport,
            loop_count = self.loop_count,
            "building LLM request"
        );
        Ok(prepared)
    }

    pub(super) async fn stream_once(&mut self) -> Result<StreamIteration> {
        if self.execution_profile_supersedes_runner().await {
            return Err(self.fresh_runner_retry_err());
        }
        let prepared_request = self.prepare_request().await?;
        if let Some(delay) = inter_request_delay(self.loop_count, self.config.message_loop_delay_ms)
        {
            tokio::time::sleep(delay).await;
        }

        if self
            .engine
            .check_circuit_breaker(&self.config.provider)
            .await
            .is_err()
        {
            return Err(self.fail_for_open_circuit().await);
        }

        let llm_started_at = Instant::now();
        let mut first_token_at: Option<Instant> = None;
        let effective_transport_for_turn = prepared_request.transport;
        let provider_is_anthropic = provider_uses_anthropic_api(
            &self.config,
            &self.provider_config,
            prepared_request.transport,
        );
        let llm_retry_strategy = if provider_is_anthropic {
            match self.retry_strategy {
                RetryStrategy::Bounded { retry_delay_ms, .. } => RetryStrategy::Bounded {
                    max_retries: 0,
                    retry_delay_ms,
                },
                RetryStrategy::DurableRateLimited => RetryStrategy::Bounded {
                    max_retries: 0,
                    retry_delay_ms: 5_000,
                },
            }
        } else {
            self.retry_strategy
        };
        let request_client = if prepared_request.force_connection_close {
            crate::agent::engine::build_fresh_agent_http_client(
                crate::agent::engine::default_agent_http_read_timeout(),
            )
        } else {
            self.engine.http_client.clone()
        };
        let copilot_initiator = if self.loop_count <= 1 {
            self.initial_copilot_initiator
        } else {
            CopilotInitiator::Agent
        };
        let is_claude_code_cli =
            self.config.provider == zorai_shared::providers::PROVIDER_ID_CLAUDE_CODE_CLI;
        let uses_local_subscription_cli =
            crate::agent::types::provider_uses_local_subscription_cli(&self.config.provider);
        let working_dir = if uses_local_subscription_cli {
            self.engine
                .resolve_thread_repo_root(&self.tid)
                .await
                .map(|(repo_root, _, _, _)| repo_root)
        } else {
            None
        };
        let claude_permission_mode = if is_claude_code_cli {
            self.resolve_claude_permission_mode()
        } else {
            None
        };
        let mut stream = crate::agent::llm_client::send_completion_request_with_options(
            &request_client,
            &self.config.provider,
            &self.provider_config,
            &self.system_prompt,
            &prepared_request.messages,
            &self.tools,
            prepared_request.transport,
            prepared_request.previous_response_id.clone(),
            prepared_request.upstream_thread_id.clone(),
            llm_retry_strategy,
            crate::agent::llm_client::CompletionRequestOptions {
                force_connection_close: prepared_request.force_connection_close,
                copilot_initiator,
                working_dir,
                claude_permission_mode,
                retry_now: Some(self.stream_retry_now.clone()),
                cancel: Some(self.stream_cancel_token.clone()),
                opencode_session_id: Some(self.tid.clone()),
            },
        );

        let mut accumulated_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut final_chunk: Option<CompletionChunk> = None;
        let llm_stream_chunk_timeout = self.stream_chunk_timeout_override.unwrap_or_else(|| {
            std::time::Duration::from_secs(self.config.llm_stream_chunk_timeout_secs)
        });
        let mut stream_timed_out = false;

        loop {
            tokio::select! {
                biased;
                _ = self.stream_cancel_token.cancelled() => {
                    if self.engine.operator_stream_stop_requested(&self.tid).await
                        || self
                            .engine
                            .provider_circuit_is_closed(&self.config.provider)
                            .await
                    {
                        self.was_cancelled = true;
                        break;
                    }
                    return Err(self.fail_for_open_circuit().await);
                }
                _ = self.stream_retry_now.notified() => {
                    if self.execution_profile_supersedes_runner().await {
                        return Err(self.fresh_runner_retry_err());
                    }
                }
                _ = tokio::time::sleep(llm_stream_chunk_timeout) => {
                    tracing::warn!("LLM stream timeout -- no data for {}s", llm_stream_chunk_timeout.as_secs());
                    self.engine.record_llm_outcome(&self.config.provider, false).await;
                    stream_timed_out = true;
                    break;
                }
                maybe_chunk = stream.next() => {
                    let Some(chunk_result) = maybe_chunk else {
                        break;
                    };

                    let chunk = match chunk_result {
                        Ok(chunk) => chunk,
                        Err(e) => {
                            let err_str = e.to_string();
                            if err_str.contains("tool call result does not follow tool call")
                                && !self.tool_sequence_repaired
                            {
                                tracing::warn!("detected broken tool call sequence -- repairing thread and retrying");
                                self.tool_sequence_repaired = true;
                                self.engine.repair_tool_call_sequence(&self.tid).await;
                                let _ = self.engine.event_tx.send(AgentEvent::WorkflowNotice {
                                    thread_id: self.tid.clone(),
                                    kind: "tool-repair".to_string(),
                                    message: "Repairing message sequence, retrying...".to_string(),
                                    details: None,
                                });
                                return Ok(StreamIteration {
                                    prepared_request,
                                    llm_started_at,
                                    first_token_at,
                                    effective_transport_for_turn,
                                    accumulated_content,
                                    accumulated_reasoning,
                                    final_chunk: None,
                                    stream_timed_out: true,
                                    retry_loop: true,
                                });
                            }
                            self.engine.record_llm_outcome(&self.config.provider, false).await;
                            self.engine.finish_stream_cancellation(&self.tid, self.stream_generation).await;
                            return Err(e);
                        }
                    };

                    match chunk {
                        CompletionChunk::Delta { content, reasoning } => {
                            if !content.is_empty()
                                || reasoning.as_ref().is_some_and(|value| !value.is_empty())
                            {
                                self.clear_retry_status_on_progress();
                            }
                            if first_token_at.is_none()
                                && (!content.is_empty()
                                    || reasoning.as_ref().map(|s| !s.is_empty()).unwrap_or(false))
                            {
                                first_token_at = Some(Instant::now());
                            }
                            if !content.is_empty() {
                                self.assistant_output_visible = true;
                                accumulated_content.push_str(&content);
                                self.engine
                                    .note_stream_progress(
                                        &self.tid,
                                        self.stream_generation,
                                        StreamProgressKind::Content,
                                        &content,
                                    )
                                    .await;
                                let _ = self.engine.event_tx.send(AgentEvent::Delta {
                                    thread_id: self.tid.clone(),
                                    content,
                                });
                            }
                            if let Some(r) = reasoning {
                                accumulated_reasoning.push_str(&r);
                                self.engine
                                    .note_stream_progress(
                                        &self.tid,
                                        self.stream_generation,
                                        StreamProgressKind::Reasoning,
                                        &r,
                                    )
                                    .await;
                                let _ = self.engine.event_tx.send(AgentEvent::Reasoning {
                                    thread_id: self.tid.clone(),
                                    content: r,
                                });
                            }
                        }
                        CompletionChunk::Retry {
                            attempt,
                            max_retries,
                            delay_ms,
                            failure_class,
                            message,
                        } => {
                            if !self
                                .engine
                                .provider_circuit_is_closed(&self.config.provider)
                                .await
                            {
                                self.engine
                                    .record_llm_outcome(&self.config.provider, false)
                                    .await;
                                return Err(self.fail_for_open_circuit().await);
                            }
                            if self.stream_cancel_token.is_cancelled() {
                                self.was_cancelled = true;
                                break;
                            }
                            if self.execution_profile_supersedes_runner().await {
                                return Err(self.fresh_runner_retry_err());
                            }
                            tracing::info!(
                                thread_id = %self.tid,
                                provider = %self.config.provider,
                                attempt,
                                max_retries,
                                delay_ms,
                                failure_class = %failure_class,
                                message = %sanitize_upstream_failure_message(&message),
                                "provider-level retry requested by llm client"
                            );
                            let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                thread_id: self.tid.clone(),
                                phase: "retrying".to_string(),
                                attempt,
                                max_retries,
                                delay_ms,
                                failure_class,
                                message,
                            });
                            self.retry_status_visible = true;
                            self.engine
                                .note_stream_progress(
                                    &self.tid,
                                    self.stream_generation,
                                    StreamProgressKind::Started,
                                    &format!("retry {attempt}/{max_retries}"),
                                )
                                .await;
                        }
                        CompletionChunk::TransportFallback { .. } => {}
                        chunk @ CompletionChunk::Done { .. } => {
                            self.clear_retry_status_on_progress();
                            self.engine
                                .note_stream_progress(
                                    &self.tid,
                                    self.stream_generation,
                                    StreamProgressKind::Content,
                                    &accumulated_content,
                                )
                                .await;
                            final_chunk = Some(chunk);
                            break;
                        }
                        chunk @ CompletionChunk::ToolCalls { .. } => {
                            self.clear_retry_status_on_progress();
                            self.engine
                                .note_stream_progress(
                                    &self.tid,
                                    self.stream_generation,
                                    StreamProgressKind::ToolCalls,
                                    &accumulated_reasoning,
                                )
                                .await;
                            final_chunk = Some(chunk);
                            break;
                        }
                        CompletionChunk::Error { message } => {
                            let visible_message = sanitize_upstream_failure_message(&message);
                            let structured_failure = parse_structured_upstream_failure(&message);
                            if is_context_window_exceeded_failure(&message) {
                                tracing::warn!(
                                    thread_id = %self.tid,
                                    provider = %self.config.provider,
                                    model = %self.provider_config.model,
                                    visible_message = %visible_message,
                                    "context-window overflow detected; forcing compaction before retry"
                                );
                                let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                    thread_id: self.tid.clone(),
                                    phase: "compacting".to_string(),
                                    attempt: self.scheduled_retry_cycles.saturating_add(1),
                                    max_retries: 0,
                                    delay_ms: 0,
                                    failure_class: "context_window_exceeded".to_string(),
                                    message: "Input exceeded the model context window; compacting context and retrying".to_string(),
                                });
                                self.retry_status_visible = true;
                                let compacted = self
                                    .engine
                                    .force_persist_compaction_artifact(
                                        &self.tid,
                                        self.task_id,
                                        &self.config,
                                        &self.provider_config,
                                    )
                                    .await?;
                                if compacted {
                                    self.engine.clear_thread_continuation_state(&self.tid).await;
                                    self.recorded_compaction_provenance = true;
                                    return Err(FreshRunnerRetrySignal {
                                        scheduled_retry_cycles: 0,
                                    }
                                    .into());
                                }
                            }
                            if let Some(structured) = structured_failure.as_ref() {
                                let recovery = self
                                    .engine
                                    .maybe_recover_fixable_upstream_failure(
                                        &self.tid,
                                        structured,
                                        self.assistant_output_visible,
                                        self.tool_side_effect_committed,
                                        &mut self.attempted_recovery_signatures,
                                    )
                                    .await?;
                                if recovery.retry_attempted {
                                    return Ok(StreamIteration {
                                        prepared_request,
                                        llm_started_at,
                                        first_token_at,
                                        effective_transport_for_turn,
                                        accumulated_content,
                                        accumulated_reasoning,
                                        final_chunk: None,
                                        stream_timed_out: true,
                                        retry_loop: true,
                                    });
                                }
                                let notice_kind = match structured.class.as_str() {
                                    "transport_incompatible" => "transport-incompatible",
                                    "request_invalid" => "request-invalid",
                                    "temporary_upstream" => "temporary-upstream",
                                    _ => "upstream-error",
                                };
                                self.engine.emit_workflow_notice(
                                    &self.tid,
                                    notice_kind,
                                    structured.summary.clone(),
                                    Some(
                                        serde_json::json!({
                                            "provider": self.config.provider,
                                            "transport": prepared_request.transport,
                                            "class": structured.class,
                                            "diagnostics": structured.diagnostics,
                                        })
                                        .to_string(),
                                    ),
                                );
                            }
                            let structured_retry_after_ms = structured_failure.as_ref().and_then(|failure| {
                                failure
                                    .diagnostics
                                    .get("retry_after_ms")
                                    .and_then(|value| value.as_u64())
                            });
                            if !self
                                .engine
                                .provider_circuit_is_closed(&self.config.provider)
                                .await
                            {
                                self.engine
                                    .record_llm_outcome(&self.config.provider, false)
                                    .await;
                                return Err(self.fail_for_open_circuit().await);
                            }
                            let usage_limit_exhausted =
                                crate::agent::llm_client::is_exhausted_provider_usage_limit(&message);
                            if usage_limit_exhausted
                                && !self.engine.operator_stream_stop_requested(&self.tid).await
                            {
                                let delay_ms = if cfg!(test) {
                                    1
                                } else {
                                    usage_limit_retry_delay_ms(&message)
                                };
                                let attempt = self.scheduled_retry_cycles.saturating_add(1);
                                tracing::warn!(
                                    thread_id = %self.tid,
                                    provider = %self.config.provider,
                                    attempt,
                                    delay_ms,
                                    visible_message = %visible_message,
                                    "usage-limit retry parked so the operator can stop or retry without appending another error"
                                );
                                let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                    thread_id: self.tid.clone(),
                                    phase: "waiting".to_string(),
                                    attempt,
                                    max_retries: 0,
                                    delay_ms,
                                    failure_class: "rate_limit".to_string(),
                                    message: visible_message.clone(),
                                });
                                self.retry_status_visible = true;
                                self.scheduled_retry_cycles = attempt;
                                tokio::select! {
                                    biased;
                                    _ = self.stream_cancel_token.cancelled() => {
                                        self.was_cancelled = true;
                                        break;
                                    }
                                    _ = self.stream_retry_now.notified() => {
                                        if self.execution_profile_supersedes_runner().await {
                                            return Err(self.fresh_runner_retry_err());
                                        }
                                        return Ok(StreamIteration {
                                            prepared_request,
                                            llm_started_at,
                                            first_token_at,
                                            effective_transport_for_turn,
                                            accumulated_content,
                                            accumulated_reasoning,
                                            final_chunk: None,
                                            stream_timed_out: true,
                                            retry_loop: true,
                                        });
                                    }
                                    _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                                        if self.stream_cancel_token.is_cancelled()
                                            || self.engine.operator_stream_stop_requested(&self.tid).await
                                        {
                                            self.was_cancelled = true;
                                            break;
                                        }
                                        return Ok(StreamIteration {
                                            prepared_request,
                                            llm_started_at,
                                            first_token_at,
                                            effective_transport_for_turn,
                                            accumulated_content,
                                            accumulated_reasoning,
                                            final_chunk: None,
                                            stream_timed_out: true,
                                            retry_loop: true,
                                        });
                                    }
                                }
                            }
                            let structured_retryable = !usage_limit_exhausted
                                && structured_failure
                                    .as_ref()
                                    .map(|failure| {
                                        matches!(
                                            failure.class.as_str(),
                                            "rate_limit" | "temporary_upstream" | "transient_transport"
                                        )
                                    })
                                    .unwrap_or(false);
                            if provider_is_anthropic
                                && (structured_retryable || is_transient_retry_message(&message))
                            {
                                match self.retry_strategy {
                                    RetryStrategy::Bounded {
                                        max_retries,
                                        retry_delay_ms,
                                    } if self.scheduled_retry_cycles < max_retries => {
                                        let attempt = self.scheduled_retry_cycles.saturating_add(1);
                                        let delay_ms = structured_retry_after_ms.unwrap_or_else(|| {
                                            crate::agent::llm_client::compute_retry_delay_ms_for_attempt(
                                                retry_delay_ms,
                                                attempt,
                                            )
                                        });
                                        tracing::warn!(
                                            thread_id = %self.tid,
                                            provider = %self.config.provider,
                                            attempt,
                                            max_retries,
                                            delay_ms,
                                            failure_class = %retry_failure_class_from_message(&message),
                                            visible_message = %visible_message,
                                            "fresh-runner retry scheduled for anthropic request"
                                        );
                                        let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                            thread_id: self.tid.clone(),
                                            phase: "retrying".to_string(),
                                            attempt,
                                            max_retries,
                                            delay_ms,
                                            failure_class: retry_failure_class_from_message(&message).to_string(),
                                            message: visible_message.clone(),
                                        });
                                        self.retry_status_visible = true;
                                        self.scheduled_retry_cycles = attempt;
                                        tokio::select! {
                                            _ = self.stream_cancel_token.cancelled() => {
                                                self.was_cancelled = true;
                                                return Err(anyhow::anyhow!("fresh-runner retry cancelled"));
                                            }
                                            _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                                                return Err(
                                                    FreshRunnerRetrySignal {
                                                        scheduled_retry_cycles: attempt,
                                                    }
                                                    .into(),
                                                );
                                            }
                                        }
                                    }
                                    RetryStrategy::DurableRateLimited
                                        if structured_failure
                                            .as_ref()
                                            .map(|failure| failure.class == "rate_limit")
                                            .unwrap_or(false) =>
                                    {
                                        let attempt = self.scheduled_retry_cycles.saturating_add(1);
                                        let delay_ms = structured_retry_after_ms.unwrap_or_else(|| {
                                            crate::agent::llm_client::compute_retry_delay_ms_for_attempt(
                                                5_000,
                                                attempt,
                                            )
                                        });
                                        tracing::warn!(
                                            thread_id = %self.tid,
                                            provider = %self.config.provider,
                                            attempt,
                                            delay_ms,
                                            failure_class = %retry_failure_class_from_message(&message),
                                            visible_message = %visible_message,
                                            "durable fresh-runner retry scheduled for anthropic request"
                                        );
                                        let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                            thread_id: self.tid.clone(),
                                            phase: "retrying".to_string(),
                                            attempt,
                                            max_retries: 0,
                                            delay_ms,
                                            failure_class: retry_failure_class_from_message(&message).to_string(),
                                            message: visible_message.clone(),
                                        });
                                        self.retry_status_visible = true;
                                        self.scheduled_retry_cycles = attempt;
                                        tokio::select! {
                                            _ = self.stream_cancel_token.cancelled() => {
                                                self.was_cancelled = true;
                                                return Err(anyhow::anyhow!("fresh-runner retry cancelled"));
                                            }
                                            _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                                                return Err(
                                                    FreshRunnerRetrySignal {
                                                        scheduled_retry_cycles: attempt,
                                                    }
                                                    .into(),
                                                );
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            let live_auto_retry = self.engine.config.read().await.auto_retry;
                            if self.task_id.is_none()
                                && live_auto_retry
                                && !self.engine.operator_stream_stop_requested(&self.tid).await
                                && is_transient_retry_message(&message)
                            {
                                let delay_ms = if cfg!(test) {
                                    u64::from(self.config.retry_delay_ms).max(1)
                                } else {
                                    30_000u64
                                };
                                let attempt = self.scheduled_retry_cycles.saturating_add(1);
                                let max_retries = 0;
                                let promote_to_fresh_runner =
                                    !provider_is_anthropic
                                        && attempt >= MAX_OUTER_AUTO_RETRY_WAITS_BEFORE_FRESH_RUNNER;
                                tracing::warn!(
                                    thread_id = %self.tid,
                                    provider = %self.config.provider,
                                    attempt,
                                    delay_ms,
                                    failure_class = %retry_failure_class_from_message(&message),
                                    visible_message = %visible_message,
                                    "outer auto-retry wait scheduled after terminal llm error"
                                );
                                let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                    thread_id: self.tid.clone(),
                                    phase: "waiting".to_string(),
                                    attempt,
                                    max_retries,
                                    delay_ms,
                                    failure_class: retry_failure_class_from_message(&message).to_string(),
                                    message: visible_message.clone(),
                                });
                                self.retry_status_visible = true;
                                self.scheduled_retry_cycles = attempt;
                                tokio::select! {
                                    biased;
                                    _ = self.stream_cancel_token.cancelled() => {
                                        self.was_cancelled = true;
                                        break;
                                    }
                                    _ = self.stream_retry_now.notified() => {
                                        tracing::info!(
                                            thread_id = %self.tid,
                                            provider = %self.config.provider,
                                            attempt,
                                            "outer auto-retry resumed immediately by operator request"
                                        );
                                        let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
                                            thread_id: self.tid.clone(),
                                            phase: "retrying".to_string(),
                                            attempt,
                                            max_retries,
                                            delay_ms: 0,
                                            failure_class: retry_failure_class_from_message(&message).to_string(),
                                            message: visible_message.clone(),
                                        });
                                        if self.execution_profile_supersedes_runner().await
                                            || provider_is_anthropic
                                        {
                                            return Err(
                                                FreshRunnerRetrySignal {
                                                    scheduled_retry_cycles: attempt,
                                                }
                                                .into(),
                                            );
                                        }
                                        if promote_to_fresh_runner {
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
                                            return Err(
                                                FreshRunnerRetrySignal {
                                                    scheduled_retry_cycles: 0,
                                                }
                                                .into(),
                                            );
                                        }
                                        return Ok(StreamIteration {
                                            prepared_request,
                                            llm_started_at,
                                            first_token_at,
                                            effective_transport_for_turn,
                                            accumulated_content,
                                            accumulated_reasoning,
                                            final_chunk: None,
                                            stream_timed_out: true,
                                            retry_loop: true,
                                        });
                                    }
                                    _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                                        if self.stream_cancel_token.is_cancelled()
                                            || self.engine.operator_stream_stop_requested(&self.tid).await
                                            || !self.engine.config.read().await.auto_retry
                                        {
                                            self.was_cancelled = true;
                                            break;
                                        }
                                        tracing::info!(
                                            thread_id = %self.tid,
                                            provider = %self.config.provider,
                                            attempt,
                                            delay_ms,
                                            "outer auto-retry resumed after wait timer"
                                        );
                                        if self.execution_profile_supersedes_runner().await
                                            || provider_is_anthropic
                                        {
                                            return Err(
                                                FreshRunnerRetrySignal {
                                                    scheduled_retry_cycles: attempt,
                                                }
                                                .into(),
                                            );
                                        }
                                        if promote_to_fresh_runner {
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
                                            return Err(
                                                FreshRunnerRetrySignal {
                                                    scheduled_retry_cycles: 0,
                                                }
                                                .into(),
                                            );
                                        }
                                        return Ok(StreamIteration {
                                            prepared_request,
                                            llm_started_at,
                                            first_token_at,
                                            effective_transport_for_turn,
                                            accumulated_content,
                                            accumulated_reasoning,
                                            final_chunk: None,
                                            stream_timed_out: true,
                                            retry_loop: true,
                                        });
                                    }
                                }
                            }
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
                            self.engine.record_llm_outcome(&self.config.provider, false).await;
                            self.engine.add_assistant_message(
                                &self.tid,
                                &format!("Error: {visible_message}"),
                                0,
                                0,
                                None,
                                None,
                                None,
                                None,
                                None,
                            ).await;
                            self.engine.persist_thread_by_id(&self.tid).await;
                            self.engine.emit_turn_error_completion(
                                &self.tid,
                                &visible_message,
                                Some(self.config.provider.clone()),
                                Some(self.provider_config.model.clone()),
                            ).await;
                            self.engine.finish_stream_cancellation(&self.tid, self.stream_generation).await;
                            return Err(anyhow::anyhow!("LLM error: {message}"));
                        }
                    }
                }
            }
        }

        Ok(StreamIteration {
            prepared_request,
            llm_started_at,
            first_token_at,
            effective_transport_for_turn,
            accumulated_content,
            accumulated_reasoning,
            final_chunk,
            stream_timed_out,
            retry_loop: false,
        })
    }

    pub(super) async fn handle_stream_timeout(
        &mut self,
        iteration: &StreamIteration,
    ) -> Result<bool> {
        if iteration.retry_loop {
            return Ok(true);
        }
        const MAX_STREAM_TIMEOUTS: u32 = 3;
        if !iteration.stream_timed_out {
            return Ok(false);
        }
        self.stream_timeout_count += 1;
        if self.stream_timeout_count >= MAX_STREAM_TIMEOUTS && !self.config.auto_retry {
            let msg = format!(
                "Connection timed out {} times \u{2014} giving up. The provider may be overloaded.",
                self.stream_timeout_count
            );
            let _ = self.engine.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: self.tid.clone(),
                kind: "stream-timeout".to_string(),
                message: msg.clone(),
                details: None,
            });
            self.engine
                .finish_stream_cancellation(&self.tid, self.stream_generation)
                .await;
            return Err(anyhow::anyhow!("{msg}"));
        }
        let delay_ms = stream_timeout_retry_delay_ms(self.stream_timeout_count);
        let phase = if self.stream_timeout_count >= MAX_STREAM_TIMEOUTS {
            "waiting"
        } else {
            "retrying"
        };
        let waiting_timeout_count = self
            .stream_timeout_count
            .saturating_sub(MAX_STREAM_TIMEOUTS.saturating_sub(1));
        let promote_to_fresh_runner = self.stream_timeout_count >= MAX_STREAM_TIMEOUTS
            && waiting_timeout_count >= MAX_WAITING_TIMEOUTS_BEFORE_FRESH_RUNNER;
        let _ = self.engine.event_tx.send(AgentEvent::RetryStatus {
            thread_id: self.tid.clone(),
            phase: phase.to_string(),
            attempt: self.stream_timeout_count,
            max_retries: MAX_STREAM_TIMEOUTS,
            delay_ms,
            failure_class: "timeout".to_string(),
            message: "Connection timed out while waiting for streamed output".to_string(),
        });
        self.retry_status_visible = true;
        tokio::select! {
            _ = self.stream_cancel_token.cancelled() => {
                self.was_cancelled = true;
                Ok(false)
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                if promote_to_fresh_runner {
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
                    return Err(FreshRunnerRetrySignal {
                        scheduled_retry_cycles: 0,
                    }
                    .into());
                }
                Ok(true)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{inter_request_delay, DEFAULT_LLM_STREAM_CHUNK_TIMEOUT_SECS};
    use std::time::Duration;

    #[test]
    fn inter_request_delay_uses_configured_delay_after_first_loop() {
        assert_eq!(inter_request_delay(1, 500), None);
        assert_eq!(
            inter_request_delay(2, 500),
            Some(Duration::from_millis(500))
        );
        assert_eq!(inter_request_delay(2, 0), None);
    }

    #[test]
    fn default_stream_chunk_timeout_allows_long_first_token_latency() {
        assert_eq!(DEFAULT_LLM_STREAM_CHUNK_TIMEOUT_SECS, 300);
    }
}
