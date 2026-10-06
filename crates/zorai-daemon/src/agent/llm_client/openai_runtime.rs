use super::*;
pub(crate) async fn fetch_native_assistant_message(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    base_url: &str,
    thread_id: &str,
    copilot_initiator: CopilotInitiator,
    force_connection_close: bool,
) -> Result<String> {
    let url = format!("{base_url}/threads/{thread_id}/messages?order=desc&limit=20");
    let response = maybe_force_connection_close(
        apply_openai_auth_headers(client.get(&url), provider, config, copilot_initiator),
        force_connection_close,
    )
    .send()
    .await?;
    if !response.status().is_success() {
        let status = response.status();
        let retry_after_ms = extract_retry_after_ms(Some(response.headers()), "");
        let text = response.text().await.unwrap_or_default();
        return Err(classify_http_failure_with_retry_after(
            status,
            provider,
            &text,
            retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
        ));
    }
    let payload: serde_json::Value = response.json().await?;
    let data = payload
        .get("data")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("native assistant message list returned no data array"))?;
    for message in data {
        if message.get("role").and_then(|value| value.as_str()) != Some("assistant") {
            continue;
        }
        if let Some(content_blocks) = message.get("content").and_then(|value| value.as_array()) {
            let text = content_blocks
                .iter()
                .filter_map(|block| {
                    block
                        .get("text")
                        .and_then(|value| value.get("value").or(Some(value)))
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
        if let Some(text) = message.get("content").and_then(|value| value.as_str()) {
            if !text.trim().is_empty() {
                return Ok(text.to_string());
            }
        }
    }
    Err(anyhow::anyhow!(
        "native assistant thread did not contain a completed assistant message"
    ))
}

pub(crate) async fn run_openai_chat_completions(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    copilot_initiator: CopilotInitiator,
    force_connection_close: bool,
    opencode_session_id: Option<&str>,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let url = build_chat_completion_url(&config.base_url);
    let mut body =
        build_openai_chat_completions_body(provider, config, system_prompt, messages, tools)?;
    apply_openrouter_session_id(provider, opencode_session_id, &mut body);

    let req = apply_openrouter_session_header(
        apply_opencode_go_headers(
            apply_dashscope_coding_plan_sdk_headers(
                build_openai_auth_request(
                    client,
                    &url,
                    provider,
                    config,
                    copilot_initiator,
                    force_connection_close,
                ),
                provider,
                &config.base_url,
                ApiType::OpenAI,
            ),
            provider,
            opencode_session_id,
        ),
        provider,
        opencode_session_id,
    );

    let response = req.body(body.to_string()).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let retry_after_ms = extract_retry_after_ms(Some(response.headers()), "");
        let text = response.text().await.unwrap_or_default();
        return Err(classify_http_failure_with_retry_after(
            status,
            provider,
            &text,
            retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
        ));
    }

    parse_openai_sse(response, tx).await
}

fn normalized_openrouter_provider_slugs(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|existing| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn apply_openrouter_provider_routing(
    provider: &str,
    config: &ProviderConfig,
    body: &mut serde_json::Value,
) {
    if provider != zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        return;
    }

    let order = normalized_openrouter_provider_slugs(&config.openrouter_provider_order);
    let ignore = normalized_openrouter_provider_slugs(&config.openrouter_provider_ignore);
    if order.is_empty() && ignore.is_empty() && config.openrouter_allow_fallbacks.is_none() {
        return;
    }

    let mut provider_preferences = serde_json::Map::new();
    if !order.is_empty() {
        provider_preferences.insert("order".to_string(), serde_json::json!(order));
    }
    if !ignore.is_empty() {
        provider_preferences.insert("ignore".to_string(), serde_json::json!(ignore));
    }
    if let Some(allow_fallbacks) = config.openrouter_allow_fallbacks {
        provider_preferences.insert(
            "allow_fallbacks".to_string(),
            serde_json::Value::Bool(allow_fallbacks),
        );
    }

    body["provider"] = serde_json::Value::Object(provider_preferences);
}

const OPENROUTER_SESSION_ID_MAX_LEN: usize = 256;

fn normalized_openrouter_session_id(session_id: Option<&str>) -> Option<String> {
    let session_id = session_id?.trim();
    if session_id.is_empty()
        || session_id.len() > OPENROUTER_SESSION_ID_MAX_LEN
        || reqwest::header::HeaderValue::from_str(session_id).is_err()
    {
        return None;
    }
    Some(session_id.to_owned())
}

pub(crate) fn apply_openrouter_session_id(
    provider: &str,
    session_id: Option<&str>,
    body: &mut serde_json::Value,
) {
    if provider != zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        return;
    }
    let Some(session_id) = normalized_openrouter_session_id(session_id) else {
        return;
    };
    if let Some(object) = body.as_object_mut() {
        object.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id),
        );
    }
}

