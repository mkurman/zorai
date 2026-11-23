use super::*;
pub(crate) async fn parse_openai_sse(
    response: reqwest::Response,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    use futures::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut total_content = String::new();
    let mut total_reasoning = String::new();
    let mut pending_tool_calls: HashMap<u32, PendingToolCall> = HashMap::new();
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut cache_read_input_tokens: Option<u64> = None;
    let mut cost_usd: Option<f64> = None;
    let mut reasoning_tokens: Option<u64> = None;
    let mut response_id: Option<String> = None;
    let mut response_model: Option<String> = None;
    let mut finish_reason: Option<String> = None;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("failed to read SSE chunk")?;
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
            let line = line.as_str();
            let Some(data) = line.strip_prefix("data:").map(str::trim) else {
                continue;
            };
            if data == "[DONE]" {
                if !pending_tool_calls.is_empty() {
                    let tool_calls = drain_tool_calls(&mut pending_tool_calls);
                    let provider_final_result = Some(
                        crate::agent::types::CompletionProviderFinalResult::OpenAiChatCompletions(
                            crate::agent::types::CompletionOpenAiChatCompletionsFinalResult {
                                id: response_id.clone(),
                                model: response_model.clone(),
                                output_text: total_content.clone(),
                                reasoning: (!total_reasoning.is_empty())
                                    .then(|| total_reasoning.clone()),
                                tool_calls: tool_calls.clone(),
                                finish_reason: finish_reason.clone(),
                                input_tokens: Some(input_tokens),
                                output_tokens: Some(output_tokens),
                                cache_creation_input_tokens: None,
                                cache_read_input_tokens,
                            },
                        ),
                    );
                    let _ = tx
                        .send(Ok(CompletionChunk::ToolCalls {
                            reasoning_tokens,
                            tool_calls,
                            content: if total_content.is_empty() {
                                None
                            } else {
                                Some(total_content.clone())
                            },
                            reasoning: if total_reasoning.is_empty() {
                                None
                            } else {
                                Some(total_reasoning.clone())
                            },
                            input_tokens: Some(input_tokens),
                            output_tokens: Some(output_tokens),
                            stop_reason: None,
                            stop_sequence: None,
                            response_id: None,
                            request_id: None,
                            upstream_model: None,
                            upstream_role: None,
                            upstream_message_type: None,
                            upstream_container: None,
                            upstream_message: None,
                            provider_final_result: provider_final_result.clone(),
                            upstream_thread_id: None,
                            cache_creation_input_tokens: None,
                            cache_read_input_tokens,
                            server_tool_use: None,
                        }))
                        .await;
                } else {
                    let provider_final_result = Some(
                        crate::agent::types::CompletionProviderFinalResult::OpenAiChatCompletions(
                            crate::agent::types::CompletionOpenAiChatCompletionsFinalResult {
                                id: response_id.clone(),
                                model: response_model.clone(),
                                output_text: total_content.clone(),
                                reasoning: (!total_reasoning.is_empty())
                                    .then(|| total_reasoning.clone()),
                                tool_calls: Vec::new(),
                                finish_reason: finish_reason.clone(),
                                input_tokens: Some(input_tokens),
                                output_tokens: Some(output_tokens),
                                cache_creation_input_tokens: None,
                                cache_read_input_tokens,
                            },
                        ),
                    );
                    let _ = tx
                        .send(Ok(CompletionChunk::Done {
                            reasoning_tokens,
                            content: total_content.clone(),
                            reasoning: if total_reasoning.is_empty() {
                                None
                            } else {
                                Some(total_reasoning.clone())
                            },
                            input_tokens,
                            output_tokens,
                            cost_usd,
                            stop_reason: None,
                            stop_sequence: None,
                            response_id: None,
                            request_id: None,
                            upstream_model: None,
                            upstream_role: None,
                            upstream_message_type: None,
                            upstream_container: None,
                            upstream_message: None,
                            provider_final_result,
                            upstream_thread_id: None,
                            cache_creation_input_tokens: None,
                            cache_read_input_tokens,
                            server_tool_use: None,
                        }))
                        .await;
                }
                return Ok(());
            }

            let parsed: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            response_id = parsed
                .get("id")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .or(response_id);
            response_model = parsed
                .get("model")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .or(response_model);
            if let Some(usage) = parsed.get("usage") {
                input_tokens = usage
                    .get("prompt_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(input_tokens);
                output_tokens = usage
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(output_tokens);
                let total_tokens = usage.get("total_tokens").and_then(|v| v.as_u64());
                (input_tokens, output_tokens) =
                    normalize_openai_usage_tokens(input_tokens, output_tokens, total_tokens);
                cache_read_input_tokens =
                    openai_usage_cache_read(usage).or(cache_read_input_tokens);
                cost_usd = usage.get("cost").and_then(|v| v.as_f64()).or(cost_usd);
                reasoning_tokens = usage
                    .pointer("/completion_tokens_details/reasoning_tokens")
                    .and_then(|v| v.as_u64())
                    .or(reasoning_tokens);
            }

            let delta = match parsed.pointer("/choices/0/delta") {
                Some(d) => d,
                None => continue,
            };
            finish_reason = parsed
                .pointer("/choices/0/finish_reason")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .or(finish_reason);
            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                total_content.push_str(content);
                let _ = tx
                    .send(Ok(CompletionChunk::Delta {
                        content: content.into(),
                        reasoning: None,
                    }))
                    .await;
            }
            let reasoning_text = delta
                .get("reasoning")
                .and_then(|v| v.as_str())
                .or_else(|| delta.get("reasoning_content").and_then(|v| v.as_str()));
            if let Some(reasoning) = reasoning_text {
                total_reasoning.push_str(reasoning);
                let _ = tx
                    .send(Ok(CompletionChunk::Delta {
                        content: String::new(),
                        reasoning: Some(reasoning.into()),
                    }))
                    .await;
            }
            if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tcs {
                    let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let entry = pending_tool_calls
                        .entry(idx)
                        .or_insert_with(|| PendingToolCall {
                            id: String::new(),
                            name: String::new(),
                            arguments: String::new(),
                        });

                    if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                        entry.id = id.into();
                    }
                    if let Some(name) = tc.pointer("/function/name").and_then(|v| v.as_str()) {
                        entry.name.push_str(name);
                    }
                    if let Some(args) = tc.pointer("/function/arguments").and_then(|v| v.as_str()) {
                        entry.arguments.push_str(args);
                    }
                }
            }
        }
    }
    if !pending_tool_calls.is_empty() {
        let tool_calls = drain_tool_calls(&mut pending_tool_calls);
        let _ = tx
            .send(Ok(CompletionChunk::ToolCalls {
                reasoning_tokens,
                tool_calls,
                content: if total_content.is_empty() {
                    None
                } else {
                    Some(total_content)
                },
                reasoning: if total_reasoning.is_empty() {
                    None
                } else {
                    Some(total_reasoning)
                },
                input_tokens: Some(input_tokens),
                output_tokens: Some(output_tokens),
                stop_reason: None,
                stop_sequence: None,
                response_id: None,
                request_id: None,
                upstream_model: None,
                upstream_role: None,
                upstream_message_type: None,
                upstream_container: None,
                upstream_message: None,
                provider_final_result: None,
                upstream_thread_id: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens,
                server_tool_use: None,
            }))
            .await;
    } else {
        let _ = tx
            .send(Ok(CompletionChunk::Done {
                reasoning_tokens,
                content: total_content,
                reasoning: if total_reasoning.is_empty() {
                    None
                } else {
                    Some(total_reasoning)
                },
                input_tokens,
                output_tokens,
                cost_usd,
                stop_reason: None,
                stop_sequence: None,
                response_id: None,
                request_id: None,
                upstream_model: None,
                upstream_role: None,
                upstream_message_type: None,
                upstream_container: None,
                upstream_message: None,
                provider_final_result: None,
                upstream_thread_id: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens,
                server_tool_use: None,
            }))
            .await;
    }
    Ok(())
}

