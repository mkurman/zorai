use super::*;
pub(crate) fn anthropic_messages_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{}/messages", base)
    } else {
        format!("{}/v1/messages", base)
    }
}

#[allow(dead_code)]
fn anthropic_count_tokens_url(base_url: &str) -> String {
    format!("{}/count_tokens", anthropic_messages_url(base_url))
}
fn provider_requires_fresh_anthropic_connection(provider: &str) -> bool {
    matches!(
        provider,
        zorai_shared::providers::PROVIDER_ID_MINIMAX
            | zorai_shared::providers::PROVIDER_ID_MINIMAX_CODING_PLAN
    )
}
fn build_fresh_anthropic_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .http1_only()
        .pool_max_idle_per_host(0)
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(Into::into)
}
fn short_sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(bytes);
    format!("{:x}", digest)[..16].to_string()
}
fn redacted_header_value(name: &str, value: &reqwest::header::HeaderValue) -> String {
    match name {
        "authorization" | "x-api-key" | "proxy-authorization" | "cookie" | "set-cookie" => {
            "<redacted>".to_string()
        }
        _ => value
            .to_str()
            .map(|text| text.to_string())
            .unwrap_or_else(|_| "<binary>".to_string()),
    }
}
pub(crate) fn anthropic_request_fingerprint(request: &reqwest::Request) -> String {
    let mut header_lines: Vec<String> = request
        .headers()
        .iter()
        .map(|(name, value)| {
            format!(
                "{}:{}",
                name.as_str(),
                redacted_header_value(name.as_str(), value)
            )
        })
        .collect();
    header_lines.sort();
    let body_bytes = request
        .body()
        .and_then(|body| body.as_bytes())
        .unwrap_or(&[]);
    let canonical = format!(
        "method={}\nurl={}\nversion={:?}\nheaders={}\nbody_sha256={}\nbody_len={}",
        request.method(),
        request.url(),
        request.version(),
        header_lines.join("\n"),
        short_sha256_hex(body_bytes),
        body_bytes.len()
    );
    short_sha256_hex(canonical.as_bytes())
}
pub(crate) fn build_anthropic_request(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    force_connection_close: bool,
) -> Result<reqwest::Request> {
    let url = anthropic_messages_url(&config.base_url);

    let mut body = build_anthropic_base_body(provider, config, system_prompt, messages, tools);
    body["max_tokens"] = serde_json::json!(config.max_tokens.unwrap_or(4096));
    body["stream"] = serde_json::json!(true);

    build_anthropic_post_request(client, provider, config, &url, body, force_connection_close)
}

/// Build the `system` request field.
///
/// For native Claude models the system prompt is the stablest, most-reused span
/// in the request (it changes far less often than the conversation tail), so we
/// emit it as a content-block array with an explicit `cache_control` breakpoint.
/// Tools render before `system`, so this single breakpoint caches tools + system
/// as one segment that survives turns where only the message tail changes — a
/// dedicated cache prefix the body-root breakpoint can't independently provide.
///
/// TTL is left at the ephemeral default: the time-context section refreshes each
/// minute, so a longer TTL would only pay the higher cache-write multiplier
/// without extending the window the entry stays valid.
///
/// Non-Claude providers on this code path keep the plain-string form to avoid
/// any compatibility surprise with Anthropic-compatible third-party endpoints.
fn anthropic_system_field(config: &ProviderConfig, system_prompt: &str) -> serde_json::Value {
    if system_prompt.is_empty() || !is_native_claude_model(&config.model) {
        return serde_json::json!(system_prompt);
    }
    serde_json::json!([{
        "type": "text",
        "text": system_prompt,
        "cache_control": { "type": "ephemeral" },
    }])
}

/// Single gate for "this model speaks the native Claude request dialect"
/// (system content blocks with cache_control, the `thinking` field, ...).
/// Every Claude-only request feature must use this predicate rather than
/// scattering `starts_with("claude")` checks, so a future capability flag or
/// naming change lands in one place.
pub(crate) fn is_native_claude_model(model: &str) -> bool {
    model.starts_with("claude")
}