pub(crate) fn apply_openrouter_session_header(
    req: reqwest::RequestBuilder,
    provider: &str,
    session_id: Option<&str>,
) -> reqwest::RequestBuilder {
    if provider != zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        return req;
    }
    let Some(session_id) = normalized_openrouter_session_id(session_id) else {
        return req;
    };
    req.header("x-session-id", session_id)
}

pub(crate) fn build_openai_chat_completions_body(
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
) -> Result<serde_json::Value> {
    let include_tool_reasoning_content =
        chat_completions_replays_tool_reasoning_content(provider, &config.model);
    let include_non_tool_reasoning_content =
        chat_completions_replays_non_tool_reasoning_content(provider);
    let include_non_tool_reasoning_content_after_tool_call =
        chat_completions_replays_non_tool_reasoning_content_after_tool_call(
            provider,
            &config.model,
        );
    let synthesize_missing_tool_reasoning_content =
        chat_completions_repairs_missing_tool_reasoning_content(provider, config);
    let all_messages = build_chat_completion_messages_with_options(
        system_prompt,
        messages,
        include_tool_reasoning_content,
        synthesize_missing_tool_reasoning_content,
        include_non_tool_reasoning_content,
        include_non_tool_reasoning_content_after_tool_call,
    )?;

    let mut body = serde_json::json!({
        "model": super::openai_transport::huggingface_routed_model(
            provider,
            &config.model,
            config.huggingface_provider.as_deref(),
        ),
        "messages": all_messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });

    if !tools.is_empty() {
        body["tools"] = serde_json::to_value(tools)?;
        body["tool_choice"] = serde_json::json!("auto");
    }

    if let Some(ref schema) = config.response_schema {
        if matches!(provider, zorai_shared::providers::PROVIDER_ID_OPENAI)
            && (config.model.contains("gpt-4o")
                || config.model.contains("gpt-4.1")
                || config.model.contains("gpt-5")
                || config.model.contains("gpt-6")
                || config.model.starts_with("o"))
        {
            body["response_format"] = serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "structured_output",
                    "strict": true,
                    "schema": schema,
                }
            });
        } else {
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }
    }

    if dashscope_openai_uses_enable_thinking(provider, &config.model) {
        body["enable_thinking"] =
            serde_json::Value::Bool(normalize_reasoning_effort(&config.reasoning_effort).is_some());
    } else if openai_reasoning_supported(provider, &config.model) {
        if let Some(effort) = normalize_reasoning_effort(&config.reasoning_effort) {
            body["reasoning_effort"] = serde_json::Value::String(effort.clone());
            body["reasoning"] = serde_json::json!({ "effort": effort });
        }
    }

    if provider == zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        body["usage"] = serde_json::json!({ "include": true });
    }

    apply_openrouter_provider_routing(provider, config, &mut body);
    Ok(body)
}

/// DeepSeek thinking models require the prior turn's `reasoning_content` to be
/// replayed. Besides the dedicated DeepSeek provider, the OpenCode Go/Zen gateways
/// serve DeepSeek models (`deepseek-*`) that carry the same requirement.
fn behaves_as_deepseek_for_reasoning(provider: &str, model: &str) -> bool {
    provider == zorai_shared::providers::PROVIDER_ID_DEEPSEEK
        || ((provider == zorai_shared::providers::PROVIDER_ID_OPENCODE_GO
            || provider == zorai_shared::providers::PROVIDER_ID_OPENCODE_ZEN)
            && model.starts_with("deepseek"))
}

fn chat_completions_replays_tool_reasoning_content(provider: &str, model: &str) -> bool {
    provider == zorai_shared::providers::PROVIDER_ID_OPENROUTER
        || behaves_as_deepseek_for_reasoning(provider, model)
}

fn chat_completions_replays_non_tool_reasoning_content(provider: &str) -> bool {
    provider == zorai_shared::providers::PROVIDER_ID_OPENROUTER
}

fn chat_completions_replays_non_tool_reasoning_content_after_tool_call(
    provider: &str,
    model: &str,
) -> bool {
    behaves_as_deepseek_for_reasoning(provider, model)
}

fn chat_completions_repairs_missing_tool_reasoning_content(
    provider: &str,
    config: &ProviderConfig,
) -> bool {
    behaves_as_deepseek_for_reasoning(provider, &config.model)
        || (provider == zorai_shared::providers::PROVIDER_ID_OPENROUTER
            && normalize_reasoning_effort(&config.reasoning_effort).is_some())
}