fn apply_openai_responses_terminal_response(
    response: &OpenAiResponsesStreamTerminalResponse,
    response_id: &mut Option<String>,
    input_tokens: &mut u64,
    output_tokens: &mut u64,
) {
    if let Some(id) = response.id.clone() {
        *response_id = Some(id);
    }
    if let Some(usage) = response.usage.as_ref() {
        (*input_tokens, *output_tokens) = usage.normalized_input_output_tokens();
    }
}

fn canonical_openai_responses_terminal_response(
    response: &OpenAiResponsesStreamTerminalResponse,
) -> Option<OpenAiResponsesTerminalResponse> {
    Some(OpenAiResponsesTerminalResponse {
        id: response.id.clone()?,
        object: response.object.clone()?,
        status: response.status.clone()?,
        output: response.output.clone()?,
        usage: response.usage.clone()?,
        error: response.error.clone(),
    })
}

fn responses_stream_failure_message(err: &anyhow::Error) -> String {
    upstream_failure_error(err)
        .map(|failure| failure.operator_message())
        .unwrap_or_else(|| err.to_string())
}

fn non_empty_owned(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn openai_usage_cache_read(usage: &serde_json::Value) -> Option<u64> {
    usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cached_tokens"))
        .and_then(|value| value.as_u64())
}

fn openai_responses_cache_read(response: Option<&OpenAiResponsesTerminalResponse>) -> Option<u64> {
    response
        .and_then(|response| response.usage.input_tokens_details.as_ref())
        .and_then(|details| details.cached_tokens)
}

fn build_openai_responses_provider_final_result(
    response_id: Option<String>,
    total_content: &str,
    reasoning: Option<String>,
    tool_calls: Vec<ToolCall>,
    response: Option<OpenAiResponsesTerminalResponse>,
    response_json: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
) -> crate::agent::types::CompletionProviderFinalResult {
    let cache_read_input_tokens = openai_responses_cache_read(response.as_ref());
    crate::agent::types::CompletionProviderFinalResult::OpenAiResponses(
        crate::agent::types::CompletionOpenAiResponsesFinalResult {
            id: response_id,
            output_text: total_content.to_owned(),
            reasoning,
            tool_calls,
            response,
            response_json,
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
            cache_creation_input_tokens: None,
            cache_read_input_tokens,
        },
    )
}

fn pending_tool_call_entry(
    pending_tool_calls: &mut HashMap<u32, PendingToolCall>,
    output_index: u32,
) -> &mut PendingToolCall {
    pending_tool_calls
        .entry(output_index)
        .or_insert_with(|| PendingToolCall {
            id: String::new(),
            name: String::new(),
            arguments: String::new(),
        })
}

fn apply_openai_responses_function_call_item(
    pending_tool_calls: &mut HashMap<u32, PendingToolCall>,
    output_index: u32,
    item: &serde_json::Value,
) {
    let entry = pending_tool_call_entry(pending_tool_calls, output_index);
    if let Some(call_id) = item.get("call_id").and_then(|value| value.as_str()) {
        entry.id = call_id.to_owned();
    }
    if let Some(name) = item.get("name").and_then(|value| value.as_str()) {
        entry.name = name.to_owned();
    }
    if let Some(arguments) = item.get("arguments").and_then(|value| value.as_str()) {
        entry.arguments = arguments.to_owned();
    }
}

fn openai_responses_sse_data_line<'a>(line: &'a str, remaining: &mut String) -> Option<&'a str> {
    let Some(data) = line.strip_prefix("data:").map(str::trim) else {
        if !line.is_empty() && !line.starts_with(':') && !line.starts_with("event:") {
            remaining.push_str(line);
            remaining.push('\n');
        }
        return None;
    };

    match data {
        "" | "[DONE]" => None,
        data => Some(data),
    }
}