pub(crate) fn build_anthropic_base_body(
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
) -> serde_json::Value {
    let anthropic_messages = build_anthropic_messages(messages);

    let mut body = serde_json::json!({
        "model": config.model,
        "system": anthropic_system_field(config, system_prompt),
        "messages": anthropic_messages,
    });

    if !tools.is_empty() {
        let anthropic_tools: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "input_schema": t.function.parameters,
                })
            })
            .collect();
        body["tools"] = serde_json::json!(anthropic_tools);
        body["tool_choice"] = anthropic_tool_choice_json(config).unwrap_or_else(|| {
            serde_json::json!({
                "type": "auto",
            })
        });
    }

    if config.response_schema.is_some() || config.output_effort.is_some() {
        body["output_config"] = serde_json::json!({});
        if let Some(ref schema) = config.response_schema {
            body["output_config"]["format"] = serde_json::json!({
                "type": "json_schema",
                "schema": schema,
            });
        }
        if let Some(output_effort) = config.output_effort.as_ref() {
            body["output_config"]["effort"] = serde_json::json!(output_effort);
        }
    }

    apply_anthropic_optional_request_fields(&mut body, config);

    if let Some(budget_tokens) = anthropic_thinking_budget(&config.reasoning_effort) {
        if is_native_claude_model(&config.model)
            || (provider == zorai_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN
                && dashscope_openai_uses_enable_thinking(provider, &config.model))
            || matches!(
                provider,
                zorai_shared::providers::PROVIDER_ID_MINIMAX
                    | zorai_shared::providers::PROVIDER_ID_MINIMAX_CODING_PLAN
            )
        {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget_tokens,
            });
        }
    }

    body
}

fn build_anthropic_post_request(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    url: &str,
    body: serde_json::Value,
    force_connection_close: bool,
) -> Result<reqwest::Request> {
    let auth_method = get_provider_definition(provider)
        .map(|d| d.auth_method)
        .unwrap_or(AuthMethod::XApiKey);
    let mut request = auth_method.apply(
        client.post(url).header("Content-Type", "application/json"),
        &config.api_key,
    );
    if !is_dashscope_coding_plan_anthropic_base_url(&config.base_url) {
        request = request.header("anthropic-version", "2023-06-01");
    }
    if needs_coding_plan_sdk_headers(provider) {
        request = request.header(
            "anthropic-beta",
            "fine-grained-tool-streaming-2025-05-14,interleaved-thinking-2025-05-14",
        );
    }
    request = apply_dashscope_coding_plan_sdk_headers(
        request,
        provider,
        &config.base_url,
        ApiType::Anthropic,
    )
    .body(body.to_string());

    if force_connection_close || provider_requires_fresh_anthropic_connection(provider) {
        request = request.header(reqwest::header::CONNECTION, "close");
    }

    request.build().map_err(Into::into)
}

#[allow(dead_code)]
fn build_anthropic_count_tokens_request(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
) -> Result<reqwest::Request> {
    let url = anthropic_count_tokens_url(&config.base_url);
    let body = build_anthropic_base_body(provider, config, system_prompt, messages, tools);
    build_anthropic_post_request(client, provider, config, &url, body, false)
}

