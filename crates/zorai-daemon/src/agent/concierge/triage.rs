use super::*;

impl ConciergeEngine {
    pub async fn triage_gateway_message(
        &self,
        agent: &super::super::AgentEngine,
        platform: &str,
        sender: &str,
        content: &str,
        recent_channel_history: Option<&str>,
        gateway_thread_id: Option<&str>,
        _threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        _tasks: &tokio::sync::Mutex<std::collections::VecDeque<AgentTask>>,
    ) -> GatewayTriage {
        let config = self.config.read().await;
        if !config.concierge.enabled {
            return GatewayTriage::Complex;
        }
        let provider_config = match resolve_concierge_provider(&config) {
            Ok(pc) => fast_concierge_provider_config(&pc),
            Err(e) => {
                tracing::warn!("concierge: triage provider resolution failed: {e}");
                return GatewayTriage::Complex;
            }
        };
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        let safe_tools = gateway_triage_safe_tools(
            &config,
            agent.data_dir.as_path(),
            gateway_thread_id.is_some(),
        );
        drop(config);

        let recent_history_threads = self
            .recent_persisted_history_threads(&agent.session_manager)
            .await;
        let context = self
            .gather_gateway_context(&agent.history, &recent_history_threads)
            .await;
        let user_prompt = build_gateway_triage_prompt(
            platform,
            sender,
            content,
            recent_channel_history,
            &context,
        );

        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(user_prompt),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        if let Err(e) = self.check_circuit_breaker(&provider_id).await {
            tracing::warn!("concierge: triage skipped — circuit breaker open: {e}");
            return GatewayTriage::Complex;
        }

        let system_prompt = format!(
            "{}\n\n{}",
            gateway_triage_system_prompt(),
            super::super::build_concierge_runtime_identity_prompt(
                &provider_id,
                &provider_config.model,
            )
        );

        let mut messages = messages;
        for tool_round in 0..=GATEWAY_TRIAGE_MAX_TOOL_ROUNDS {
            let stream = llm_client::send_completion_request(
                &self.http_client,
                &provider_id,
                &provider_config,
                &system_prompt,
                &messages,
                &safe_tools,
                provider_config.api_transport,
                None,
                None,
                RetryStrategy::Bounded {
                    max_retries: 1,
                    retry_delay_ms: 500,
                },
            );

            let mut full_content = String::new();
            let mut requested_tool_calls: Option<(Vec<ToolCall>, String)> = None;
            let mut stream = std::pin::pin!(stream);
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(CompletionChunk::Delta { content, .. }) => full_content.push_str(&content),
                    Ok(CompletionChunk::Done { content, .. }) => {
                        self.record_llm_outcome(&provider_id, true).await;
                        if !content.is_empty() {
                            full_content = content;
                        }
                        break;
                    }
                    Ok(CompletionChunk::ToolCalls {
                        tool_calls,
                        content,
                        ..
                    }) => {
                        self.record_llm_outcome(&provider_id, true).await;
                        requested_tool_calls =
                            Some((tool_calls, content.unwrap_or_else(|| full_content.clone())));
                        break;
                    }
                    Ok(CompletionChunk::Error { message }) => {
                        self.record_llm_outcome(&provider_id, false).await;
                        tracing::warn!("concierge: triage LLM error: {message}");
                        return GatewayTriage::Complex;
                    }
                    Err(e) => {
                        self.record_llm_outcome(&provider_id, false).await;
                        tracing::warn!("concierge: triage stream error: {e}");
                        return GatewayTriage::Complex;
                    }
                    Ok(_) => {}
                }
            }

