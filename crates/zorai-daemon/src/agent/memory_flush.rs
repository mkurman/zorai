//! Pre-compaction memory flush for durable facts before older context is summarized away.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use zorai_shared::providers::PROVIDER_ID_OPENAI;

use futures::StreamExt;
use zorai_protocol::SessionId;

use super::llm_client::{messages_to_api_format, ApiContent, ApiMessage};
use super::tool_executor::get_memory_flush_tools;
use super::*;

const MEMORY_FLUSH_EXACT_MESSAGE_MAX: usize = 24;

impl AgentEngine {
    pub(super) async fn maybe_run_pre_compaction_memory_flush(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
        system_prompt: &str,
        preferred_session_id: Option<SessionId>,
        retry_strategy: RetryStrategy,
        last_flushed_signature: &mut Option<u64>,
    ) -> Result<bool> {
        let Some((older_messages, target_tokens)) = self
            .pre_compaction_memory_flush_input(thread_id, config, provider_config)
            .await
        else {
            return Ok(false);
        };

        let flush_signature = memory_flush_signature(&older_messages);
        if last_flushed_signature.as_ref() == Some(&flush_signature) {
            return Ok(false);
        }

        *last_flushed_signature = Some(flush_signature);
        let flush_prompt = build_memory_flush_prompt(system_prompt);
        let flush_messages = build_memory_flush_messages(&older_messages, target_tokens);
        let flush_tools = get_memory_flush_tools();
        let flush_transport = select_memory_flush_transport(config, provider_config);

        self.emit_workflow_notice(
            thread_id,
            "memory-flush-check",
            "Reviewing older context before compaction to preserve durable memory.",
            Some(format!(
                "older_messages={}; target_tokens={target_tokens}",
                older_messages.len()
            )),
        );

        if let Err(e) = self.check_circuit_breaker(&config.provider).await {
            tracing::warn!(thread_id = %thread_id, "memory flush skipped — circuit breaker open: {e}");
            return Ok(false);
        }

        let mut stream = send_completion_request(
            &self.http_client,
            &config.provider,
            provider_config,
            &flush_prompt,
            &flush_messages,
            &flush_tools,
            flush_transport,
            None,
            None,
            retry_strategy,
        );

        let mut pending_tool_calls = Vec::new();
        let mut completed = false;

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(chunk) => chunk,
                Err(error) => {
                    self.record_llm_outcome(&config.provider, false).await;
                    tracing::warn!(thread_id = %thread_id, "pre-compaction memory flush failed: {error}");
                    self.emit_workflow_notice(
                        thread_id,
                        "memory-flush-error",
                        "Pre-compaction memory flush failed; continuing without memory changes.",
                        Some(error.to_string()),
                    );
                    return Ok(false);
                }
            };

            match chunk {
                CompletionChunk::ToolCalls { tool_calls, .. } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    pending_tool_calls = tool_calls;
                    completed = true;
                    break;
                }
                CompletionChunk::Done { .. } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    completed = true;
                    break;
                }
                CompletionChunk::TransportFallback { .. }
                | CompletionChunk::Retry { .. }
                | CompletionChunk::Delta { .. } => {}
                CompletionChunk::Error { message } => {
                    self.record_llm_outcome(&config.provider, false).await;
                    tracing::warn!(thread_id = %thread_id, "pre-compaction memory flush returned error: {message}");
                    self.emit_workflow_notice(
                        thread_id,
                        "memory-flush-error",
                        "Pre-compaction memory flush returned an LLM error; continuing without memory changes.",
                        Some(message),
                    );
                    return Ok(false);
                }
            }
        }

        if !completed || pending_tool_calls.is_empty() {
            return Ok(false);
        }

        let mut memory_updated = false;
        for tool_call in &pending_tool_calls {
            if tool_call.function.name != zorai_protocol::tool_names::UPDATE_MEMORY {
                tracing::warn!(
                    thread_id = %thread_id,
                    tool = %tool_call.function.name,
                    "unexpected tool returned from pre-compaction memory flush"
                );
                continue;
            }

            let result = execute_tool(
                tool_call,
                self,
                thread_id,
                task_id,
                &self.session_manager,
                preferred_session_id,
                &self.event_tx,
                &self.data_dir,
                &self.http_client,
                None,
            )
            .await;

            if result.is_error {
                tracing::warn!(
                    thread_id = %thread_id,
                    tool = %result.name,
                    "pre-compaction memory update failed: {}",
                    result.content
                );
                self.emit_workflow_notice(
                    thread_id,
                    "memory-flush-error",
                    "Memory flush attempted an invalid durable-memory update.",
                    Some(result.content),
                );
                continue;
            }

            memory_updated = true;
        }

        if memory_updated {
            self.refresh_memory_cache().await;
            self.emit_workflow_notice(
                thread_id,
                "memory-flushed",
                "Durable memory was updated before context compaction.",
                Some(format!("tool_calls={}", pending_tool_calls.len())),
            );
        }

        Ok(memory_updated)
    }

    async fn pre_compaction_memory_flush_input(
        &self,
        thread_id: &str,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
    ) -> Option<(Vec<AgentMessage>, usize)> {
        if crate::agent::agent_identity::is_internal_dm_thread(thread_id)
            || crate::agent::agent_identity::is_participant_playground_thread(thread_id)
            || super::thread_handoffs::is_internal_handoff_thread(thread_id)
        {
            return None;
        }

        let threads = self.threads.read().await;
        let thread = threads.get(thread_id)?;
        let (window_start, _) = active_compaction_window(&thread.messages);
        let candidate = compaction_candidate(&thread.messages, config, provider_config)?;
        Some((
            thread.messages[window_start..window_start + candidate.split_at].to_vec(),
            candidate.target_tokens,
        ))
    }
}

