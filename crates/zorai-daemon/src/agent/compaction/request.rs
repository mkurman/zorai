use super::*;

pub(crate) fn prepare_llm_request(
    thread: &AgentThread,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> PreparedLlmRequest {
    prepare_llm_request_with_reused_user_message(thread, config, provider_config, None)
}

pub(crate) fn prepare_llm_request_with_reused_user_message(
    thread: &AgentThread,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
    reused_user_message: Option<&str>,
) -> PreparedLlmRequest {
    let uses_chatgpt_subscription_responses = config.provider == PROVIDER_ID_OPENAI
        && provider_config.auth_source == crate::agent::types::AuthSource::ChatgptSubscription;
    let mut selected_transport =
        if provider_supports_transport(&config.provider, provider_config.api_transport) {
            provider_config.api_transport
        } else {
            default_api_transport_for_provider(&config.provider)
        };
    if uses_chatgpt_subscription_responses {
        selected_transport = ApiTransport::Responses;
    }
    if let Some(fixed_transport) =
        fixed_api_transport_for_model(&config.provider, &provider_config.model)
    {
        selected_transport = fixed_transport;
    }
    let messages = &thread.messages;
    let compacted = compact_messages_for_request(messages, config, provider_config);
    let compaction_active =
        compacted.len() != messages.len() || compacted.iter().any(message_is_compaction_summary);
    let request_messages = if compacted.iter().any(message_is_compaction_summary) {
        append_owner_only_pins_after_artifact(
            &compacted,
            owner_only_pins_within_budget(thread, config, provider_config),
        )
    } else {
        compacted.clone()
    };

    let resumes_local_cli_session =
        crate::agent::types::provider_uses_local_subscription_cli(&config.provider);
    if selected_transport == ApiTransport::NativeAssistant
        && (resumes_local_cli_session
            || (!compaction_active && !provider_config.assistant_id.trim().is_empty()))
    {
        let latest_user_message = messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .cloned();
        if let Some(user_message) = latest_user_message {
            let assistant_id_matches = resumes_local_cli_session
                || thread.upstream_assistant_id.as_deref()
                    == Some(provider_config.assistant_id.as_str());
            return PreparedLlmRequest {
                messages: messages_to_api_format(&[user_message]),
                transport: ApiTransport::NativeAssistant,
                previous_response_id: None,
                upstream_thread_id: if thread.upstream_transport
                    == Some(ApiTransport::NativeAssistant)
                    && thread.upstream_provider.as_deref() == Some(config.provider.as_str())
                    && thread.upstream_model.as_deref() == Some(provider_config.model.as_str())
                    && assistant_id_matches
                {
                    thread.upstream_thread_id.clone()
                } else {
                    None
                },
                force_connection_close: false,
            };
        }
    }

    if selected_transport == ApiTransport::Responses {
        let previous_response_id = if !uses_chatgpt_subscription_responses
            && supports_response_continuity(&config.provider)
        {
            request_messages
                .iter()
                .enumerate()
                .rev()
                .find(|(_, message)| {
                    message.role == MessageRole::Assistant
                        && message.response_id.is_some()
                        && message.provider.as_deref() == Some(config.provider.as_str())
                        && message.model.as_deref() == Some(provider_config.model.as_str())
                        && message.api_transport == Some(ApiTransport::Responses)
                })
                .and_then(|(anchor_index, anchor_message)| {
                    let trailing = &request_messages[anchor_index + 1..];
                    if config.provider == PROVIDER_ID_GITHUB_COPILOT
                        && trailing.iter().any(|message| {
                            message.role == MessageRole::Tool
                                || message
                                    .tool_calls
                                    .as_ref()
                                    .is_some_and(|tool_calls| !tool_calls.is_empty())
                        })
                    {
                        return None;
                    }
                    let trailing_messages =
                        continuation_api_messages(trailing, reused_user_message);
                    if trailing_messages.is_empty() {
                        None
                    } else {
                        Some((trailing_messages, anchor_message.response_id.clone()))
                    }
                })
        } else {
            None
        };

        if let Some((messages, previous_response_id)) = previous_response_id {
            let mut messages = messages;
            inject_reused_user_message_if_missing(&mut messages, reused_user_message);
            return PreparedLlmRequest {
                messages,
                transport: ApiTransport::Responses,
                previous_response_id,
                upstream_thread_id: None,
                force_connection_close: false,
            };
        }

        let mut messages = messages_to_api_format(&request_messages);
        inject_reused_user_message_if_missing(&mut messages, reused_user_message);
        return PreparedLlmRequest {
            messages,
            transport: ApiTransport::Responses,
            previous_response_id: None,
            upstream_thread_id: uses_chatgpt_subscription_responses.then(|| {
                thread
                    .upstream_thread_id
                    .clone()
                    .unwrap_or_else(|| thread.id.clone())
            }),
            force_connection_close: false,
        };
    }

    let mut messages = messages_to_api_format(&request_messages);
    inject_reused_user_message_if_missing(&mut messages, reused_user_message);
    PreparedLlmRequest {
        messages,
        transport: ApiTransport::ChatCompletions,
        previous_response_id: None,
        upstream_thread_id: None,
        force_connection_close: false,
    }
}

/// Build an LLM request from an explicit message slice instead of `thread.messages`.
/// Used when the caller has already patched the user message content without cloning
/// the whole AgentThread — avoids per-turn O(thread_len) heap churn on megathreads.
pub(crate) fn prepare_llm_request_from_messages(
    messages: &[AgentMessage],
    thread: &AgentThread,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
    reused_user_message: Option<&str>,
) -> PreparedLlmRequest {
    let uses_chatgpt_subscription_responses = config.provider == PROVIDER_ID_OPENAI
        && provider_config.auth_source == crate::agent::types::AuthSource::ChatgptSubscription;
    let mut selected_transport =
        if provider_supports_transport(&config.provider, provider_config.api_transport) {
            provider_config.api_transport
        } else {
            default_api_transport_for_provider(&config.provider)
        };
    if uses_chatgpt_subscription_responses {
        selected_transport = ApiTransport::Responses;
    }
    if let Some(fixed_transport) =
        fixed_api_transport_for_model(&config.provider, &provider_config.model)
    {
        selected_transport = fixed_transport;
    }
    let compacted = compact_messages_for_request(messages, config, provider_config);
    let compaction_active =
        compacted.len() != messages.len() || compacted.iter().any(message_is_compaction_summary);
    let request_messages = if compacted.iter().any(message_is_compaction_summary) {
        append_owner_only_pins_after_artifact(
            &compacted,
            owner_only_pins_within_budget(thread, config, provider_config),
        )
    } else {
        compacted.clone()
    };

    let resumes_local_cli_session =
        crate::agent::types::provider_uses_local_subscription_cli(&config.provider);
    if selected_transport == ApiTransport::NativeAssistant
        && (resumes_local_cli_session
            || (!compaction_active && !provider_config.assistant_id.trim().is_empty()))
    {
        let latest_user_message = messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .cloned();
        if let Some(user_message) = latest_user_message {
            let assistant_id_matches = resumes_local_cli_session
                || thread.upstream_assistant_id.as_deref()
                    == Some(provider_config.assistant_id.as_str());
            return PreparedLlmRequest {
                messages: messages_to_api_format(&[user_message]),
                transport: ApiTransport::NativeAssistant,
                previous_response_id: None,
                upstream_thread_id: if thread.upstream_transport
                    == Some(ApiTransport::NativeAssistant)
                    && thread.upstream_provider.as_deref() == Some(config.provider.as_str())
                    && thread.upstream_model.as_deref() == Some(provider_config.model.as_str())
                    && assistant_id_matches
                {
                    thread.upstream_thread_id.clone()
                } else {
                    None
                },
                force_connection_close: false,
            };
        }
    }

    if selected_transport == ApiTransport::Responses {
        let previous_response_id = if !uses_chatgpt_subscription_responses
            && supports_response_continuity(&config.provider)
        {
            request_messages
                .iter()
                .enumerate()
                .rev()
                .find(|(_, message)| {
                    message.role == MessageRole::Assistant
                        && message.response_id.is_some()
                        && message.provider.as_deref() == Some(config.provider.as_str())
                        && message.model.as_deref() == Some(provider_config.model.as_str())
                        && message.api_transport == Some(ApiTransport::Responses)
                })
                .and_then(|(anchor_index, anchor_message)| {
                    let trailing = &request_messages[anchor_index + 1..];
                    if config.provider == PROVIDER_ID_GITHUB_COPILOT
                        && trailing.iter().any(|message| {
                            message.role == MessageRole::Tool
                                || message
                                    .tool_calls
                                    .as_ref()
                                    .is_some_and(|tool_calls| !tool_calls.is_empty())
                        })
                    {
                        return None;
                    }
                    let trailing_messages =
                        continuation_api_messages(trailing, reused_user_message);
                    if trailing_messages.is_empty() {
                        None
                    } else {
                        Some((trailing_messages, anchor_message.response_id.clone()))
                    }
                })
        } else {
            None
        };

        if let Some((messages, previous_response_id)) = previous_response_id {
            let mut messages = messages;
            inject_reused_user_message_if_missing(&mut messages, reused_user_message);
            return PreparedLlmRequest {
                messages,
                transport: ApiTransport::Responses,
                previous_response_id,
                upstream_thread_id: None,
                force_connection_close: false,
            };
        }

        let mut messages = messages_to_api_format(&request_messages);
        inject_reused_user_message_if_missing(&mut messages, reused_user_message);
        return PreparedLlmRequest {
            messages,
            transport: ApiTransport::Responses,
            previous_response_id: None,
            upstream_thread_id: uses_chatgpt_subscription_responses.then(|| {
                thread
                    .upstream_thread_id
                    .clone()
                    .unwrap_or_else(|| thread.id.clone())
            }),
            force_connection_close: false,
        };
    }

    let mut messages = messages_to_api_format(&request_messages);
    inject_reused_user_message_if_missing(&mut messages, reused_user_message);
    PreparedLlmRequest {
        messages,
        transport: ApiTransport::ChatCompletions,
        previous_response_id: None,
        upstream_thread_id: None,
        force_connection_close: false,
    }
}

pub(crate) fn inject_reused_user_message_if_missing(
    messages: &mut Vec<ApiMessage>,
    reused_user_message: Option<&str>,
) {
    let Some(reused_user_message) = reused_user_message
        .map(str::trim)
        .filter(|message| !message.is_empty())
    else {
        return;
    };

    let needs_trailing_user = match messages.last() {
        Some(message) => message.role != "user",
        None => true,
    };
    if !needs_trailing_user {
        return;
    }

    messages.push(ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text(reused_user_message.to_string()),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });
}