            if let Some((tool_calls, assistant_content)) = requested_tool_calls {
                if tool_round >= GATEWAY_TRIAGE_MAX_TOOL_ROUNDS {
                    tracing::warn!(
                        platform = %platform,
                        "concierge: triage exhausted safe tool rounds, routing to agent"
                    );
                    return GatewayTriage::Complex;
                }

                messages.push(ApiMessage {
                    role: "assistant".into(),
                    content: ApiContent::Text(assistant_content),
                    reasoning: None,
                    tool_call_id: None,
                    name: None,
                    tool_calls: Some(api_tool_calls_from_tool_calls(&tool_calls)),
                });

                for tool_call in tool_calls {
                    if !gateway_triage_tool_allowed(&tool_call.function.name)
                        || (tool_call.function.name
                            == zorai_protocol::tool_names::FETCH_GATEWAY_HISTORY
                            && gateway_thread_id.is_none())
                    {
                        tracing::info!(
                            platform = %platform,
                            tool = %tool_call.function.name,
                            "concierge: triage requested disallowed tool, routing to agent"
                        );
                        return GatewayTriage::Complex;
                    }

                    let result = execute_tool(
                        &tool_call,
                        agent,
                        gateway_thread_id.unwrap_or(""),
                        None,
                        &agent.session_manager,
                        None,
                        &self.event_tx,
                        &agent.data_dir,
                        &self.http_client,
                        None,
                    )
                    .await;

                    messages.push(ApiMessage {
                        role: "tool".into(),
                        content: ApiContent::Text(result.content),
                        reasoning: None,
                        tool_call_id: Some(result.tool_call_id),
                        name: Some(result.name),
                        tool_calls: None,
                    });
                }

                continue;
            }

            let trimmed = full_content.trim();
            if trimmed.starts_with("[SIMPLE]") {
                let response = trimmed.trim_start_matches("[SIMPLE]").trim().to_string();
                if response.is_empty() {
                    return GatewayTriage::Complex;
                }
                return GatewayTriage::Simple(response);
            } else {
                return GatewayTriage::Complex;
            }
        }

        GatewayTriage::Complex
    }

    pub(super) async fn check_circuit_breaker(&self, provider: &str) -> Result<()> {
        let breaker_arc = self.circuit_breakers.get(provider).await;
        let mut breaker = breaker_arc.lock().await;
        let now = super::super::now_millis();

        if !breaker.can_execute(now) {
            let trip_count = breaker.trip_count();
            drop(breaker);
            let outage = super::super::engine::collect_provider_outage_metadata(
                &self.config,
                &self.circuit_breakers,
                provider,
                trip_count,
                "circuit breaker open",
            )
            .await;
            let _ = self.event_tx.send(AgentEvent::ProviderCircuitOpen {
                provider: outage.provider,
                failed_model: outage.failed_model,
                trip_count: outage.trip_count,
                reason: outage.reason,
                suggested_alternatives: outage.suggested_alternatives,
            });
            anyhow::bail!(
                "Circuit breaker open for provider '{}' — requests blocked for ~30s",
                provider
            );
        }
        Ok(())
    }

    pub(super) async fn record_llm_outcome(&self, provider: &str, success: bool) {
        use super::super::circuit_breaker::CircuitState;

        let breaker_arc = self.circuit_breakers.get(provider).await;
        let mut breaker = breaker_arc.lock().await;
        let now = super::super::now_millis();

        if success {
            let was_half_open = breaker.state() == CircuitState::HalfOpen;
            breaker.record_success(now);
            if was_half_open && breaker.state() == CircuitState::Closed {
                let _ = self.event_tx.send(AgentEvent::ProviderCircuitRecovered {
                    provider: provider.to_string(),
                });
            }
        } else {
            let was_closed_or_half = breaker.state() != CircuitState::Open;
            breaker.record_failure(now);
            if was_closed_or_half && breaker.state() == CircuitState::Open {
                let trip_count = breaker.trip_count();
                drop(breaker);
                let outage = super::super::engine::collect_provider_outage_metadata(
                    &self.config,
                    &self.circuit_breakers,
                    provider,
                    trip_count,
                    "circuit breaker tripped",
                )
                .await;
                let _ = self.event_tx.send(AgentEvent::ProviderCircuitOpen {
                    provider: outage.provider,
                    failed_model: outage.failed_model,
                    trip_count: outage.trip_count,
                    reason: outage.reason,
                    suggested_alternatives: outage.suggested_alternatives,
                });
            }
        }
    }
}

