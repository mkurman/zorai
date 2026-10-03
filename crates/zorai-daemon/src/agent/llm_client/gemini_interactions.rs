use super::gemini_stream::GeminiStreamAssembler;
use super::*;

pub(crate) const GEMINI_API_REVISION: &str = "2026-05-20";
pub(crate) const GEMINI_THOUGHT_MARKER: &str = "\u{1f}gemini-thought:";

pub(crate) fn gemini_interactions_url(base_url: &str, stream: bool) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if stream {
        format!("{base}/interactions?alt=sse")
    } else {
        format!("{base}/interactions")
    }
}

pub(crate) fn build_gemini_interaction_body(
    model: &str,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    stream: bool,
    previous_interaction_id: Option<&str>,
) -> serde_json::Value {
    let previous_interaction_id = previous_interaction_id
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let mut body = serde_json::json!({
        "model": model,
        "input": messages_to_interaction_input(messages),
        "store": true,
        "stream": stream,
    });
    if let Some(previous_interaction_id) = previous_interaction_id {
        body["previous_interaction_id"] =
            serde_json::Value::String(previous_interaction_id.to_string());
    } else {
        let system_prompt = system_prompt.trim();
        if !system_prompt.is_empty() {
            body["system_instruction"] = serde_json::Value::String(system_prompt.to_string());
        }
    }
    if !tools.is_empty() {
        body["tools"] =
            serde_json::Value::Array(tools.iter().map(tool_definition_to_json).collect());
    }
    if stream {
        body["generation_config"] = serde_json::json!({
            "thinking_summaries": "auto"
        });
    }
    body
}

pub(crate) async fn run_gemini(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    previous_interaction_id: Option<&str>,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let url = gemini_interactions_url(&config.base_url, true);
    let body = build_gemini_interaction_body(
        &config.model,
        system_prompt,
        messages,
        tools,
        true,
        previous_interaction_id,
    );
    let auth_method = get_provider_definition(provider)
        .map(|definition| definition.auth_method)
        .unwrap_or(AuthMethod::XGoogApiKey);
    let request = auth_method
        .apply(
            client
                .post(&url)
                .header("Content-Type", "application/json")
                .header("Api-Revision", GEMINI_API_REVISION),
            &config.api_key,
        )
        .body(body.to_string())
        .build()?;

    let response = client.execute(request).await?;
    if !response.status().is_success() {
        let status = response.status();
        let retry_after_ms = extract_retry_after_ms(Some(response.headers()), "");
        let mut text = response.text().await.unwrap_or_default();
        if previous_interaction_id.is_some()
            && status.is_client_error()
            && status != reqwest::StatusCode::TOO_MANY_REQUESTS
            && status != reqwest::StatusCode::UNAUTHORIZED
            && status != reqwest::StatusCode::FORBIDDEN
        {
            text.push_str(" previous_interaction_id invalid request");
        }
        return Err(classify_http_failure_with_retry_after(
            status,
            "Gemini",
            &text,
            retry_after_ms.or_else(|| extract_retry_after_ms(None, &text)),
        ));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if content_type.contains("application/json") && !content_type.contains("text/event-stream") {
        let payload = response.json::<serde_json::Value>().await?;
        let mut assembler = GeminiStreamAssembler::default();
        assembler.ingest_interaction(&payload)?;
        return send_chunks(tx, assembler.finish()?).await;
    }

    let mut assembler = GeminiStreamAssembler::default();
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
        buffer.push_str(&String::from_utf8_lossy(&chunk?));
        while let Some(split_at) = buffer.find('\n') {
            let line = buffer.drain(..=split_at).collect::<String>();
            let chunks = assembler.push_line(line.trim_end_matches(['\r', '\n']))?;
            send_chunks(tx, chunks).await?;
        }
    }
    if !buffer.trim().is_empty() {
        let chunks = assembler.push_line(buffer.trim())?;
        send_chunks(tx, chunks).await?;
    }
    send_chunks(tx, assembler.finish()?).await
}

async fn send_chunks(
    tx: &mpsc::Sender<Result<CompletionChunk>>,
    chunks: Vec<CompletionChunk>,
) -> Result<()> {
    for chunk in chunks {
        if tx.send(Ok(chunk)).await.is_err() {
            break;
        }
    }
    Ok(())
}

