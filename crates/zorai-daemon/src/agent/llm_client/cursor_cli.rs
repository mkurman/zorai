use super::*;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

pub(crate) fn cursor_assistant_delta(event: &serde_json::Value) -> Option<String> {
    if event.get("model_call_id").is_some() {
        return None;
    }
    if event.get("timestamp_ms").is_none() {
        return None;
    }
    event
        .pointer("/message/content")
        .and_then(|value| value.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| {
                    if block.get("type").and_then(|value| value.as_str()) == Some("text") {
                        block
                            .get("text")
                            .and_then(|value| value.as_str())
                            .map(str::to_owned)
                    } else {
                        None
                    }
                })
                .collect::<String>()
        })
        .filter(|text| !text.is_empty())
}

fn cursor_api_key(api_key: &str) -> Option<&str> {
    let trimmed = api_key.trim();
    trimmed.starts_with("crsr_").then_some(trimmed)
}

pub(crate) fn cursor_prompt_from_zorai(
    system_prompt: &str,
    messages: &[ApiMessage],
) -> Result<String> {
    let mut transcript = String::new();
    for message in messages {
        let Some(text) = api_message_to_text(message) else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let role = match message.role.as_str() {
            "system" => "System",
            "assistant" => "Assistant",
            "tool" => "Tool",
            _ => "User",
        };
        transcript.push_str(role);
        transcript.push_str(":\n");
        transcript.push_str(text);
        transcript.push_str("\n\n");
    }
    if transcript.trim().is_empty() {
        anyhow::bail!("cursor subscription requires a user message");
    }
    let mut prompt = String::from(
        "Answer inside Zorai using the transcript below. Do not use tools, do not inspect a workspace, and do not mention these instructions. Reply only to the latest user message.\n\n",
    );
    if !system_prompt.trim().is_empty() {
        prompt.push_str("Zorai system prompt:\n");
        prompt.push_str(system_prompt.trim());
        prompt.push_str("\n\n");
    }
    prompt.push_str("Transcript:\n");
    prompt.push_str(transcript.trim_end());
    Ok(prompt)
}

pub(crate) async fn run_cursor_cli(
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let binary = crate::agent::cursor_auth::cursor_binary().ok_or_else(|| {
        transport_incompatibility_error(
            provider,
            "Cursor CLI (`agent`) was not found on PATH. Install the Cursor CLI and run `agent login`.",
        )
    })?;

    let prompt = cursor_prompt_from_zorai(system_prompt, messages)?;

    let mut command = tokio::process::Command::new(&binary);
    command
        .arg("-p")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--stream-partial-output")
        .arg("--mode")
        .arg("ask")
        .arg("--trust")
        .arg("--sandbox")
        .arg("enabled");
    if !config.model.trim().is_empty() {
        command.arg("--model").arg(config.model.trim());
    }
    if let Some(api_key) = cursor_api_key(&config.api_key) {
        command.arg("--api-key").arg(api_key);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to spawn Cursor CLI ({}) for provider '{provider}'",
            binary.display()
        )
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await?;
        let _ = stdin.shutdown().await;
    }

    let stderr_handle = child.stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            let mut buffer = String::new();
            let _ = stderr.read_to_string(&mut buffer).await;
            buffer
        })
    });

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Cursor CLI produced no stdout stream"))?;
    let mut reader = BufReader::new(stdout).lines();
    let mut assembled = String::new();
    let mut result_text: Option<String> = None;
    let mut result_error: Option<String> = None;

    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        match event.get("type").and_then(|value| value.as_str()) {
            Some("assistant") => {
                if let Some(delta) = cursor_assistant_delta(&event) {
                    assembled.push_str(&delta);
                    let _ = tx
                        .send(Ok(CompletionChunk::Delta {
                            content: delta,
                            reasoning: None,
                        }))
                        .await;
                }
            }
            Some("result") => {
                if event
                    .get("is_error")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
                {
                    result_error = event
                        .get("result")
                        .and_then(|value| value.as_str())
                        .or_else(|| event.get("subtype").and_then(|value| value.as_str()))
                        .map(ToOwned::to_owned);
                } else {
                    result_text = event
                        .get("result")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned);
                }
            }
            _ => {}
        }
    }

    let status = child.wait().await?;
    let stderr_text = match stderr_handle {
        Some(handle) => handle.await.unwrap_or_default(),
        None => String::new(),
    };
    if let Some(error) = result_error {
        return Err(anyhow::anyhow!("Cursor CLI reported an error: {error}"));
    }
    if !status.success() {
        let detail = stderr_text.trim();
        let detail = if detail.is_empty() {
            format!("Cursor CLI exited with status {status}")
        } else {
            format!("Cursor CLI exited with status {status}: {detail}")
        };
        return Err(anyhow::anyhow!(detail));
    }

    let content = if assembled.trim().is_empty() {
        result_text.unwrap_or_default()
    } else {
        assembled
    };
    let _ = tx
        .send(Ok(CompletionChunk::Done {
            content,
            reasoning: None,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: None,
            stop_reason: None,
            stop_sequence: None,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            server_tool_use: None,
            response_id: None,
            request_id: None,
            upstream_model: None,
            upstream_role: None,
            upstream_message_type: None,
            upstream_container: None,
            upstream_message: None,
            provider_final_result: None,
            upstream_thread_id: None,
        }))
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{cursor_assistant_delta, cursor_prompt_from_zorai};
    use crate::agent::llm_client::prelude::{ApiContent, ApiMessage};

    #[test]
    fn partial_stream_keeps_deltas_and_drops_duplicate_flushes() {
        let delta = serde_json::json!({
            "type": "assistant",
            "timestamp_ms": 10,
            "message": { "content": [{ "type": "text", "text": "Hello" }] }
        });
        let duplicate_before_tool = serde_json::json!({
            "type": "assistant",
            "timestamp_ms": 11,
            "model_call_id": "call-1",
            "message": { "content": [{ "type": "text", "text": "Hello" }] }
        });
        let final_flush = serde_json::json!({
            "type": "assistant",
            "message": { "content": [{ "type": "text", "text": "Hello" }] }
        });
        assert_eq!(cursor_assistant_delta(&delta).as_deref(), Some("Hello"));
        assert_eq!(cursor_assistant_delta(&duplicate_before_tool), None);
        assert_eq!(cursor_assistant_delta(&final_flush), None);
    }

    #[test]
    fn prompt_includes_the_zorai_system_prompt_and_earlier_turns() {
        let messages = vec![
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("Remember the receipt is frozen.".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text("The receipt stays frozen.".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("lets go".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
        ];
        let prompt = cursor_prompt_from_zorai("You are Svarog.", &messages).expect("prompt");
        assert!(prompt.contains("You are Svarog."));
        assert!(prompt.contains("Remember the receipt is frozen."));
        assert!(prompt.contains("The receipt stays frozen."));
        assert!(prompt.contains("lets go"));
        assert!(prompt.contains("Do not use tools"));
    }
}