async fn emit_openai_responses_terminal_chunk(
    tx: &mpsc::Sender<Result<CompletionChunk>>,
    response_id: Option<String>,
    total_content: &str,
    total_reasoning: &str,
    pending_tool_calls: &mut HashMap<u32, PendingToolCall>,
    response: Option<OpenAiResponsesTerminalResponse>,
    response_json: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
) {
    let cache_read_input_tokens = openai_responses_cache_read(response.as_ref());
    let reasoning_tokens = response
        .as_ref()
        .and_then(|response| response.usage.output_tokens_details.as_ref())
        .and_then(|details| details.reasoning_tokens);
    let content = non_empty_owned(total_content);
    let reasoning = non_empty_owned(total_reasoning);

    if !pending_tool_calls.is_empty() {
        let tool_calls = drain_tool_calls(pending_tool_calls);
        let provider_final_result = Some(build_openai_responses_provider_final_result(
            response_id.clone(),
            total_content,
            reasoning.clone(),
            tool_calls.clone(),
            response.clone(),
            response_json.clone(),
            input_tokens,
            output_tokens,
        ));
        let _ = tx
            .send(Ok(CompletionChunk::ToolCalls {
                reasoning_tokens,
                tool_calls,
                content,
                reasoning,
                input_tokens: Some(input_tokens),
                output_tokens: Some(output_tokens),
                stop_reason: None,
                stop_sequence: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens,
                server_tool_use: None,
                response_id,
                request_id: None,
                upstream_model: None,
                upstream_role: None,
                upstream_message_type: None,
                upstream_container: None,
                upstream_message: None,
                provider_final_result,
                upstream_thread_id: None,
            }))
            .await;
        return;
    }

    let provider_final_result = Some(build_openai_responses_provider_final_result(
        response_id.clone(),
        total_content,
        reasoning.clone(),
        Vec::new(),
        response,
        response_json,
        input_tokens,
        output_tokens,
    ));
    let _ = tx
        .send(Ok(CompletionChunk::Done {
            reasoning_tokens,
            content: total_content.to_owned(),
            reasoning,
            input_tokens,
            output_tokens,
            cost_usd: None,
            stop_reason: None,
            stop_sequence: None,
            cache_creation_input_tokens: None,
            cache_read_input_tokens,
            server_tool_use: None,
            response_id,
            request_id: None,
            upstream_model: None,
            upstream_role: None,
            upstream_message_type: None,
            upstream_container: None,
            upstream_message: None,
            provider_final_result,
            upstream_thread_id: None,
        }))
        .await;
}

