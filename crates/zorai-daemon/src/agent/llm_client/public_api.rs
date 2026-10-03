use super::*;

const MAX_RETRY_DELAY_MS: u64 = 60_000;

async fn wait_for_retry_delay_or_abort(
    tx: &mpsc::Sender<Result<CompletionChunk>>,
    delay_ms: u64,
    retry_now: Option<&Arc<tokio::sync::Notify>>,
    cancel: Option<&tokio_util::sync::CancellationToken>,
) -> bool {
    if tx.is_closed() || cancel.is_some_and(|token| token.is_cancelled()) {
        return false;
    }
    let delay_ms = delay_ms.max(1);
    let cancel = cancel.cloned();
    let cancel_for_wait = cancel.clone();
    let cancelled = async move {
        if let Some(token) = cancel_for_wait {
            token.cancelled().await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    let still_open = |cancel: &Option<tokio_util::sync::CancellationToken>| {
        !tx.is_closed() && cancel.as_ref().is_none_or(|token| !token.is_cancelled())
    };
    match retry_now {
        Some(retry_now) => tokio::select! {
            biased;
            _ = tx.closed() => false,
            _ = cancelled => false,
            _ = retry_now.notified() => still_open(&cancel),
            _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => still_open(&cancel),
        },
        None => tokio::select! {
            biased;
            _ = tx.closed() => false,
            _ = cancelled => false,
            _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => still_open(&cancel),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptTarget {
    pub(crate) api_type: ApiType,
    pub(crate) branch: &'static str,
    pub(crate) url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetryFailureAnalysis {
    pub(crate) structured_class: Option<String>,
    pub(crate) failure_class: &'static str,
    pub(crate) retry_after_ms: Option<u64>,
    pub(crate) response_observed: bool,
    pub(crate) is_rate_limited: bool,
    pub(crate) is_transient_transport: bool,
    pub(crate) is_temporary_upstream: bool,
}

pub(crate) fn effective_attempt_target(
    provider: &str,
    config: &ProviderConfig,
    transport: ApiTransport,
) -> AttemptTarget {
    if transport == ApiTransport::AnthropicMessages {
        return AttemptTarget {
            api_type: ApiType::Anthropic,
            branch: "anthropic",
            url: anthropic_messages_url(&config.base_url),
        };
    }

    let api_type = get_provider_api_type(provider, &config.model, &config.base_url);
    if api_type == ApiType::Anthropic {
        return AttemptTarget {
            api_type,
            branch: "anthropic",
            url: anthropic_messages_url(&config.base_url),
        };
    }
    if api_type == ApiType::Gemini {
        return AttemptTarget {
            api_type,
            branch: "gemini_interactions",
            url: gemini_interactions_url(&config.base_url, true),
        };
    }

    match transport {
        ApiTransport::NativeAssistant => AttemptTarget {
            api_type,
            branch: "native_assistant",
            url: build_native_assistant_base_url(provider, config)
                .map(|base| format!("{base}/threads"))
                .unwrap_or_else(|| config.base_url.clone()),
        },
        ApiTransport::ChatCompletions => AttemptTarget {
            api_type,
            branch: "chat_completions",
            url: build_chat_completion_url(&config.base_url),
        },
        ApiTransport::Responses => AttemptTarget {
            api_type,
            branch: "responses",
            url: build_responses_url(&config.base_url),
        },
        ApiTransport::AnthropicMessages => unreachable!("handled above"),
    }
}

pub(crate) fn analyze_retry_failure(err: &anyhow::Error) -> RetryFailureAnalysis {
    let structured_failure = upstream_failure_error(err);
    let structured_class = structured_failure.map(|failure| failure.class);
    let response_observed = structured_class.is_some();
    let retry_after_ms = structured_failure
        .and_then(|failure| failure.diagnostics.get("retry_after_ms"))
        .and_then(|value| value.as_u64());
    let usage_limit_exhausted = is_exhausted_provider_usage_limit(&err.to_string());
    let is_rate_limited =
        !usage_limit_exhausted && matches!(structured_class, Some(UpstreamFailureClass::RateLimit));
    let is_transient_transport = matches!(
        structured_class,
        Some(UpstreamFailureClass::TransientTransport)
    ) || is_transient_transport_error(err);
    let is_temporary_upstream = matches!(
        structured_class,
        Some(UpstreamFailureClass::TemporaryUpstream)
    ) || is_temporary_upstream_error(err);
    let failure_class = if is_rate_limited {
        "rate_limit"
    } else if is_transient_transport {
        "transport"
    } else if is_temporary_upstream {
        "upstream"
    } else {
        "non_retryable"
    };

    RetryFailureAnalysis {
        structured_class: structured_class.map(|class| class.as_str().to_string()),
        failure_class,
        retry_after_ms,
        response_observed,
        is_rate_limited,
        is_transient_transport,
        is_temporary_upstream,
    }
}

pub(crate) fn compute_retry_delay_ms_for_attempt(base_delay_ms: u64, attempt: u32) -> u64 {
    let multiplier = u64::from(attempt.max(1));
    base_delay_ms
        .saturating_mul(multiplier)
        .min(MAX_RETRY_DELAY_MS)
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum CopilotInitiator {
    User,
    #[default]
    Agent,
}

impl CopilotInitiator {
    pub(crate) fn as_header_value(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CompletionRequestOptions {
    pub force_connection_close: bool,
    pub copilot_initiator: CopilotInitiator,
    pub working_dir: Option<String>,
    pub claude_permission_mode: Option<String>,
    pub retry_now: Option<Arc<tokio::sync::Notify>>,
    pub cancel: Option<tokio_util::sync::CancellationToken>,
    pub opencode_session_id: Option<String>,
}

/// Send a completion request. Returns a stream of `CompletionChunk`.
pub fn send_completion_request(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    transport: ApiTransport,
    previous_response_id: Option<String>,
    upstream_thread_id: Option<String>,
    retry_strategy: RetryStrategy,
) -> CompletionStream {
    send_completion_request_with_options(
        client,
        provider,
        config,
        system_prompt,
        messages,
        tools,
        transport,
        previous_response_id,
        upstream_thread_id,
        retry_strategy,
        CompletionRequestOptions::default(),
    )
}

pub(crate) fn coerce_transport_for_provider(
    provider: &str,
    transport: ApiTransport,
) -> ApiTransport {
    if matches!(
        get_provider_definition(provider).and_then(|definition| definition.native_transport_kind),
        Some(
            crate::agent::types::NativeTransportKind::ClaudeCodeCli
                | crate::agent::types::NativeTransportKind::CursorCli
        )
    ) {
        ApiTransport::NativeAssistant
    } else {
        transport
    }
}

pub(crate) fn send_completion_request_with_options(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    transport: ApiTransport,
    previous_response_id: Option<String>,
    upstream_thread_id: Option<String>,
    retry_strategy: RetryStrategy,
    options: CompletionRequestOptions,
) -> CompletionStream {
    let (tx, rx) = mpsc::channel(64);
    let transport = coerce_transport_for_provider(provider, transport);
    let client = client.clone();
    let provider = provider.to_string();
    let config = config.clone();
    let system_prompt = system_prompt.to_string();
    let messages = messages.to_vec();
    let tools = tools.to_vec();
    let previous_response_id = previous_response_id.clone();
    let upstream_thread_id = upstream_thread_id.clone();
    let options = options;

    tokio::spawn(async move {
        let mut retry_attempt = 0u32;
        loop {
            if tx.is_closed()
                || options
                    .cancel
                    .as_ref()
                    .is_some_and(|token| token.is_cancelled())
            {
                break;
            }
            let target = effective_attempt_target(&provider, &config, transport);
            let attempt_number = retry_attempt.saturating_add(1);
            tracing::info!(
                provider = %provider,
                model = %config.model,
                api_type = ?target.api_type,
                attempt = attempt_number,
                branch = target.branch,
                url = %target.url,
                configured_transport = ?transport,
                retry_strategy = ?retry_strategy,
                previous_response_id = previous_response_id.as_deref().unwrap_or(""),
                upstream_thread_id = upstream_thread_id.as_deref().unwrap_or(""),
                message_count = messages.len(),
                tool_count = tools.len(),
                "llm attempt start"
            );

            let result = if transport == ApiTransport::AnthropicMessages
                || target.api_type == ApiType::Anthropic
            {
                run_anthropic(
                    &client,
                    &provider,
                    attempt_number,
                    &config,
                    &system_prompt,
                    &messages,
                    &tools,
                    options.force_connection_close,
                    options.opencode_session_id.as_deref(),
                    &tx,
                )
                .await
            } else if target.api_type == ApiType::Gemini {
                run_gemini(
                    &client,
                    &provider,
                    &config,
                    &system_prompt,
                    &messages,
                    &tools,
                    previous_response_id.as_deref(),
                    &tx,
                )
                .await
            } else {
                match transport {
                    ApiTransport::NativeAssistant => {
                        if matches!(
                            get_provider_definition(&provider)
                                .and_then(|definition| definition.native_transport_kind),
                            Some(crate::agent::types::NativeTransportKind::ClaudeCodeCli)
                        ) {
                            run_claude_code_cli(
                                &provider,
                                &config,
                                &system_prompt,
                                &messages,
                                upstream_thread_id.as_deref(),
                                options.working_dir.as_deref(),
                                options.claude_permission_mode.as_deref(),
                                &tx,
                            )
                            .await
                        } else if matches!(
                            get_provider_definition(&provider)
                                .and_then(|definition| definition.native_transport_kind),
                            Some(crate::agent::types::NativeTransportKind::CursorCli)
                        ) {
                            run_cursor_cli(&provider, &config, &system_prompt, &messages, &tx).await
                        } else {
                            run_native_assistant(
                                &client,
                                &provider,
                                &config,
                                &messages,
                                upstream_thread_id.as_deref(),
                                options.copilot_initiator,
                                options.force_connection_close,
                                &tx,
                            )
                            .await
                        }
                    }
                    ApiTransport::ChatCompletions => {
                        run_openai_chat_completions(
                            &client,
                            &provider,
                            &config,
                            &system_prompt,
                            &messages,
                            &tools,
                            options.copilot_initiator,
                            options.force_connection_close,
                            options.opencode_session_id.as_deref(),
                            &tx,
                        )
                        .await
                    }
                    ApiTransport::Responses => {
                        run_openai_responses(
                            &client,
                            &provider,
                            &config,
                            &system_prompt,
                            &messages,
                            &tools,
                            previous_response_id.as_deref(),
                            upstream_thread_id.as_deref(),
                            options.copilot_initiator,
                            options.force_connection_close,
                            options.opencode_session_id.as_deref(),
                            &tx,
                        )
                        .await
                    }
                    ApiTransport::AnthropicMessages => unreachable!("handled above"),
                }
            };

            match result {
                Ok(()) => {
                    tracing::info!(
                        provider = %provider,
                        model = %config.model,
                        attempt = attempt_number,
                        branch = target.branch,
                        url = %target.url,
                        "llm attempt completed"
                    );
                    break;
                }
                Err(e) => {
                    let analysis = analyze_retry_failure(&e);
                    tracing::warn!(
                        provider = %provider,
                        model = %config.model,
                        attempt = attempt_number,
                        branch = target.branch,
                        url = %target.url,
                        configured_transport = ?transport,
                        structured_class = analysis.structured_class.as_deref().unwrap_or(""),
                        failure_class = analysis.failure_class,
                        response_observed = analysis.response_observed,
                        retry_after_ms = analysis.retry_after_ms.unwrap_or(0),
                        error_chain = %summarize_transport_error(&e),
                        "llm attempt failed"
                    );
                    if analysis.is_rate_limited
                        || analysis.is_transient_transport
                        || analysis.is_temporary_upstream
                    {
                        let retry_message = e.to_string();
                        match retry_strategy {
                            RetryStrategy::Bounded {
                                max_retries,
                                retry_delay_ms,
                            } if retry_attempt < max_retries => {
                                retry_attempt += 1;
                                let delay_ms = analysis.retry_after_ms.unwrap_or_else(|| {
                                    compute_retry_delay_ms_for_attempt(
                                        retry_delay_ms,
                                        retry_attempt,
                                    )
                                });
                                tracing::info!(
                                    provider = %provider,
                                    model = %config.model,
                                    branch = target.branch,
                                    url = %target.url,
                                    attempt = retry_attempt,
                                    max_retries,
                                    delay_ms,
                                    failure_class = analysis.failure_class,
                                    retry_reason = analysis.structured_class.as_deref().unwrap_or("transient"),
                                    "llm retry scheduled"
                                );
                                if tx.is_closed() {
                                    break;
                                }
                                let _ = tx
                                    .send(Ok(CompletionChunk::Retry {
                                        attempt: retry_attempt,
                                        max_retries,
                                        delay_ms,
                                        failure_class: analysis.failure_class.to_string(),
                                        message: retry_message.clone(),
                                    }))
                                    .await;
                                if !wait_for_retry_delay_or_abort(
                                    &tx,
                                    delay_ms,
                                    options.retry_now.as_ref(),
                                    options.cancel.as_ref(),
                                )
                                .await
                                {
                                    break;
                                }
                                continue;
                            }
                            RetryStrategy::DurableRateLimited if analysis.is_rate_limited => {
                                retry_attempt = retry_attempt.saturating_add(1);
                                let delay_ms = analysis.retry_after_ms.unwrap_or_else(|| {
                                    compute_retry_delay_ms_for_attempt(5_000, retry_attempt)
                                });
                                tracing::info!(
                                    provider = %provider,
                                    model = %config.model,
                                    branch = target.branch,
                                    url = %target.url,
                                    attempt = retry_attempt,
                                    delay_ms,
                                    failure_class = analysis.failure_class,
                                    retry_reason = analysis.structured_class.as_deref().unwrap_or("rate_limit"),
                                    "durable llm retry scheduled"
                                );
                                if tx.is_closed() {
                                    break;
                                }
                                let _ = tx
                                    .send(Ok(CompletionChunk::Retry {
                                        attempt: retry_attempt,
                                        max_retries: 0,
                                        delay_ms,
                                        failure_class: analysis.failure_class.to_string(),
                                        message: retry_message.clone(),
                                    }))
                                    .await;
                                if !wait_for_retry_delay_or_abort(
                                    &tx,
                                    delay_ms,
                                    options.retry_now.as_ref(),
                                    options.cancel.as_ref(),
                                )
                                .await
                                {
                                    break;
                                }
                                continue;
                            }
                            _ => {}
                        }
                    }

                    let message = if let Some(failure) = upstream_failure_error(&e) {
                        failure.operator_message()
                    } else if let Some(rate_limit) = e.downcast_ref::<RateLimitError>() {
                        rate_limit.to_string()
                    } else if is_timeout_error(&e) {
                        format!(
                            "{provider} request timed out: {}",
                            summarize_transport_error(&e)
                        )
                    } else if is_transient_transport_error(&e) {
                        format!(
                            "{provider} transport error: {}",
                            summarize_transport_error(&e)
                        )
                    } else {
                        format!("API error: {}", summarize_transport_error(&e))
                    };
                    let _ = tx.send(Ok(CompletionChunk::Error { message })).await;
                    break;
                }
            }
        }
    });

    CompletionStream { rx }
}

#[allow(dead_code)]
pub async fn count_request_tokens(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
) -> Result<AnthropicMessageTokensCount> {
    let api_type = get_provider_api_type(provider, &config.model, &config.base_url);
    if api_type != ApiType::Anthropic {
        return Err(transport_incompatibility_error(
            provider,
            "count_tokens is only implemented for Anthropic Messages providers",
        ));
    }

    count_anthropic_tokens(client, provider, config, system_prompt, messages, tools).await
}

/// Convert `AgentMessage` history to API format.
pub fn messages_to_api_format(messages: &[crate::agent::types::AgentMessage]) -> Vec<ApiMessage> {
    let mut pending_tool_results = std::collections::VecDeque::new();

    messages
        .iter()
        .filter(|m| {
            matches!(
                m.role,
                crate::agent::types::MessageRole::User
                    | crate::agent::types::MessageRole::Assistant
                    | crate::agent::types::MessageRole::Tool
            )
        })
        .filter_map(|m| {
            let mut normalized_tool_calls = None;
            if matches!(m.role, crate::agent::types::MessageRole::Assistant) {
                pending_tool_results.clear();
                if let Some(tool_calls) = &m.tool_calls {
                    let normalized: Vec<ApiToolCall> = tool_calls
                        .iter()
                        .enumerate()
                        .map(|(index, tc)| {
                            let normalized_id = if tc.id.trim().is_empty() {
                                format!(
                                    "synthetic_tool_call_{}_{}_{}",
                                    m.timestamp, index, tc.function.name
                                )
                            } else {
                                tc.id.clone()
                            };
                            pending_tool_results.push_back(normalized_id.clone());
                            ApiToolCall {
                                id: normalized_id,
                                call_type: "function".into(),
                                function: ApiToolCallFunction {
                                    name: tc.function.name.clone(),
                                    arguments: normalize_tool_call_arguments_for_api(
                                        &tc.function.arguments,
                                    ),
                                },
                            }
                        })
                        .collect();
                    normalized_tool_calls = Some(normalized);
                }
            }

            let mut normalized_tool_call_id = m.tool_call_id.clone();
            if matches!(m.role, crate::agent::types::MessageRole::Tool) {
                let resolved_tool_call_id = if let Some(tool_call_id) = m
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
            } else if !matches!(m.role, crate::agent::types::MessageRole::Assistant)
                && !pending_tool_results.is_empty()
            {
                pending_tool_results.clear();
            }

            Some(ApiMessage {
                role: match m.role {
                    crate::agent::types::MessageRole::System => "system".into(),
                    crate::agent::types::MessageRole::User => "user".into(),
                    crate::agent::types::MessageRole::Assistant => "assistant".into(),
                    crate::agent::types::MessageRole::Tool => "tool".into(),
                },
                content: agent_message_content_to_api_content(m),
                reasoning: m.reasoning.clone(),
                tool_call_id: normalized_tool_call_id,
                name: m.tool_name.clone(),
                tool_calls: normalized_tool_calls.or_else(|| {
                    m.tool_calls.as_ref().map(|tcs| {
                        tcs.iter()
                            .map(|tc| ApiToolCall {
                                id: tc.id.clone(),
                                call_type: "function".into(),
                                function: ApiToolCallFunction {
                                    name: tc.function.name.clone(),
                                    arguments: normalize_tool_call_arguments_for_api(
                                        &tc.function.arguments,
                                    ),
                                },
                            })
                            .collect()
                    })
                }),
            })
        })
        .collect()
}

fn normalize_tool_call_arguments_for_api(arguments: &str) -> String {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return "{}".to_string();
    }
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return trimmed.to_string();
    }

    serde_json::json!({ "_invalid_tool_arguments": arguments }).to_string()
}

fn agent_message_content_to_api_content(message: &crate::agent::types::AgentMessage) -> ApiContent {
    let blocks = agent_content_blocks_to_api_blocks(&message.content_blocks);
    if blocks.is_empty() {
        ApiContent::Text(message.content.clone())
    } else {
        ApiContent::Blocks(blocks)
    }
}

fn agent_content_blocks_to_api_blocks(
    blocks: &[crate::agent::types::AgentContentBlock],
) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            crate::agent::types::AgentContentBlock::Text { text } => {
                (!text.is_empty()).then(|| {
                    serde_json::json!({
                        "type": "input_text",
                        "text": text,
                    })
                })
            }
            crate::agent::types::AgentContentBlock::Image {
                url,
                data_url,
                mime_type: _,
            } => url
                .as_ref()
                .or(data_url.as_ref())
                .filter(|value| !value.trim().is_empty())
                .map(|image_url| {
                    serde_json::json!({
                        "type": "input_image",
                        "image_url": image_url,
                    })
                }),
            crate::agent::types::AgentContentBlock::Audio {
                url,
                data_url,
                mime_type,
            } => {
                let audio_url = url
                    .as_ref()
                    .or(data_url.as_ref())
                    .filter(|value| !value.trim().is_empty())?;
                let input_audio = if let Some(data) = audio_url.strip_prefix("data:") {
                    let (header, payload) = data.split_once(',')?;
                    if !header.contains(";base64") {
                        return None;
                    }
                    let format = mime_type
                        .as_deref()
                        .and_then(audio_format_from_mime_type)
                        .or_else(|| {
                            header
                                .split(';')
                                .next()
                                .and_then(audio_format_from_mime_type)
                        })
                        .unwrap_or("wav");
                    serde_json::json!({
                        "data": payload,
                        "format": format,
                    })
                } else {
                    serde_json::json!({
                        "url": audio_url,
                        "format": mime_type
                            .as_deref()
                            .and_then(audio_format_from_mime_type)
                            .unwrap_or("wav"),
                    })
                };
                Some(serde_json::json!({
                    "type": "input_audio",
                    "input_audio": input_audio,
                }))
            }
        })
        .collect()
}

fn audio_format_from_mime_type(mime_type: &str) -> Option<&'static str> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/ogg" => Some("ogg"),
        "audio/flac" => Some("flac"),
        "audio/mp4" | "audio/m4a" => Some("mp4"),
        "audio/webm" => Some("webm"),
        _ => None,
    }
}

fn api_content_to_json(content: &ApiContent) -> serde_json::Value {
    match content {
        ApiContent::Text(text) => serde_json::Value::String(text.clone()),
        ApiContent::Blocks(blocks) => {
            serde_json::Value::Array(map_api_blocks_to_chat_completions_blocks(blocks))
        }
    }
}

fn map_api_blocks_to_chat_completions_blocks(
    blocks: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(map_api_block_to_chat_completions_block)
        .collect()
}

fn map_api_block_to_chat_completions_block(block: &serde_json::Value) -> Option<serde_json::Value> {
    let block_type = block
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    match block_type {
        "input_text" | "text" => block
            .get("text")
            .and_then(|value| value.as_str())
            .map(|text| {
                serde_json::json!({
                    "type": "text",
                    "text": text,
                })
            }),
        "input_image" | "image_url" => {
            let image_url = block.get("image_url")?;
            let url = image_url
                .get("url")
                .and_then(|value| value.as_str())
                .or_else(|| image_url.as_str())
                .filter(|url| !url.trim().is_empty())?;
            Some(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": url },
            }))
        }
        "input_audio" => Some(block.clone()),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) fn build_chat_completion_messages(
    system_prompt: &str,
    messages: &[ApiMessage],
) -> Result<Vec<serde_json::Value>> {
    build_chat_completion_messages_with_options(system_prompt, messages, false, false, false, false)
}

pub(crate) fn build_chat_completion_messages_with_options(
    system_prompt: &str,
    messages: &[ApiMessage],
    include_tool_reasoning_content: bool,
    synthesize_missing_tool_reasoning_content: bool,
    include_non_tool_reasoning_content: bool,
    include_non_tool_reasoning_content_after_tool_call: bool,
) -> Result<Vec<serde_json::Value>> {
    let messages = sanitize_api_messages(messages);

    let mut out = Vec::with_capacity(messages.len() + 1);
    out.push(serde_json::json!({
        "role": "system",
        "content": system_prompt,
    }));

    let mut current_turn_has_tool_call = false;
    for message in messages {
        if message.role == "user" {
            current_turn_has_tool_call = false;
        }
        let mut obj = serde_json::Map::new();
        obj.insert(
            "role".to_string(),
            serde_json::Value::String(message.role.clone()),
        );

        let assistant_has_tool_calls = message.role == "assistant"
            && message
                .tool_calls
                .as_ref()
                .is_some_and(|tool_calls| !tool_calls.is_empty());

        if assistant_has_tool_calls {
            current_turn_has_tool_call = true;
            obj.insert(
                "content".to_string(),
                serde_json::Value::String(String::new()),
            );
            obj.insert(
                "tool_calls".to_string(),
                serde_json::to_value(message.tool_calls.clone().unwrap_or_default())?,
            );
            if include_tool_reasoning_content {
                insert_reasoning_content(
                    &mut obj,
                    message.reasoning.as_deref(),
                    synthesize_missing_tool_reasoning_content,
                );
            }
        } else {
            let mut content_json = api_content_to_json(&message.content);
            if message.role == "assistant" {
                let include_after_tool_call = include_non_tool_reasoning_content_after_tool_call
                    && current_turn_has_tool_call;
                if include_non_tool_reasoning_content || include_after_tool_call {
                    insert_reasoning_content(
                        &mut obj,
                        message.reasoning.as_deref(),
                        include_after_tool_call && synthesize_missing_tool_reasoning_content,
                    );
                }
                if assistant_content_is_unset(&content_json) {
                    if obj.contains_key("reasoning_content") {
                        content_json = serde_json::Value::String(String::new());
                    } else {
                        continue;
                    }
                }
            }
            obj.insert("content".to_string(), content_json);
            if let Some(tool_call_id) = &message.tool_call_id {
                obj.insert(
                    "tool_call_id".to_string(),
                    serde_json::Value::String(tool_call_id.clone()),
                );
            }
            if let Some(name) = &message.name {
                obj.insert("name".to_string(), serde_json::Value::String(name.clone()));
            }
            if let Some(tool_calls) = &message.tool_calls {
                if !tool_calls.is_empty() {
                    obj.insert("tool_calls".to_string(), serde_json::to_value(tool_calls)?);
                }
            }
        }

        out.push(serde_json::Value::Object(obj));
    }

    Ok(out)
}

fn assistant_content_is_unset(content: &serde_json::Value) -> bool {
    match content {
        serde_json::Value::Null => true,
        serde_json::Value::String(text) => text.trim().is_empty(),
        serde_json::Value::Array(blocks) => blocks.is_empty(),
        _ => false,
    }
}

fn insert_reasoning_content(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    reasoning: Option<&str>,
    synthesize_missing: bool,
) {
    if let Some(reasoning) = reasoning.filter(|value| !value.trim().is_empty()) {
        obj.insert(
            "reasoning_content".to_string(),
            serde_json::Value::String(reasoning.to_string()),
        );
    } else if synthesize_missing {
        obj.insert(
            "reasoning_content".to_string(),
            serde_json::Value::String(" ".to_string()),
        );
    }
}