fn messages_to_interaction_input(messages: &[ApiMessage]) -> serde_json::Value {
    let mut steps = Vec::new();
    for message in messages {
        match message.role.as_str() {
            "system" | "user" => {
                let content = content_parts(&message.content);
                if !content.is_empty() {
                    steps.push(serde_json::json!({
                        "type": "user_input",
                        "content": content,
                    }));
                }
            }
            "assistant" => {
                let (summary, signature) =
                    split_stored_thought(message.reasoning.as_deref().unwrap_or(""));
                if let Some(signature) = signature {
                    steps.push(serde_json::json!({
                        "type": "thought",
                        "signature": signature,
                    }));
                }
                let _ = summary;
                let content = content_parts(&message.content);
                if !content.is_empty() {
                    steps.push(serde_json::json!({
                        "type": "model_output",
                        "content": content,
                    }));
                }
                if let Some(tool_calls) = &message.tool_calls {
                    for tool_call in tool_calls {
                        steps.push(serde_json::json!({
                            "type": "function_call",
                            "id": tool_call.id,
                            "name": tool_call.function.name,
                            "arguments": parse_arguments(&tool_call.function.arguments),
                        }));
                    }
                }
            }
            "tool" => {
                let Some(call_id) = message
                    .tool_call_id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty())
                else {
                    continue;
                };
                let result_text = text_from_content(&message.content);
                steps.push(serde_json::json!({
                    "type": "function_result",
                    "name": message.name.clone().unwrap_or_default(),
                    "call_id": call_id,
                    "result": [{ "type": "text", "text": result_text }],
                }));
            }
            _ => {}
        }
    }
    serde_json::Value::Array(steps)
}

fn tool_definition_to_json(tool: &ToolDefinition) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "name": tool.function.name,
        "description": tool.function.description,
        "parameters": tool.function.parameters,
    })
}

fn content_parts(content: &ApiContent) -> Vec<serde_json::Value> {
    match content {
        ApiContent::Text(text) => {
            if text.trim().is_empty() {
                Vec::new()
            } else {
                vec![serde_json::json!({ "type": "text", "text": text })]
            }
        }
        ApiContent::Blocks(blocks) => blocks.iter().filter_map(block_to_part).collect(),
    }
}

fn block_to_part(block: &serde_json::Value) -> Option<serde_json::Value> {
    let kind = block
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    match kind {
        "text" | "input_text" => block
            .get("text")
            .and_then(|value| value.as_str())
            .map(|text| serde_json::json!({ "type": "text", "text": text })),
        "image" | "input_image" => {
            let url = block
                .get("image_url")
                .and_then(|value| value.as_str())
                .or_else(|| block.get("url").and_then(|value| value.as_str()))
                .or_else(|| {
                    block
                        .pointer("/image_url/url")
                        .and_then(|value| value.as_str())
                })?;
            image_part_from_url(url)
        }
        "audio" | "input_audio" => {
            let url = block
                .get("url")
                .and_then(|value| value.as_str())
                .or_else(|| {
                    block
                        .pointer("/input_audio/url")
                        .and_then(|value| value.as_str())
                })?;
            Some(serde_json::json!({
                "type": "audio",
                "uri": url,
                "mime_type": mime_from_url(url, "audio/mp3"),
            }))
        }
        _ => None,
    }
}

fn image_part_from_url(url: &str) -> Option<serde_json::Value> {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("data:") {
        let (meta, data) = rest.split_once(',')?;
        let mime = meta
            .split(';')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or("image/png");
        return Some(serde_json::json!({
            "type": "image",
            "data": data,
            "mime_type": mime,
        }));
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(serde_json::json!({
            "type": "image",
            "uri": url,
            "mime_type": mime_from_url(url, "image/jpeg"),
        }));
    }
    None
}

fn mime_from_url(url: &str, fallback: &str) -> String {
    let lower = url.to_ascii_lowercase();
    if lower.contains(".png") {
        "image/png".to_string()
    } else if lower.contains(".webp") {
        "image/webp".to_string()
    } else if lower.contains(".gif") {
        "image/gif".to_string()
    } else if lower.contains(".mp3") {
        "audio/mp3".to_string()
    } else if lower.contains(".wav") {
        "audio/wav".to_string()
    } else {
        fallback.to_string()
    }
}

fn text_from_content(content: &ApiContent) -> String {
    match content {
        ApiContent::Text(text) => text.clone(),
        ApiContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(|value| value.as_str()))
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn parse_arguments(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({ "_raw": raw }))
}

pub(crate) fn split_stored_thought(reasoning: &str) -> (String, Option<String>) {
    match reasoning.split_once(GEMINI_THOUGHT_MARKER) {
        Some((summary, signature)) => {
            let signature = signature.trim();
            (
                summary.trim().to_string(),
                (!signature.is_empty()).then(|| signature.to_string()),
            )
        }
        None => (reasoning.trim().to_string(), None),
    }
}

pub(crate) fn reasoning_for_storage(summary: &str, signature: &str) -> Option<String> {
    let summary = summary.trim();
    let signature = signature.trim();
    if signature.is_empty() {
        return (!summary.is_empty()).then(|| summary.to_string());
    }
    if summary.is_empty() {
        Some(format!("{GEMINI_THOUGHT_MARKER}{signature}"))
    } else {
        Some(format!("{summary}\n{GEMINI_THOUGHT_MARKER}{signature}"))
    }
}

#[cfg(test)]
#[path = "gemini_interactions_tests.rs"]
mod tests;