fn gateway_triage_system_prompt() -> String {
    format!(
        "You are {}, {}'s concierge triage agent, operating in zorai. You receive messages from external platforms \
         (Slack, Discord, Telegram, WhatsApp) and decide whether to handle them yourself or \
         route them to the full agent.\n\n\
         SIMPLE messages (handle yourself): greetings, casual chat, status inquiries, \
         quick factual lookups, acknowledgments, scheduling questions, thank-yous.\n\
         You may use the provided safe read-only tools for history lookup, memory lookup, local skill lookup, and web search \
         when they help you answer naturally and preserve conversation continuity.\n\
         COMPLEX messages (route to agent): code requests, file operations, debugging, \
         multi-step tasks, technical analysis, project work, or anything needing tools beyond the provided safe set.\n\n\
         If the user asks about prior context you do not already have, use the safe tools first instead of saying you cannot access it.\n\
         If the user asks specifically about {} or asks you to check with {} instead of answering from your own perspective, respond [COMPLEX] so {} can answer.\n\
         If SIMPLE: respond with [SIMPLE] followed by your concise, friendly reply.\n\
         If COMPLEX: respond with just [COMPLEX].\n\
         Be fast. Keep simple replies concise and natural. Never hallucinate tool usage.",
        CONCIERGE_AGENT_NAME,
        MAIN_AGENT_NAME,
        MAIN_AGENT_NAME,
        MAIN_AGENT_NAME,
        MAIN_AGENT_NAME,
    )
}

pub(super) fn build_gateway_triage_prompt(
    platform: &str,
    sender: &str,
    content: &str,
    recent_channel_history: Option<&str>,
    context: &WelcomeContext,
) -> String {
    let mut prompt = format!("[{platform} message from {sender}]: {content}\n");
    if let Some(history) = recent_channel_history.filter(|history| !history.trim().is_empty()) {
        prompt.push_str(&format!(
            "\nRecent messages from this same channel:\n{history}\n"
        ));
    }
    if let Some(last) = context.recent_threads.first() {
        prompt.push_str(&format!(
            "\nContext: Last session was \"{}\" ({}).",
            last.title,
            format_timestamp(last.updated_at),
        ));
    }
    if context.running_goal_total > 0 || context.paused_goal_total > 0 {
        prompt.push_str(&format!(
            " {} running goals, {} paused goals.",
            context.running_goal_total, context.paused_goal_total
        ));
    }
    prompt
}

fn gateway_triage_tool_allowed(name: &str) -> bool {
    GATEWAY_TRIAGE_SAFE_TOOL_NAMES.contains(&name)
}

pub(super) fn gateway_triage_safe_tools(
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    allow_gateway_history: bool,
) -> Vec<ToolDefinition> {
    get_available_tools(config, agent_data_dir, false)
        .into_iter()
        .filter(|tool| {
            let name = tool.function.name.as_str();
            gateway_triage_tool_allowed(name)
                && (allow_gateway_history
                    || name != zorai_protocol::tool_names::FETCH_GATEWAY_HISTORY)
        })
        .collect()
}

pub(in crate::agent) fn concierge_chat_tool_allowed(name: &str) -> bool {
    gateway_triage_tool_allowed(name)
        && name != zorai_protocol::tool_names::FETCH_GATEWAY_HISTORY
        && name != zorai_protocol::tool_names::MESSAGE_AGENT
}

pub(in crate::agent) fn concierge_chat_safe_tools(
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
) -> Vec<ToolDefinition> {
    gateway_triage_safe_tools(config, agent_data_dir, false)
        .into_iter()
        .filter(|tool| concierge_chat_tool_allowed(&tool.function.name))
        .collect()
}

pub(in crate::agent) fn api_tool_calls_from_tool_calls(
    tool_calls: &[ToolCall],
) -> Vec<ApiToolCall> {
    tool_calls
        .iter()
        .map(|tool_call| ApiToolCall {
            id: tool_call.id.clone(),
            call_type: "function".to_string(),
            function: ApiToolCallFunction {
                name: tool_call.function.name.clone(),
                arguments: tool_call.function.arguments.clone(),
            },
        })
        .collect()
}