pub(crate) fn messages_to_responses_input(
    provider: &str,
    messages: &[ApiMessage],
    previous_response_id: Option<&str>,
) -> Vec<serde_json::Value> {
    let local_function_call_ids: std::collections::HashSet<&str> = messages
        .iter()
        .filter(|message| message.role == "assistant")
        .filter_map(|message| message.tool_calls.as_ref())
        .flat_map(|tool_calls| tool_calls.iter())
        .map(|tool_call| tool_call.id.as_str())
        .filter(|call_id| !call_id.trim().is_empty())
        .collect();

    let emitted_tool_output_call_ids: std::collections::HashSet<&str> = messages
        .iter()
        .filter(|message| message.role == "tool")
        .filter(|_message| {
            !(provider == zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT
                && previous_response_id.is_some())
        })
        .filter_map(|message| message.tool_call_id.as_deref())
        .filter(|call_id| !call_id.trim().is_empty())
        .filter(|call_id| {
            previous_response_id.is_some() || local_function_call_ids.contains(call_id)
        })
        .collect();

    messages
        .iter()
        .flat_map(|message| match message.role.as_str() {
            "user" => vec![serde_json::json!({
                "role": message.role,
                "content": match &message.content {
                    ApiContent::Text(text) => serde_json::Value::String(text.clone()),
                    ApiContent::Blocks(blocks) => serde_json::Value::Array(blocks.clone()),
                }
            })],
            "assistant" => {
                let mut items = Vec::new();
                let content = match &message.content {
                    ApiContent::Text(text) => serde_json::Value::String(text.clone()),
                    ApiContent::Blocks(blocks) => serde_json::Value::Array(blocks.clone()),
                };
                let has_non_empty_content = match &content {
                    serde_json::Value::String(text) => !text.trim().is_empty(),
                    serde_json::Value::Array(blocks) => !blocks.is_empty(),
                    _ => false,
                };
                if has_non_empty_content || message.tool_calls.as_ref().is_none_or(Vec::is_empty) {
                    items.push(serde_json::json!({
                        "role": message.role,
                        "content": content,
                    }));
                }
                if let Some(tool_calls) = &message.tool_calls {
                    for tool_call in tool_calls {
                        if !emitted_tool_output_call_ids.contains(tool_call.id.as_str()) {
                            continue;
                        }
                        items.push(serde_json::json!({
                            "type": "function_call",
                            "call_id": tool_call.id,
                            "name": tool_call.function.name,
                            "arguments": tool_call.function.arguments,
                        }));
                    }
                }
                items
            }
            "tool" => {
                if provider == zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT
                    && previous_response_id.is_some()
                {
                    return Vec::new();
                }
                message
                    .tool_call_id
                    .as_ref()
                    .filter(|call_id| emitted_tool_output_call_ids.contains(call_id.as_str()))
                    .map(|call_id| {
                        vec![serde_json::json!({
                            "type": "function_call_output",
                            "call_id": call_id,
                            "output": match &message.content {
                                ApiContent::Text(text) => serde_json::Value::String(text.clone()),
                                ApiContent::Blocks(blocks) => serde_json::Value::Array(blocks.clone()),
                            }
                        })]
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        })
        .collect()
}

pub(crate) fn extract_reasoning_summary_text(item: &serde_json::Value) -> Option<String> {
    let summary = item.get("summary")?.as_array()?;
    let combined = summary
        .iter()
        .filter_map(|part| {
            let part_type = part.get("type").and_then(|value| value.as_str());
            match part_type {
                Some("summary_text") => part.get("text").and_then(|value| value.as_str()),
                _ => None,
            }
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!combined.is_empty()).then_some(combined)
}

fn build_anthropic_message_content(message: &ApiMessage) -> serde_json::Value {
    if message.role == "assistant"
        && message
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
    {
        let mut blocks = Vec::new();
        if let ApiContent::Text(text) = &message.content {
            if !text.is_empty() {
                blocks.push(serde_json::json!({
                    "type": "text",
                    "text": text,
                }));
            }
        }
        if let Some(tool_calls) = &message.tool_calls {
            for tool_call in tool_calls {
                let input =
                    serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments)
                        .unwrap_or_else(|_| {
                            serde_json::json!({
                                "_raw_arguments": tool_call.function.arguments,
                            })
                        });
                blocks.push(serde_json::json!({
                    "type": "tool_use",
                    "id": tool_call.id,
                    "name": tool_call.function.name,
                    "input": input,
                }));
            }
        }
        serde_json::Value::Array(blocks)
    } else {
        match &message.content {
            ApiContent::Text(text) => serde_json::json!(text),
            ApiContent::Blocks(blocks) => {
                serde_json::json!(map_api_blocks_to_anthropic_blocks(blocks))
            }
        }
    }
}

fn map_api_blocks_to_anthropic_blocks(blocks: &[serde_json::Value]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| {
            let block_type = block
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            match block_type {
                "input_text" | "text" => {
                    block
                        .get("text")
                        .and_then(|value| value.as_str())
                        .map(|text| {
                            serde_json::json!({
                                "type": "text",
                                "text": text,
                            })
                        })
                }
                "input_image" => {
                    let image_url = block.get("image_url")?;
                    let image_url = image_url
                        .get("url")
                        .and_then(|value| value.as_str())
                        .or_else(|| image_url.as_str())?;
                    let (media_type, data) = image_url
                        .strip_prefix("data:")
                        .and_then(parse_data_url_payload)?;
                    Some(serde_json::json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data,
                        }
                    }))
                }
                "input_audio" => {
                    let input_audio = block.get("input_audio")?;
                    let data = input_audio.get("data").and_then(|value| value.as_str())?;
                    let format = input_audio
                        .get("format")
                        .and_then(|value| value.as_str())
                        .unwrap_or("wav");
                    let media_type = anthropic_audio_media_type(format);
                    Some(serde_json::json!({
                        "type": "audio",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data,
                        }
                    }))
                }
                _ => None,
            }
        })
        .collect()
}

