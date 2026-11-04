use super::*;

impl AgentEngine {
    pub(in crate::agent) async fn send_concierge_message_on_thread(
        &self,
        thread_id: &str,
        content: &str,
        preferred_session_hint: Option<&str>,
        operator_origin: bool,
        allow_escalation: bool,
    ) -> Result<()> {
        let (tid, _) = self.get_or_create_thread(Some(thread_id), content).await;
        let pinned = tid == crate::agent::concierge::CONCIERGE_THREAD_ID;
        let title = if pinned {
            "Concierge".to_string()
        } else {
            internal_dm_thread_title(CONCIERGE_AGENT_ID, MAIN_AGENT_ID)
        };
        self.ensure_thread_identity(&tid, &title, pinned).await;

        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                thread
                    .messages
                    .push(AgentMessage::user(content.trim(), now_millis()));
                thread.updated_at = now_millis();
            }
        }
        self.persist_thread_by_id(&tid).await;

        if operator_origin {
            self.record_operator_message(&tid, content, false).await?;
        }

        let (reply, reasoning) = if allow_escalation && concierge_should_escalate(content) {
            let result = Box::pin(self.send_internal_agent_message(
                CONCIERGE_AGENT_ID,
                MAIN_AGENT_ID,
                content,
                preferred_session_hint,
            ))
            .await?;
            let response = result.response;
            (
                format!("I checked with {}. {}", MAIN_AGENT_NAME, response.trim()),
                None,
            )
        } else {
            self.generate_concierge_reply(&tid).await?
        };

        let persisted_message_id = self
            .add_assistant_message(
                &tid,
                &reply,
                0,
                0,
                reasoning.clone(),
                Some("concierge".to_string()),
                None,
                None,
                None,
            )
            .await;
        let _ = self.event_tx.send(AgentEvent::Delta {
            thread_id: tid.clone(),
            content: reply.clone(),
        });
        let _ = self.event_tx.send(AgentEvent::Done {
            thread_id: tid,
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider: Some("concierge".to_string()),
            model: None,
            tps: None,
            generation_ms: None,
            reasoning,
            upstream_message: None,
            provider_final_result: None,
            message_id: persisted_message_id,
        });
        Ok(())
    }

    async fn generate_concierge_reply(&self, thread_id: &str) -> Result<(String, Option<String>)> {
        let config = self.config.read().await.clone();
        let provider_config = crate::agent::concierge::fast_concierge_provider_config(
            &crate::agent::concierge::resolve_concierge_provider(&config)?,
        );
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        let safe_tools =
            crate::agent::concierge::concierge_chat_safe_tools(&config, self.data_dir.as_path());
        drop(config);

        let memory_paths = memory_paths_for_scope(&self.data_dir, MAIN_AGENT_ID);
        let memory = self.memory_snapshot_for_scope(MAIN_AGENT_ID).await;
        let mut system_prompt = format!(
            "{}\n\n{} is the main builder agent. You are {}, the concierge. If you need deeper implementation work, summarize what {} should handle.\nPersistent memory files:\n- MEMORY.md: {}\n- SOUL.md: {}\n- USER.md: {}",
            crate::agent::concierge::concierge_system_prompt(),
            MAIN_AGENT_NAME,
            CONCIERGE_AGENT_NAME,
            MAIN_AGENT_NAME,
            memory_paths.memory_path.display(),
            memory_paths.soul_path.display(),
            memory_paths.user_path.display(),
        );
        if !safe_tools.is_empty() {
            system_prompt.push_str(
                "\n\nYou may call the provided read-only tools to search past sessions and history, look up memory, skills and guidelines, and search the web. Use them instead of guessing about prior context. Only call tools through the tool interface; never write tool calls as text. You have no other tools.",
            );
        }
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&build_concierge_runtime_identity_prompt(
            &provider_id,
            &provider_config.model,
        ));
        if !memory.soul.trim().is_empty() {
            system_prompt.push_str("\n\n## Identity Notes\n");
            system_prompt.push_str(memory.soul.trim());
        }
        if !memory.memory.trim().is_empty() {
            system_prompt.push_str("\n\n## Persistent Memory\n");
            system_prompt.push_str(memory.memory.trim());
        }
        if !memory.user_profile.trim().is_empty() {
            system_prompt.push_str("\n\n## Operator Profile\n");
            system_prompt.push_str(memory.user_profile.trim());
        }
        if is_internal_dm_thread(thread_id) {
            system_prompt.push_str(
                "\n\nThis thread is an internal agent-to-agent DM. Reply to another agent, not directly to the operator.",
            );
        }

        let mut messages = {
            let threads = self.threads.read().await;
            let history = threads
                .get(thread_id)
                .map(|thread| {
                    let start = thread.messages.len().saturating_sub(16);
                    thread.messages[start..].to_vec()
                })
                .unwrap_or_default();
            super::llm_client::messages_to_api_format(&history)
        };

        let max_rounds = crate::agent::concierge::CONCIERGE_CHAT_MAX_TOOL_ROUNDS;
        let mut full_content = String::new();
        let mut full_reasoning = String::new();
        for tool_round in 0..=max_rounds {
            let tools: &[ToolDefinition] = if tool_round < max_rounds {
                &safe_tools
            } else {
                &[]
            };
            let stream = send_completion_request(
                &self.http_client,
                &provider_id,
                &provider_config,
                &system_prompt,
                &messages,
                tools,
                provider_config.api_transport,
                None,
                None,
                RetryStrategy::Bounded {
                    max_retries: 1,
                    retry_delay_ms: 500,
                },
            );
            full_content.clear();
            full_reasoning.clear();
            let mut requested_tool_calls: Option<(Vec<ToolCall>, String)> = None;
            let mut stream = std::pin::pin!(stream);
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(CompletionChunk::Delta { content, reasoning }) => {
                        full_content.push_str(&content);
                        if let Some(reasoning) = reasoning {
                            full_reasoning.push_str(&reasoning);
                        }
                    }
                    Ok(CompletionChunk::Done {
                        content, reasoning, ..
                    }) => {
                        if !content.is_empty() {
                            full_content = content;
                        }
                        if let Some(reasoning) = reasoning.filter(|value| !value.is_empty()) {
                            full_reasoning = reasoning;
                        }
                        break;
                    }
                    Ok(CompletionChunk::ToolCalls {
                        tool_calls,
                        content,
                        ..
                    }) => {
                        requested_tool_calls =
                            Some((tool_calls, content.unwrap_or_else(|| full_content.clone())));
                        break;
                    }
                    Ok(CompletionChunk::Error { message }) => anyhow::bail!(message),
                    Err(error) => return Err(error),
                    Ok(_) => {}
                }
            }

            let Some((tool_calls, assistant_content)) = requested_tool_calls else {
                break;
            };
            messages.push(super::llm_client::ApiMessage {
                role: "assistant".into(),
                content: super::llm_client::ApiContent::Text(assistant_content),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: Some(crate::agent::concierge::api_tool_calls_from_tool_calls(
                    &tool_calls,
                )),
            });
            for tool_call in tool_calls {
                let (tool_call_id, name, content) =
                    if crate::agent::concierge::concierge_chat_tool_allowed(
                        &tool_call.function.name,
                    ) {
                        let result = crate::agent::execute_tool(
                            &tool_call,
                            self,
                            thread_id,
                            None,
                            &self.session_manager,
                            None,
                            &self.event_tx,
                            &self.data_dir,
                            &self.http_client,
                            None,
                        )
                        .await;
                        (result.tool_call_id, result.name, result.content)
                    } else {
                        (
                            tool_call.id.clone(),
                            tool_call.function.name.clone(),
                            format!(
                                "Tool '{}' is not available to the concierge.",
                                tool_call.function.name
                            ),
                        )
                    };
                messages.push(super::llm_client::ApiMessage {
                    role: "tool".into(),
                    content: super::llm_client::ApiContent::Text(content),
                    reasoning: None,
                    tool_call_id: Some(tool_call_id),
                    name: Some(name),
                    tool_calls: None,
                });
            }
        }
        let reasoning =
            (!full_reasoning.trim().is_empty()).then(|| full_reasoning.trim().to_string());
        Ok((full_content.trim().to_string(), reasoning))
    }
}
