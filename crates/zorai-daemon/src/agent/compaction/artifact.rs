use super::*;

#[derive(Debug)]
pub(crate) struct CompactionLlmFailureWithCapacity {
    pub(crate) strategy: CompactionStrategy,
    pub(crate) provider_id: String,
    pub(crate) model_window_tokens: usize,
    pub(crate) input_tokens: usize,
    pub(crate) source: anyhow::Error,
}

impl std::fmt::Display for CompactionLlmFailureWithCapacity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} compaction model `{}` (window={} tokens) failed for {}-token input; \
             refusing to fall back to Heuristic since the model had capacity. Cause: {}",
            self.strategy,
            self.provider_id,
            self.model_window_tokens,
            self.input_tokens,
            self.source,
        )
    }
}

impl std::error::Error for CompactionLlmFailureWithCapacity {}

fn weles_compaction_target_is_explicit(config: &AgentConfig) -> bool {
    !config.compaction.weles.provider.trim().is_empty()
        || !config.compaction.weles.model.trim().is_empty()
}

impl AgentEngine {
    pub(crate) async fn build_compaction_artifact(
        &self,
        thread_id: &str,
        messages: &[AgentMessage],
        target_tokens: usize,
        trigger: CompactionTrigger,
        pre_compaction_total_tokens: usize,
        effective_context_window_tokens: usize,
        config: &AgentConfig,
        structural_memory: Option<&ThreadStructuralMemory>,
        scope: Option<&CompactionScopeSnapshot>,
        mode: CompactionCandidateMode,
    ) -> Result<(AgentMessage, CompactionStrategy, Option<String>)> {
        let mut strategy_used = config.compaction.strategy;
        let mut fallback_notice = None;
        let mut structural_refs = Vec::new();
        let payload = match strategy_used {
            CompactionStrategy::Heuristic => {
                tracing::info!(
                    thread_id,
                    "compaction using heuristic strategy; no model request"
                );
                let rule_based = self
                    .build_rule_based_compaction_payload(
                        thread_id,
                        messages,
                        target_tokens,
                        structural_memory,
                        scope,
                    )
                    .await;
                structural_refs = rule_based.structural_refs;
                fallback_notice = rule_based.fallback_notice;
                rule_based.payload
            }
            CompactionStrategy::Weles => {
                let (provider_id, provider_config) = if weles_compaction_target_is_explicit(config)
                {
                    self.resolve_weles_compaction_provider(config)?
                } else if let Some(resolved) = self
                    .thread_profile_compaction_provider(thread_id, config)
                    .await?
                {
                    resolved
                } else {
                    self.resolve_weles_compaction_provider(config)?
                };
                tracing::info!(
                    thread_id,
                    provider_id,
                    model = %provider_config.model,
                    "compaction LLM request starting"
                );
                self.compact_with_llm_or_fallback(
                    CompactionStrategy::Weles,
                    "WELES",
                    &provider_id,
                    &provider_config,
                    thread_id,
                    messages,
                    target_tokens,
                    structural_memory,
                    scope,
                    &mut strategy_used,
                    &mut fallback_notice,
                    &mut structural_refs,
                    mode,
                )
                .await?
            }
            CompactionStrategy::CustomModel => {
                let (provider_id, provider_config) =
                    self.resolve_custom_model_compaction_provider(config)?;
                self.compact_with_llm_or_fallback(
                    CompactionStrategy::CustomModel,
                    "Custom-model",
                    &provider_id,
                    &provider_config,
                    thread_id,
                    messages,
                    target_tokens,
                    structural_memory,
                    scope,
                    &mut strategy_used,
                    &mut fallback_notice,
                    &mut structural_refs,
                    mode,
                )
                .await?
            }
        };

        let (payload, payload_was_capped) =
            fit_compaction_payload_to_budget(payload, target_tokens);
        if payload_was_capped {
            fallback_notice = merge_compaction_fallback_notice(
                fallback_notice,
                Some(
                    "Compaction checkpoint exceeded the continuity budget and was truncated."
                        .to_string(),
                ),
            );
        }

        let payload = self
            .append_previously_read_section(thread_id, payload, target_tokens)
            .await;

        let visible_content = build_compaction_visible_content(
            pre_compaction_total_tokens,
            effective_context_window_tokens,
            target_tokens,
            trigger,
            strategy_used,
        );

        Ok((
            AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: visible_content,
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
                message_kind: AgentMessageKind::CompactionArtifact,
                compaction_strategy: Some(strategy_used),
                compaction_payload: Some(payload),
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs,
                pinned_for_compaction: false,
                timestamp: now_millis(),
                feedback: None,
            },
            strategy_used,
            fallback_notice,
        ))
    }

    pub(crate) async fn build_rule_based_compaction_payload(
        &self,
        thread_id: &str,
        messages: &[AgentMessage],
        target_tokens: usize,
        structural_memory: Option<&ThreadStructuralMemory>,
        scope: Option<&CompactionScopeSnapshot>,
    ) -> RuleBasedCompactionPayload {
        let checkpoint_payload =
            build_checkpoint_compaction_payload_with_scope(messages, target_tokens, scope);

        if crate::agent::agent_identity::is_internal_dm_thread(thread_id)
            || crate::agent::agent_identity::is_participant_playground_thread(thread_id)
            || super::thread_handoffs::is_internal_handoff_thread(thread_id)
        {
            return RuleBasedCompactionPayload {
                payload: checkpoint_payload,
                structural_refs: Vec::new(),
                fallback_notice: None,
            };
        }

        match determine_rule_based_compaction_mode(structural_memory, messages) {
            RuleBasedCompactionMode::Conversational => RuleBasedCompactionPayload {
                payload: checkpoint_payload,
                structural_refs: Vec::new(),
                fallback_notice: None,
            },
            RuleBasedCompactionMode::Coding => {
                let Some(structural_memory) = structural_memory else {
                    return RuleBasedCompactionPayload {
                        payload: checkpoint_payload,
                        structural_refs: Vec::new(),
                        fallback_notice: None,
                    };
                };

                match self
                    .build_coding_compaction_payload(
                        thread_id,
                        messages,
                        target_tokens,
                        structural_memory,
                        scope,
                    )
                    .await
                {
                    Ok((payload, structural_refs)) => RuleBasedCompactionPayload {
                        payload,
                        structural_refs,
                        fallback_notice: None,
                    },
                    Err(error) => {
                        tracing::warn!(
                            thread_id = %thread_id,
                            %error,
                            "structured coding compaction assembly failed"
                        );
                        RuleBasedCompactionPayload {
                            payload: checkpoint_payload,
                            structural_refs: Vec::new(),
                            fallback_notice: Some(CODING_COMPACTION_FALLBACK_NOTICE.to_string()),
                        }
                    }
                }
            }
        }
    }

    pub(crate) async fn build_coding_compaction_payload(
        &self,
        thread_id: &str,
        messages: &[AgentMessage],
        target_tokens: usize,
        structural_memory: &ThreadStructuralMemory,
        scope: Option<&CompactionScopeSnapshot>,
    ) -> Result<(String, Vec<String>)> {
        let seed_structural_refs = collect_message_structural_refs(messages);
        let structural_entries = structural_memory.concise_context_entries(
            &seed_structural_refs,
            CODING_COMPACTION_STRUCTURAL_ENTRY_LIMIT,
        );
        if structural_entries.is_empty() {
            anyhow::bail!("no structural context entries available for coding compaction");
        }

        let graph_neighbors = load_memory_graph_neighbors(
            &self.history,
            &seed_structural_refs,
            CODING_COMPACTION_GRAPH_NEIGHBOR_LIMIT,
        )
        .await?;
        let merged_structural_entries = merge_structural_context_entries(
            &structural_entries,
            &graph_neighbors,
            CODING_COMPACTION_STRUCTURAL_ENTRY_LIMIT + CODING_COMPACTION_GRAPH_NEIGHBOR_LIMIT,
        );

        let offloaded_metadata =
            load_referenced_offloaded_payload_metadata(&self.history, thread_id, messages).await?;
        let structural_refs = merged_structural_entries
            .iter()
            .map(|entry| entry.node_id.clone())
            .collect::<Vec<_>>();
        let mut payload = format!(
            "## Primary Objective\n{}\n\n## Execution Map\n{}\n\n## Structural Context\n{}\n\n## Offloaded Payload References\n{}\n\n## Immediate Next Step\n{}\n",
            checkpoint_primary_objective_with_scope(messages, scope),
            coding_execution_map(messages),
            render_structural_context(&merged_structural_entries),
            render_offloaded_payload_references(&offloaded_metadata),
            checkpoint_immediate_next_step(messages),
        );
        payload.truncate(coding_compaction_payload_max_chars(target_tokens));

        Ok((payload, structural_refs))
    }

    #[allow(clippy::too_many_arguments)]
    async fn compact_with_llm_or_fallback(
        &self,
        strategy: CompactionStrategy,
        strategy_label: &'static str,
        provider_id: &str,
        provider_config: &ProviderConfig,
        thread_id: &str,
        messages: &[AgentMessage],
        target_tokens: usize,
        structural_memory: Option<&ThreadStructuralMemory>,
        scope: Option<&CompactionScopeSnapshot>,
        strategy_used: &mut CompactionStrategy,
        fallback_notice: &mut Option<String>,
        structural_refs: &mut Vec<String>,
        mode: CompactionCandidateMode,
    ) -> Result<String> {
        let llm_result = self
            .run_llm_compaction(provider_id, provider_config, messages, target_tokens, scope)
            .await;
        let failure_source: anyhow::Error = match llm_result {
            Ok(payload) if !payload.trim().is_empty() => {
                return Ok(ensure_payload_scope_markers(payload, scope));
            }
            Ok(_) => anyhow::anyhow!("{strategy_label} compaction returned an empty payload"),
            Err(error) => error,
        };

        let model_window_tokens = llm_compaction_input_budget(provider_id, provider_config);
        let input_tokens = estimate_message_tokens(messages);
        let model_had_capacity = input_tokens <= model_window_tokens;
        let mode_label = match mode {
            CompactionCandidateMode::Automatic => "auto",
            CompactionCandidateMode::Forced => "manual",
        };
        let fallback_reason = if model_had_capacity {
            format!(
                "{strategy_label} compaction failed despite model capacity (input {input_tokens} tokens, model window {model_window_tokens}); {mode_label} compaction fell back to rule based compaction. Cause: {failure_source}"
            )
        } else {
            format!(
                "{strategy_label} compaction failed (input {input_tokens} tokens > model window {model_window_tokens}); {mode_label} compaction fell back to rule based compaction."
            )
        };
        tracing::warn!(
            strategy = ?strategy,
            ?mode,
            provider_id,
            input_tokens,
            model_window_tokens,
            model_had_capacity,
            error = %failure_source,
            "compaction LLM call failed; falling back to heuristic"
        );
        *strategy_used = CompactionStrategy::Heuristic;
        let rule_based = self
            .build_rule_based_compaction_payload(
                thread_id,
                messages,
                target_tokens,
                structural_memory,
                scope,
            )
            .await;
        *structural_refs = rule_based.structural_refs;
        *fallback_notice =
            merge_compaction_fallback_notice(rule_based.fallback_notice, Some(fallback_reason));
        Ok(rule_based.payload)
    }

    async fn append_previously_read_section(
        &self,
        thread_id: &str,
        payload: String,
        target_tokens: usize,
    ) -> String {
        let budget_chars = target_tokens
            .saturating_mul(APPROX_CHARS_PER_TOKEN)
            .saturating_div(5);
        if budget_chars == 0 {
            return payload;
        }

        let skills = self
            .history
            .top_thread_skill_reads(thread_id, "skill", 3)
            .await
            .unwrap_or_default();
        let guidelines = self
            .history
            .top_thread_skill_reads(thread_id, "guideline", 3)
            .await
            .unwrap_or_default();
        if skills.is_empty() && guidelines.is_empty() {
            return payload;
        }

        let mut section = String::new();
        let mut remaining = budget_chars;
        let append_block = |section: &mut String,
                            header: &str,
                            items: &[crate::history::ThreadSkillRead],
                            remaining: &mut usize,
                            label: &str| {
            if items.is_empty() || *remaining == 0 {
                return;
            }
            section.push_str("\n\n");
            section.push_str(header);
            for item in items {
                let entry = format!("\n[{label}={}\n\ncontent={}]", item.name, item.content);
                let take = entry.chars().count().min(*remaining);
                if take == 0 {
                    break;
                }
                let truncated: String = entry.chars().take(take).collect();
                section.push_str(&truncated);
                *remaining = remaining.saturating_sub(take);
            }
        };
        append_block(
            &mut section,
            "Previously read skills:",
            &skills,
            &mut remaining,
            "skill",
        );
        append_block(
            &mut section,
            "Previously read guidelines:",
            &guidelines,
            &mut remaining,
            "guideline",
        );

        if section.is_empty() {
            payload
        } else {
            format!("{payload}{section}")
        }
    }

    pub(crate) async fn run_llm_compaction(
        &self,
        provider_id: &str,
        provider_config: &ProviderConfig,
        messages: &[AgentMessage],
        target_tokens: usize,
        scope: Option<&CompactionScopeSnapshot>,
    ) -> Result<String> {
        let transport = select_compaction_transport(provider_id, provider_config);
        let api_messages = build_llm_compaction_messages_with_scope(
            messages,
            target_tokens,
            llm_compaction_input_budget(provider_id, provider_config),
            scope,
        );
        self.check_circuit_breaker(provider_id).await?;

        let mut stream = send_completion_request(
            &self.http_client,
            provider_id,
            provider_config,
            COMPACTION_MODEL_SYSTEM_PROMPT,
            &api_messages,
            &[],
            transport,
            None,
            None,
            RetryStrategy::DurableRateLimited,
        );
        let mut content = String::new();
        let mut reasoning = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(value) => value,
                Err(error) => {
                    self.record_llm_outcome(provider_id, false).await;
                    return Err(error);
                }
            };
            match chunk {
                CompletionChunk::Delta {
                    content: delta,
                    reasoning: reasoning_delta,
                } => {
                    content.push_str(&delta);
                    if let Some(reasoning_delta) = reasoning_delta {
                        reasoning.push_str(&reasoning_delta);
                    }
                }
                CompletionChunk::Done {
                    content: done,
                    reasoning: done_reasoning,
                    ..
                } => {
                    self.record_llm_outcome(provider_id, true).await;
                    if let Some(done_reasoning) = done_reasoning {
                        reasoning = done_reasoning;
                    }
                    let final_content = if done.is_empty() { content } else { done };
                    let trimmed = final_content.trim();
                    if !trimmed.is_empty() {
                        return Ok(trimmed.to_string());
                    }
                    if !reasoning.trim().is_empty() {
                        return Ok(reasoning.trim().to_string());
                    }
                    anyhow::bail!("compaction LLM returned empty output");
                }
                CompletionChunk::Error { message } => {
                    self.record_llm_outcome(provider_id, false).await;
                    anyhow::bail!(message);
                }
                CompletionChunk::ToolCalls { .. } => {
                    self.record_llm_outcome(provider_id, true).await;
                    anyhow::bail!("compaction LLM unexpectedly returned tool calls");
                }
                CompletionChunk::TransportFallback { .. } | CompletionChunk::Retry { .. } => {}
            }
        }

        if !content.trim().is_empty() {
            return Ok(content.trim().to_string());
        }
        anyhow::bail!("compaction LLM returned empty output")
    }

    pub(crate) async fn thread_profile_compaction_provider(
        &self,
        thread_id: &str,
        config: &AgentConfig,
    ) -> Result<Option<(String, ProviderConfig)>> {
        let Some(profile) = self.get_thread_execution_profile(thread_id).await else {
            return Ok(None);
        };
        let provider_id = profile
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let model = profile
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if provider_id.is_none() && model.is_none() {
            return Ok(None);
        }
        let provider_id = provider_id.unwrap_or_else(|| config.provider.clone());
        let mut provider_config =
            resolve_provider_config_for(config, &provider_id, model.as_deref())?;
        if let Some(effort) = profile
            .reasoning_effort
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            provider_config.reasoning_effort = effort.to_string();
        }
        if let Some(window) = profile.context_window_tokens.filter(|window| *window > 0) {
            provider_config.context_window_tokens = window;
        }
        Ok(Some((provider_id, provider_config)))
    }

    pub(crate) fn resolve_weles_compaction_provider(
        &self,
        config: &AgentConfig,
    ) -> Result<(String, ProviderConfig)> {
        let provider_id = config.compaction.weles.provider.trim().to_string();
        let provider_id = if provider_id.is_empty() {
            config
                .builtin_sub_agents
                .weles
                .provider
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| config.provider.clone())
        } else {
            provider_id
        };
        let model = config.compaction.weles.model.trim().to_string();
        let model = if model.is_empty() {
            config
                .builtin_sub_agents
                .weles
                .model
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| config.model.clone())
        } else {
            model
        };
        let reasoning_effort = config.compaction.weles.reasoning_effort.trim().to_string();
        let reasoning_effort = if reasoning_effort.is_empty() {
            config
                .builtin_sub_agents
                .weles
                .reasoning_effort
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "medium".to_string())
        } else {
            reasoning_effort
        };
        let mut provider_config =
            resolve_provider_config_for(config, &provider_id, Some(model.as_str()))?;
        let api_transport = config
            .compaction
            .weles
            .api_transport
            .or(config.builtin_sub_agents.weles.api_transport);
        crate::agent::provider_resolution::apply_role_transport_override(
            &provider_id,
            &mut provider_config,
            api_transport,
        );
        provider_config.reasoning_effort = reasoning_effort;
        provider_config.response_schema = None;
        Ok((provider_id, provider_config))
    }

    pub(crate) fn resolve_custom_model_compaction_provider(
        &self,
        config: &AgentConfig,
    ) -> Result<(String, ProviderConfig)> {
        let custom = &config.compaction.custom_model;
        let mut runtime_config = config.clone();
        runtime_config.providers.clear();
        if !custom.provider.trim().is_empty() {
            runtime_config.provider = custom.provider.trim().to_string();
        }
        if !custom.base_url.trim().is_empty() {
            runtime_config.base_url = custom.base_url.trim().to_string();
        }
        if !custom.model.trim().is_empty() {
            runtime_config.model = custom.model.trim().to_string();
        }
        if !custom.api_key.trim().is_empty() {
            runtime_config.api_key = custom.api_key.clone();
        }
        if !custom.assistant_id.trim().is_empty() {
            runtime_config.assistant_id = custom.assistant_id.clone();
        }
        runtime_config.auth_source = custom.auth_source;
        runtime_config.api_transport = custom.api_transport;
        if !custom.reasoning_effort.trim().is_empty() {
            runtime_config.reasoning_effort = custom.reasoning_effort.clone();
        }
        if custom.context_window_tokens > 0 {
            runtime_config.context_window_tokens = custom.context_window_tokens;
        }

        let provider_id = runtime_config.provider.trim().to_string();
        if provider_id.is_empty() {
            anyhow::bail!("custom compaction provider is not configured");
        }
        let model = runtime_config.model.trim().to_string();
        if model.is_empty() {
            anyhow::bail!("custom compaction model is not configured");
        }

        let mut provider_config =
            resolve_provider_config_for(&runtime_config, &provider_id, Some(model.as_str()))?;
        provider_config.response_schema = None;
        Ok((provider_id, provider_config))
    }
}