fn parse_data_url_payload(data_url_body: &str) -> Option<(String, String)> {
    let (header, payload) = data_url_body.split_once(',')?;
    if !header.contains(";base64") {
        return None;
    }
    let media_type = header
        .split(';')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    Some((media_type, payload.trim().to_string()))
}

fn anthropic_audio_media_type(format: &str) -> &'static str {
    match format.trim().to_ascii_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "mp4" | "m4a" => "audio/mp4",
        "webm" => "audio/webm",
        _ => "audio/wav",
    }
}

/// Repair message sequence so every assistant tool_use is immediately followed
/// by its tool_results, and no orphaned tool results appear without a parent.
/// Drops broken pairs entirely rather than sending malformed sequences.
pub(crate) fn sanitize_api_messages(messages: &[ApiMessage]) -> Vec<ApiMessage> {
    let mut out: Vec<ApiMessage> = Vec::with_capacity(messages.len());
    let mut i = 0;
    while i < messages.len() {
        let msg = &messages[i];
        if msg.role == "assistant" {
            if let Some(tool_calls) = &msg.tool_calls {
                if !tool_calls.is_empty() {
                    let expected_ids: std::collections::HashSet<&str> =
                        tool_calls.iter().map(|tc| tc.id.as_str()).collect();
                    let mut results = Vec::new();
                    let mut matched_ids = std::collections::HashSet::new();
                    let mut j = i + 1;
                    while j < messages.len() && messages[j].role == "tool" {
                        if let Some(id) = messages[j].tool_call_id.as_deref() {
                            if expected_ids.contains(id) {
                                results.push(messages[j].clone());
                                matched_ids.insert(id);
                            }
                        }
                        j += 1;
                    }
                    let has_complete_batch = matched_ids.len() == expected_ids.len();
                    let saw_no_followup_messages = j == i + 1;
                    let is_unanswered_latest_tool_turn =
                        saw_no_followup_messages && j == messages.len();
                    if has_complete_batch {
                        out.push(msg.clone());
                        out.extend(results);
                    } else if is_unanswered_latest_tool_turn {
                        out.push(msg.clone());
                    } else {
                        tracing::warn!(
                            "sanitize_api_messages: dropping incomplete assistant tool_use with {} calls (matched {}/{})",
                            tool_calls.len(),
                            matched_ids.len(),
                            expected_ids.len()
                        );
                    }
                    i = j;
                    continue;
                }
            }
            if api_content_has_non_empty_payload(&msg.content)
                || msg
                    .reasoning
                    .as_ref()
                    .is_some_and(|reasoning| !reasoning.trim().is_empty())
            {
                out.push(msg.clone());
            } else {
                tracing::warn!(
                    "sanitize_api_messages: dropping empty assistant message without tool calls"
                );
            }
        } else if msg.role == "tool" {
            tracing::warn!(
                "sanitize_api_messages: dropping orphaned tool result (id={:?})",
                msg.tool_call_id
            );
        } else {
            out.push(msg.clone());
        }
        i += 1;
    }
    out
}