fn select_memory_flush_transport(
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> ApiTransport {
    if config.provider == PROVIDER_ID_OPENAI
        && provider_config.auth_source == AuthSource::ChatgptSubscription
    {
        return ApiTransport::Responses;
    }

    let selected = if provider_supports_transport(&config.provider, provider_config.api_transport) {
        provider_config.api_transport
    } else {
        default_api_transport_for_provider(&config.provider)
    };

    if let Some(fixed_transport) =
        fixed_api_transport_for_model(&config.provider, &provider_config.model)
    {
        return fixed_transport;
    }

    match selected {
        ApiTransport::NativeAssistant => {
            if provider_supports_transport(&config.provider, ApiTransport::Responses) {
                ApiTransport::Responses
            } else {
                ApiTransport::ChatCompletions
            }
        }
        other => other,
    }
}

fn build_memory_flush_prompt(system_prompt: &str) -> String {
    format!(
        "{system_prompt}\n\n## Pre-Compaction Memory Flush\nBefore older context is compacted away, review only the supplied older context and decide whether any durable memory should be preserved.\n- The only allowed tool is `update_memory`.\n- Save only stable user preferences, stable workspace facts, or durable recurring corrections.\n- Never save task progress, transient errors, one-off outputs, or anything that can be trivially rediscovered.\n- If nothing qualifies, do not call any tool and reply with `NO_MEMORY_UPDATE`.\n- Do not fabricate facts.\n\n{}",
        memory_curation_guidance()
    )
}

fn build_memory_flush_messages(messages: &[AgentMessage], target_tokens: usize) -> Vec<ApiMessage> {
    let use_exact_messages = messages.len() <= MEMORY_FLUSH_EXACT_MESSAGE_MAX
        && estimate_message_tokens(messages) <= target_tokens.saturating_div(2).max(512);

    let mut api_messages = if use_exact_messages {
        messages_to_api_format(messages)
    } else {
        vec![ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text(format!(
                "This older context is about to be compacted:\n\n{}",
                build_compaction_summary(messages, target_tokens)
            )),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }]
    };

    api_messages.push(ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text(
            "Persist durable memory only if it genuinely belongs in SOUL.md, MEMORY.md, or USER.md. Otherwise reply NO_MEMORY_UPDATE."
                .to_string(),
        ),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });
    api_messages
}

fn memory_flush_signature(messages: &[AgentMessage]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for message in messages {
        match message.role {
            MessageRole::System => 0u8,
            MessageRole::User => 1u8,
            MessageRole::Assistant => 2u8,
            MessageRole::Tool => 3u8,
        }
        .hash(&mut hasher);
        message.content.hash(&mut hasher);
        message.tool_name.hash(&mut hasher);
        message.tool_arguments.hash(&mut hasher);
        message.tool_status.hash(&mut hasher);
        message.timestamp.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    fn sample_provider_config() -> ProviderConfig {
        ProviderConfig {
            presence_penalty: None,
            base_url: "https://example.invalid".to_string(),
            model: "test-model".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "low".to_string(),
            context_window_tokens: 128_000,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        }
    }

    fn sample_message(content: &str) -> AgentMessage {
        AgentMessage::user(content, 1)
    }

    #[test]
    fn memory_flush_signature_changes_when_messages_change() {
        let left = vec![sample_message("a")];
        let right = vec![sample_message("b")];

        assert_ne!(
            memory_flush_signature(&left),
            memory_flush_signature(&right)
        );
    }

    #[test]
    fn memory_flush_messages_add_a_final_instruction_message() {
        let messages = vec![sample_message("remember this preference")];
        let api_messages = build_memory_flush_messages(&messages, 4096);

        assert!(!api_messages.is_empty());
        assert_eq!(
            api_messages.last().map(|msg| msg.role.as_str()),
            Some("user")
        );
    }

    #[tokio::test]
    async fn internal_dm_thread_skips_pre_compaction_memory_flush_input() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.auto_compact_context = true;
        config.max_context_messages = 2;
        config.keep_recent_on_compact = 1;
        let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
        let provider = sample_provider_config();
        let thread_id = crate::agent::agent_identity::internal_dm_thread_id(
            crate::agent::agent_identity::MAIN_AGENT_ID,
            crate::agent::agent_identity::WELES_AGENT_ID,
        );

        engine.threads.write().await.insert(
            thread_id.clone(),
            AgentThread {
                id: thread_id.clone(),
                agent_name: None,
                title: "Internal DM".to_string(),
                messages: vec![
                    AgentMessage::user("one", 1),
                    AgentMessage::user("two", 2),
                    AgentMessage::user("three", 3),
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at: 1,
                updated_at: 3,
                total_input_tokens: 0,
                total_output_tokens: 0,
            },
        );

        let input = engine
            .pre_compaction_memory_flush_input(&thread_id, &config, &provider)
            .await;

        assert!(input.is_none());
    }
}