pub(crate) async fn run_anthropic(
    client: &reqwest::Client,
    provider: &str,
    attempt: u32,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    force_connection_close: bool,
    opencode_session_id: Option<&str>,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let client = if force_connection_close || provider_requires_fresh_anthropic_connection(provider)
    {
        build_fresh_anthropic_http_client()?
    } else {
        client.clone()
    };
    let mut request = build_anthropic_request(
        &client,
        provider,
        config,
        system_prompt,
        messages,
        tools,
        force_connection_close,
    )?;
    insert_opencode_go_headers(request.headers_mut(), provider, opencode_session_id);
    let request_fingerprint = anthropic_request_fingerprint(&request);
    tracing::info!(
        provider = %provider,
        model = %config.model,
        attempt,
        url = %request.url(),
        version = ?request.version(),
        request_fingerprint = %request_fingerprint,
        connection_close = request
            .headers()
            .get(reqwest::header::CONNECTION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or(""),
        tool_count = tools.len(),
        "dispatching anthropic request"
    );
    let response = client.execute(request).await?;
    tracing::info!(
        provider = %provider,
        model = %config.model,
        attempt,
        request_fingerprint = %request_fingerprint,
        status = %response.status(),
        "anthropic response headers received"
    );

    if !response.status().is_success() {
        let status = response.status();
        let retry_after_ms = extract_retry_after_ms(Some(response.headers()), "");
        let text = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>();
        tracing::warn!(
            provider = %provider,
            model = %config.model,
            attempt,
            request_fingerprint = %request_fingerprint,
            status = %status,
            retry_after_ms = retry_after_ms
                .or_else(|| extract_retry_after_ms(None, &text))
                .unwrap_or(0),
            body = %summarize_upstream_body(&text),
            "anthropic request returned non-success response"
        );
        return Err(classify_http_failure_with_retry_after(
            status,
            "Anthropic",
            &text,
            retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
        ));
    }

    let request_id = anthropic_request_id(response.headers());
    parse_anthropic_sse(response, request_id, tx).await
}

#[allow(dead_code)]
pub(crate) async fn count_anthropic_tokens(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
) -> Result<AnthropicMessageTokensCount> {
    let request = build_anthropic_count_tokens_request(
        client,
        provider,
        config,
        system_prompt,
        messages,
        tools,
    )?;
    let response = client.execute(request).await?;

    if !response.status().is_success() {
        let status = response.status();
        let retry_after_ms = extract_retry_after_ms(Some(response.headers()), "");
        let text = response.text().await.unwrap_or_default();
        return Err(classify_http_failure_with_retry_after(
            status,
            "Anthropic",
            &text,
            retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
        ));
    }

    let request_id = response
        .headers()
        .get("request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let mut parsed: serde_json::Value = response
        .json()
        .await
        .context("parse Anthropic count_tokens response")?;
    if let (Some(request_id), Some(object)) = (request_id, parsed.as_object_mut()) {
        object.insert("request_id".to_string(), serde_json::json!(request_id));
    }
    serde_json::from_value(parsed).context("decode Anthropic count_tokens response")
}

pub(crate) async fn parse_anthropic_sse(
    response: reqwest::Response,
    request_id: Option<String>,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    use futures::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut total_content = String::new();
    let mut total_reasoning = String::new();
    let mut usage = AnthropicStreamUsage::default();
    let mut stop_metadata = AnthropicStreamStopMetadata::default();
    let mut message_start = AnthropicStreamMessageStart::default();
    let mut upstream_message = AnthropicStreamUpstreamMessage::default();
    let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
    let mut current_tool_input = String::new();
    let mut in_thinking_block = false;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("failed to read Anthropic SSE chunk")?;
        buffer.extend_from_slice(&chunk);

        let mut lines: Vec<String> = Vec::new();
        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = buffer.drain(..=pos).collect();
            lines.push(
                String::from_utf8_lossy(&line_bytes)
                    .trim_end_matches(['\r', '\n'])
                    .to_string(),
            );
        }
        for line in &lines {
            let Some(data) = line.strip_prefix("data:").map(str::trim) else {
                continue;
            };
            let parsed: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let event_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match event_type {
                "message_start" => {
                    message_start.capture(&parsed);
                    upstream_message.capture_message_start(&parsed);
                    usage.capture_message_start(&parsed);
                }
                "content_block_start" => {
                    if let Some(cb) = parsed.get("content_block") {
                        upstream_message.capture_content_block_start(cb);
                        let block_type = cb.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match block_type {
                            "tool_use" => {
                                in_thinking_block = false;
                                let id = cb
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let name = cb
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                pending_tool_calls.push(PendingToolCall {
                                    id,
                                    name,
                                    arguments: String::new(),
                                });
                                current_tool_input.clear();
                            }
                            "thinking" => {
                                in_thinking_block = true;
                            }
                            _ => {
                                in_thinking_block = false;
                            }
                        }
                    }
                }
                "content_block_delta" => {
                    if let Some(delta) = parsed.get("delta") {
                        upstream_message.capture_content_block_delta(delta);
                    }
                    let delta_type = parsed
                        .pointer("/delta/type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if delta_type == "text_delta" {
                        if in_thinking_block {
                            let text = parsed
                                .pointer("/delta/text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !text.is_empty() {
                                total_reasoning.push_str(text);
                                let _ = tx
                                    .send(Ok(CompletionChunk::Delta {
                                        content: String::new(),
                                        reasoning: Some(text.into()),
                                    }))
                                    .await;
                            }
                        } else {
                            let text = parsed
                                .pointer("/delta/text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !text.is_empty() {
                                total_content.push_str(text);
                                let _ = tx
                                    .send(Ok(CompletionChunk::Delta {
                                        content: text.into(),
                                        reasoning: None,
                                    }))
                                    .await;
                            }
                        }
                    } else if delta_type == "thinking_delta" {
                        let thinking = parsed
                            .pointer("/delta/thinking")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !thinking.is_empty() {
                            total_reasoning.push_str(thinking);
                            let _ = tx
                                .send(Ok(CompletionChunk::Delta {
                                    content: String::new(),
                                    reasoning: Some(thinking.into()),
                                }))
                                .await;
                        }
                    } else if delta_type == "input_json_delta" {
                        let partial = parsed
                            .pointer("/delta/partial_json")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        current_tool_input.push_str(partial);
                    }
                }
                "content_block_stop" => {
                    upstream_message.finish_content_block();
                    if let Some(tc) = pending_tool_calls.last_mut() {
                        if tc.arguments.is_empty() && !current_tool_input.is_empty() {
                            tc.arguments = current_tool_input.clone();
                            current_tool_input.clear();
                        }
                    }
                    in_thinking_block = false;
                }
                "message_delta" => {
                    usage.capture_message_delta(&parsed);
                    stop_metadata.capture_message_delta(&parsed);
                }
                "error" => {
                    let error_type = parsed
                        .pointer("/error/type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown_error");
                    let error_message = parsed
                        .pointer("/error/message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Anthropic stream error");
                    let _ = tx
                        .send(Ok(CompletionChunk::Error {
                            message: format!(
                                "Anthropic stream error ({error_type}): {error_message}"
                            ),
                        }))
                        .await;
                    return Ok(());
                }
                "message_stop" => {
                    let final_reasoning =
                        (!total_reasoning.is_empty()).then_some(total_reasoning.clone());
                    let final_upstream_message = upstream_message.build(&stop_metadata);
                    if !pending_tool_calls.is_empty() {
                        let tool_calls: Vec<ToolCall> = pending_tool_calls
                            .drain(..)
                            .enumerate()
                            .map(|(index, tc)| {
                                ToolCall::with_default_weles_review(
                                    if tc.id.trim().is_empty() {
                                        synthesize_tool_call_id(
                                            zorai_shared::providers::PROVIDER_ID_ANTHROPIC,
                                            index,
                                            &tc.name,
                                        )
                                    } else {
                                        tc.id
                                    },
                                    ToolFunction {
                                        name: tc.name,
                                        arguments: tc.arguments,
                                    },
                                )
                            })
                            .collect();
                        let _ = tx
                            .send(Ok(CompletionChunk::ToolCalls {
                                reasoning_tokens: None,
                                tool_calls,
                                content: if total_content.is_empty() {
                                    None
                                } else {
                                    Some(total_content.clone())
                                },
                                reasoning: final_reasoning,
                                input_tokens: Some(usage.input_tokens),
                                output_tokens: Some(usage.output_tokens),
                                stop_reason: stop_metadata.stop_reason.clone(),
                                stop_sequence: stop_metadata.stop_sequence.clone(),
                                cache_creation_input_tokens: usage
                                    .cache_creation_input_tokens,
                                cache_read_input_tokens: usage.cache_read_input_tokens,
                                server_tool_use: usage.server_tool_use.clone(),
                                response_id: message_start.response_id.clone(), request_id: request_id.clone(), upstream_model: message_start.upstream_model.clone(),
                                upstream_role: message_start.upstream_role.clone(), upstream_message_type: message_start.upstream_message_type.clone(), upstream_container: message_start.upstream_container.clone(), upstream_message: final_upstream_message.clone(), provider_final_result: final_upstream_message.clone().map(crate::agent::types::CompletionProviderFinalResult::AnthropicMessage),
                                upstream_thread_id: None,
                            }))
                            .await;
                    } else {
                        let _ = tx
                            .send(Ok(CompletionChunk::Done {
                                reasoning_tokens: None,
                                content: total_content.clone(),
                                reasoning: final_reasoning,
                                input_tokens: usage.input_tokens,
                                output_tokens: usage.output_tokens,
                                cost_usd: None,
                                stop_reason: stop_metadata.stop_reason.clone(),
                                stop_sequence: stop_metadata.stop_sequence.clone(),
                                cache_creation_input_tokens: usage
                                    .cache_creation_input_tokens,
                                cache_read_input_tokens: usage.cache_read_input_tokens,
                                server_tool_use: usage.server_tool_use.clone(),
                                response_id: message_start.response_id.clone(), request_id: request_id.clone(), upstream_model: message_start.upstream_model.clone(),
                                upstream_role: message_start.upstream_role.clone(), upstream_message_type: message_start.upstream_message_type.clone(), upstream_container: message_start.upstream_container.clone(), upstream_message: final_upstream_message.clone(), provider_final_result: final_upstream_message.clone().map(crate::agent::types::CompletionProviderFinalResult::AnthropicMessage),
                                upstream_thread_id: None,
                            }))
                            .await;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    let final_upstream_message = upstream_message.build(&stop_metadata);
    let _ = tx
        .send(Ok(CompletionChunk::Done {
            reasoning_tokens: None,
            content: total_content,
            reasoning: (!total_reasoning.is_empty()).then_some(total_reasoning),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cost_usd: None,
            stop_reason: stop_metadata.stop_reason,
            stop_sequence: stop_metadata.stop_sequence,
            cache_creation_input_tokens: usage.cache_creation_input_tokens,
            cache_read_input_tokens: usage.cache_read_input_tokens,
            server_tool_use: usage.server_tool_use,
            response_id: message_start.response_id,
            request_id,
            upstream_model: message_start.upstream_model,
            upstream_role: message_start.upstream_role,
            upstream_message_type: message_start.upstream_message_type,
            upstream_container: message_start.upstream_container,
            upstream_message: final_upstream_message.clone(),
            provider_final_result: final_upstream_message
                .map(crate::agent::types::CompletionProviderFinalResult::AnthropicMessage),
            upstream_thread_id: None,
        }))
        .await;

    Ok(())
}