fn api_content_has_non_empty_payload(content: &ApiContent) -> bool {
    match content {
        ApiContent::Text(text) => !text.trim().is_empty(),
        ApiContent::Blocks(blocks) => blocks.iter().any(api_block_has_non_empty_payload),
    }
}

fn api_block_has_non_empty_payload(block: &serde_json::Value) -> bool {
    block.as_str().is_some_and(|text| !text.trim().is_empty())
        || block
            .get("text")
            .and_then(|value| value.as_str())
            .is_some_and(|text| !text.trim().is_empty())
        || block
            .get("image_url")
            .and_then(|value| value.as_str())
            .is_some_and(|url| !url.trim().is_empty())
        || block
            .get("input_audio")
            .and_then(|value| {
                value
                    .get("data")
                    .or_else(|| value.get("url"))
                    .and_then(|payload| payload.as_str())
            })
            .is_some_and(|payload| !payload.trim().is_empty())
}

pub(crate) fn build_anthropic_messages(messages: &[ApiMessage]) -> Vec<serde_json::Value> {
    let messages = sanitize_api_messages(messages);

    let mut out = Vec::new();
    let mut index = 0usize;

    while index < messages.len() {
        let message = &messages[index];
        if message.role == "tool" {
            let mut blocks = Vec::new();
            while index < messages.len() && messages[index].role == "tool" {
                let tool_message = &messages[index];
                if let Some(tool_use_id) = tool_message.tool_call_id.as_ref() {
                    blocks.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": match &tool_message.content {
                            ApiContent::Text(text) => serde_json::Value::String(text.clone()),
                            ApiContent::Blocks(blocks) => serde_json::Value::Array(blocks.clone()),
                        }
                    }));
                }
                index += 1;
            }
            if !blocks.is_empty() {
                out.push(serde_json::json!({
                    "role": "user",
                    "content": blocks,
                }));
            }
            continue;
        }

        let role = match message.role.as_str() {
            "system" => "user",
            other => other,
        };
        out.push(serde_json::json!({
            "role": role,
            "content": build_anthropic_message_content(message),
        }));
        index += 1;
    }

    out
}

pub(crate) async fn run_openai_responses(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    previous_response_id: Option<&str>,
    upstream_thread_id: Option<&str>,
    copilot_initiator: CopilotInitiator,
    force_connection_close: bool,
    opencode_session_id: Option<&str>,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let codex_auth = resolve_openai_codex_request_auth(client, provider, config).await?;
    let url = if codex_auth.is_some() {
        "https://chatgpt.com/backend-api/codex/responses".to_string()
    } else {
        build_responses_url(&config.base_url)
    };
    let mut body = build_openai_responses_body(
        provider,
        config,
        system_prompt,
        messages,
        tools,
        previous_response_id,
        codex_auth.is_some(),
    );
    apply_openrouter_session_id(provider, opencode_session_id, &mut body);

    let req = if let Some(codex_auth) = codex_auth {
        let req = maybe_force_connection_close(
            client
                .post(&url)
                .header("Content-Type", "application/json")
                .header(
                    "Authorization",
                    format!("Bearer {}", codex_auth.access_token),
                )
                .header("chatgpt-account-id", codex_auth.account_id)
                .header("OpenAI-Beta", "responses=experimental")
                .header("originator", "zorai"),
            force_connection_close,
        );
        let upstream_thread_id = upstream_thread_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let req = if let Some(upstream_thread_id) = upstream_thread_id {
            req.header("session_id", upstream_thread_id)
                .header("x-client-request-id", upstream_thread_id)
        } else {
            req
        };
        req
    } else {
        apply_openrouter_session_header(
            apply_opencode_go_headers(
                build_openai_auth_request(
                    client,
                    &url,
                    provider,
                    config,
                    copilot_initiator,
                    force_connection_close,
                ),
                provider,
                opencode_session_id,
            ),
            provider,
            opencode_session_id,
        )
    };
    let response = req.body(body.to_string()).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let retry_after_ms = extract_retry_after_ms(Some(response.headers()), "");
        let text = response.text().await.unwrap_or_default();
        let is_compatibility_error = matches!(
            status,
            reqwest::StatusCode::BAD_REQUEST
                | reqwest::StatusCode::NOT_FOUND
                | reqwest::StatusCode::METHOD_NOT_ALLOWED
                | reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE
                | reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
        if is_compatibility_error {
            return Err(classify_http_failure_with_retry_after(
                status,
                provider,
                &text,
                retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
            ));
        }
        return Err(classify_http_failure_with_retry_after(
            status,
            provider,
            &text,
            retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
        ));
    }

    parse_openai_responses_sse(response, provider, tx).await
}