pub(crate) fn continuation_api_messages(
    messages: &[AgentMessage],
    reused_user_message: Option<&str>,
) -> Vec<ApiMessage> {
    if reused_user_message.is_none() {
        return messages_to_api_format(messages);
    }

    let mut pending_tool_results = std::collections::VecDeque::new();
    let mut api_messages = messages
        .iter()
        .filter(|message| {
            matches!(
                message.role,
                MessageRole::System
                    | MessageRole::User
                    | MessageRole::Assistant
                    | MessageRole::Tool
            )
        })
        .filter_map(|message| {
            let mut normalized_tool_calls = None;
            if message.role == MessageRole::Assistant {
                pending_tool_results.clear();
                if let Some(tool_calls) = &message.tool_calls {
                    let normalized: Vec<ApiToolCall> = tool_calls
                        .iter()
                        .enumerate()
                        .map(|(index, tool_call)| {
                            let normalized_id = if tool_call.id.trim().is_empty() {
                                format!(
                                    "synthetic_tool_call_{}_{}_{}",
                                    message.timestamp, index, tool_call.function.name
                                )
                            } else {
                                tool_call.id.clone()
                            };
                            pending_tool_results.push_back(normalized_id.clone());
                            ApiToolCall {
                                id: normalized_id,
                                call_type: "function".into(),
                                function: ApiToolCallFunction {
                                    name: tool_call.function.name.clone(),
                                    arguments: tool_call.function.arguments.clone(),
                                },
                            }
                        })
                        .collect();
                    normalized_tool_calls = Some(normalized);
                }
            }

            let mut normalized_tool_call_id = message.tool_call_id.clone();
            if message.role == MessageRole::Tool {
                let resolved_tool_call_id = if let Some(tool_call_id) = message
                    .tool_call_id
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                {
                    let position = pending_tool_results
                        .iter()
                        .position(|pending_id| pending_id == tool_call_id);
                    if let Some(position) = position {
                        pending_tool_results.remove(position).unwrap_or_default()
                    } else {
                        tool_call_id.clone()
                    }
                } else {
                    let Some(next_pending) = pending_tool_results.pop_front() else {
                        return None;
                    };
                    next_pending
                };

                normalized_tool_call_id = Some(resolved_tool_call_id);
                if normalized_tool_call_id.as_deref().is_none() {
                    return None;
                }
            } else if message.role != MessageRole::Assistant && !pending_tool_results.is_empty() {
                pending_tool_results.clear();
            }

            Some(ApiMessage {
                role: match message.role {
                    MessageRole::System => "system".into(),
                    MessageRole::User => "user".into(),
                    MessageRole::Assistant => "assistant".into(),
                    MessageRole::Tool => "tool".into(),
                },
                content: ApiContent::Text(message.content.clone()),
                reasoning: None,
                tool_call_id: normalized_tool_call_id,
                name: message.tool_name.clone(),
                tool_calls: normalized_tool_calls.or_else(|| {
                    message.tool_calls.as_ref().map(|tool_calls| {
                        tool_calls
                            .iter()
                            .map(|tool_call| ApiToolCall {
                                id: tool_call.id.clone(),
                                call_type: "function".into(),
                                function: ApiToolCallFunction {
                                    name: tool_call.function.name.clone(),
                                    arguments: tool_call.function.arguments.clone(),
                                },
                            })
                            .collect()
                    })
                }),
            })
        })
        .collect::<Vec<_>>();

    inject_reused_user_message_if_missing(&mut api_messages, reused_user_message);
    api_messages
}

pub(crate) fn append_owner_only_pins_after_artifact(
    compacted: &[AgentMessage],
    owner_only_pins: Vec<AgentMessage>,
) -> Vec<AgentMessage> {
    if owner_only_pins.is_empty()
        || compacted.is_empty()
        || !message_is_compaction_summary(&compacted[0])
    {
        return compacted.to_vec();
    }

    let mut request_messages = Vec::with_capacity(compacted.len() + owner_only_pins.len());
    request_messages.push(compacted[0].clone());
    request_messages.extend(owner_only_pins);
    request_messages.extend(compacted.iter().skip(1).cloned());
    request_messages
}