pub(crate) async fn parse_openai_responses_sse(
    response: reqwest::Response,
    provider: &str,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    use futures::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut total_content = String::new();
    let mut total_reasoning = String::new();
    let mut pending_tool_calls: HashMap<u32, PendingToolCall> = HashMap::new();
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut response_id: Option<String> = None;
    let mut saw_any_json = false;
    let mut saw_responses_event = false;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("failed to read Responses SSE chunk")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        let mut remaining = String::new();
        for segment in buffer.split_inclusive('\n') {
            if !segment.ends_with('\n') {
                remaining.push_str(segment);
                continue;
            }

            let line = segment.trim_end_matches(['\r', '\n']);
            let Some(data) = openai_responses_sse_data_line(line, &mut remaining) else {
                continue;
            };

            let parsed: serde_json::Value = serde_json::from_str(data).map_err(|err| {
                openai_responses_stream_parse_error(
                    provider,
                    format!("malformed JSON event payload: {err}"),
                    serde_json::json!({
                        "provider": provider,
                        "details": err.to_string(),
                        "raw_event": data,
                    }),
                )
            })?;
            saw_any_json = true;
            let raw_terminal_response_json = parsed
                .get("response")
                .and_then(|value| serde_json::to_string(value).ok());
            let event = parse_openai_responses_stream_event(parsed).map_err(|err| {
                openai_responses_stream_parse_error(
                    provider,
                    format!("typed event parse failed: {err}"),
                    serde_json::json!({
                        "provider": provider,
                        "details": err.to_string(),
                        "raw_event": data,
                    }),
                )
            })?;
            if event.is_recognized_responses_event() {
                saw_responses_event = true;
            }
            match event {
                OpenAiResponsesStreamEvent::ResponseCreated(event) => {
                    if let Some(id) = event.response.id {
                        response_id = Some(id);
                    }
                }
                OpenAiResponsesStreamEvent::ResponseOutputTextDelta(event) => {
                    total_content.push_str(&event.delta);
                    let _ = tx
                        .send(Ok(CompletionChunk::Delta {
                            content: event.delta,
                            reasoning: None,
                        }))
                        .await;
                }
                OpenAiResponsesStreamEvent::ResponseReasoningSummaryTextDelta(event) => {
                    total_reasoning.push_str(&event.delta);
                    let _ = tx
                        .send(Ok(CompletionChunk::Delta {
                            content: String::new(),
                            reasoning: Some(event.delta),
                        }))
                        .await;
                }
                OpenAiResponsesStreamEvent::ResponseOutputItemAdded(event) => {
                    match event.item.get("type").and_then(|value| value.as_str()) {
                        Some("function_call") => apply_openai_responses_function_call_item(
                            &mut pending_tool_calls,
                            event.output_index,
                            &event.item,
                        ),
                        _ => {}
                    }
                }
                OpenAiResponsesStreamEvent::ResponseOutputItemDone(event) => {
                    match event.item.get("type").and_then(|value| value.as_str()) {
                        Some("function_call") => apply_openai_responses_function_call_item(
                            &mut pending_tool_calls,
                            event.output_index,
                            &event.item,
                        ),
                        Some("reasoning") => {
                            if total_reasoning.trim().is_empty() {
                                if let Some(summary_text) =
                                    extract_reasoning_summary_text(&event.item)
                                {
                                    total_reasoning = summary_text;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                OpenAiResponsesStreamEvent::ResponseFunctionCallArgumentsDelta(event) => {
                    let entry =
                        pending_tool_call_entry(&mut pending_tool_calls, event.output_index);
                    entry.arguments.push_str(&event.delta);
                }
                OpenAiResponsesStreamEvent::ResponseCompleted(event)
                | OpenAiResponsesStreamEvent::ResponseIncomplete(event) => {
                    let terminal_response =
                        canonical_openai_responses_terminal_response(&event.response);
                    apply_openai_responses_terminal_response(
                        &event.response,
                        &mut response_id,
                        &mut input_tokens,
                        &mut output_tokens,
                    );
                    emit_openai_responses_terminal_chunk(
                        tx,
                        response_id.clone(),
                        &total_content,
                        &total_reasoning,
                        &mut pending_tool_calls,
                        terminal_response,
                        raw_terminal_response_json,
                        input_tokens,
                        output_tokens,
                    )
                    .await;
                    return Ok(());
                }
                OpenAiResponsesStreamEvent::ResponseFailed(event) => {
                    apply_openai_responses_terminal_response(
                        &event.response,
                        &mut response_id,
                        &mut input_tokens,
                        &mut output_tokens,
                    );
                    let error = event.response.error.as_ref();
                    let failure = classify_openai_responses_stream_failure(
                        provider,
                        "response.failed",
                        error.map(|value| value.code.as_str()),
                        error.map(|value| value.message.as_str()),
                        serde_json::json!({
                            "provider": provider,
                            "event_type": "response.failed",
                            "response_id": response_id,
                            "response_status": event.response.status,
                            "upstream_error": error,
                        }),
                    );
                    let _ = tx
                        .send(Ok(CompletionChunk::Error {
                            message: responses_stream_failure_message(&failure),
                        }))
                        .await;
                    return Ok(());
                }
                OpenAiResponsesStreamEvent::Error(event) => {
                    let failure = classify_openai_responses_stream_failure(
                        provider,
                        "error",
                        event.error.code.as_deref(),
                        event.error.message.as_deref(),
                        serde_json::json!({
                            "provider": provider,
                            "event_type": "error",
                            "response_id": response_id,
                            "upstream_error": event.error,
                        }),
                    );
                    let _ = tx
                        .send(Ok(CompletionChunk::Error {
                            message: responses_stream_failure_message(&failure),
                        }))
                        .await;
                    return Ok(());
                }
                OpenAiResponsesStreamEvent::ChatCompletionsMismatch { .. } => {
                    return Err(transport_incompatibility_error(
                        provider,
                        "endpoint returned Chat Completions events for a Responses request",
                    ));
                }
                OpenAiResponsesStreamEvent::Unknown(_) => {}
            }
        }
        buffer = remaining;
    }

    if !buffer.trim().is_empty() {
        return Err(openai_responses_stream_parse_error(
            provider,
            "truncated stream ended with an incomplete SSE line",
            serde_json::json!({
                "provider": provider,
                "buffer_tail": buffer,
            }),
        ));
    }

    if saw_any_json && !saw_responses_event {
        return Err(transport_incompatibility_error(
            provider,
            "stream did not contain recognizable Responses API events",
        ));
    }

    if saw_responses_event {
        return Err(openai_responses_stream_parse_error(
            provider,
            "stream ended before a terminal response event",
            serde_json::json!({
                "provider": provider,
                "response_id": response_id,
                "received_content_chars": total_content.len(),
                "received_reasoning_chars": total_reasoning.len(),
                "pending_tool_call_count": pending_tool_calls.len(),
            }),
        ));
    }

    Ok(())
}
