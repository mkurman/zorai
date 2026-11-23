use super::*;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const CLAUDE_BINARY: &str = "claude";
const MODEL_ONLY_MARKER: &str = "model-only";
const PERMISSION_MODE_ENV: &str = "ZORAI_CLAUDE_CODE_PERMISSION_MODE";

pub(crate) fn claude_cli_available() -> bool {
    which::which(CLAUDE_BINARY).is_ok()
}

fn claude_permission_mode() -> Option<String> {
    std::env::var(PERMISSION_MODE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn summarize_tool_input(input: Option<&serde_json::Value>) -> String {
    let Some(object) = input.and_then(|value| value.as_object()) else {
        return String::new();
    };
    let summary = [
        "file_path",
        "path",
        "command",
        "pattern",
        "query",
        "url",
        "description",
        "prompt",
    ]
    .iter()
    .find_map(|key| {
        object
            .get(*key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
    .unwrap_or("");
    let summary: String = summary
        .split('\n')
        .next()
        .unwrap_or("")
        .chars()
        .take(80)
        .collect();
    summary
}

pub(crate) struct ClaudeCompactOutcome {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub(crate) async fn compact_claude_code_cli_session(
    session_id: &str,
    model: Option<&str>,
    working_dir: Option<&str>,
) -> Result<ClaudeCompactOutcome> {
    let binary = which::which(CLAUDE_BINARY)
        .map_err(|_| anyhow::anyhow!("claude CLI binary not found on PATH"))?;

    let mut command = tokio::process::Command::new(&binary);
    command
        .arg("-p")
        .arg("--resume")
        .arg(session_id)
        .arg("--output-format")
        .arg("json");
    if let Some(mode) = claude_permission_mode() {
        command.arg("--permission-mode").arg(mode);
    }
    if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
        command.arg("--model").arg(model);
    }
    if let Some(dir) = working_dir.filter(|value| !value.trim().is_empty()) {
        command.current_dir(dir);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn claude CLI ({})", binary.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"/compact").await?;
        let _ = stdin.shutdown().await;
    }

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "claude /compact exited with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|err| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::anyhow!(
            "claude /compact returned unparseable output ({err}); stdout: {}; stderr: {}",
            stdout.trim().chars().take(240).collect::<String>(),
            stderr.trim().chars().take(240).collect::<String>()
        )
    })?;
    if value
        .get("is_error")
        .and_then(|field| field.as_bool())
        .unwrap_or(false)
    {
        let detail = value
            .get("result")
            .and_then(|field| field.as_str())
            .unwrap_or("unknown error");
        return Err(anyhow::anyhow!(
            "claude /compact reported an error: {detail}"
        ));
    }

    let usage = value.get("usage");
    Ok(ClaudeCompactOutcome {
        input_tokens: usage
            .and_then(|usage| usage.get("input_tokens"))
            .and_then(|field| field.as_u64())
            .unwrap_or(0),
        output_tokens: usage
            .and_then(|usage| usage.get("output_tokens"))
            .and_then(|field| field.as_u64())
            .unwrap_or(0),
    })
}

pub(crate) async fn run_claude_code_cli(
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    upstream_thread_id: Option<&str>,
    working_dir: Option<&str>,
    permission_mode: Option<&str>,
    tx: &mpsc::Sender<Result<CompletionChunk>>,
) -> Result<()> {
    let binary = which::which(CLAUDE_BINARY).map_err(|_| {
        transport_incompatibility_error(
            provider,
            "claude CLI binary not found on PATH; install Claude Code to use this provider",
        )
    })?;

    let user_text = messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .and_then(api_message_to_text)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("claude code cli requires a user message"))?;

    let resuming = upstream_thread_id
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let session_id = upstream_thread_id
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let model_only = config
        .assistant_id
        .trim()
        .eq_ignore_ascii_case(MODEL_ONLY_MARKER);

    let mut command = tokio::process::Command::new(&binary);
    command
        .arg("-p")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose");
    if resuming {
        command.arg("--resume").arg(&session_id);
    } else {
        command.arg("--session-id").arg(&session_id);
    }
    if !config.model.trim().is_empty() {
        command.arg("--model").arg(&config.model);
    }
    let effort = config.reasoning_effort.trim().to_ascii_lowercase();
    if matches!(effort.as_str(), "low" | "medium" | "high" | "xhigh" | "max") {
        command.arg("--effort").arg(&effort);
    }
    if model_only {
        command.arg("--tools").arg("");
        if !system_prompt.trim().is_empty() {
            command.arg("--append-system-prompt").arg(system_prompt);
        }
    } else if let Some(mode) = permission_mode
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(claude_permission_mode)
    {
        command.arg("--permission-mode").arg(mode);
    }
    if let Some(dir) = working_dir.filter(|value| !value.trim().is_empty()) {
        command.current_dir(dir);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to spawn claude CLI ({}) for provider '{provider}'",
            binary.display()
        )
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(user_text.as_bytes()).await?;
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
        .ok_or_else(|| anyhow::anyhow!("claude CLI produced no stdout stream"))?;
    let mut reader = BufReader::new(stdout).lines();

    let mut assembled = String::new();
    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut cache_creation_input_tokens: Option<u64> = None;
    let mut cache_read_input_tokens: Option<u64> = None;
    let mut upstream_model: Option<String> = None;
    let mut stop_reason: Option<String> = None;
    let mut total_cost_usd: Option<f64> = None;
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
                if let Some(model) = event
                    .pointer("/message/model")
                    .and_then(|value| value.as_str())
                {
                    upstream_model = Some(model.to_string());
                }
                if let Some(blocks) = event
                    .pointer("/message/content")
                    .and_then(|value| value.as_array())
                {
                    for block in blocks {
                        let delta = match block.get("type").and_then(|value| value.as_str()) {
                            Some("text") => block
                                .get("text")
                                .and_then(|value| value.as_str())
                                .map(ToOwned::to_owned),
                            Some("tool_use") => block
                                .get("name")
                                .and_then(|value| value.as_str())
                                .map(|name| {
                                    let summary = summarize_tool_input(block.get("input"));
                                    if summary.is_empty() {
                                        format!("\n\n› **{name}**\n\n")
                                    } else {
                                        format!("\n\n› **{name}** `{summary}`\n\n")
                                    }
                                }),
                            _ => None,
                        };
                        if let Some(delta) = delta.filter(|value| !value.is_empty()) {
                            assembled.push_str(&delta);
                            let _ = tx
                                .send(Ok(CompletionChunk::Delta {
                                    content: delta,
                                    reasoning: None,
                                }))
                                .await;
                        }
                    }
                }
            }
            Some("result") => {
                if let Some(usage) = event.get("usage") {
                    input_tokens = usage
                        .get("input_tokens")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(input_tokens);
                    output_tokens = usage
                        .get("output_tokens")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(output_tokens);
                    cache_creation_input_tokens = usage
                        .get("cache_creation_input_tokens")
                        .and_then(|value| value.as_u64())
                        .or(cache_creation_input_tokens);
                    cache_read_input_tokens = usage
                        .get("cache_read_input_tokens")
                        .and_then(|value| value.as_u64())
                        .or(cache_read_input_tokens);
                }
                stop_reason = event
                    .get("stop_reason")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned);
                total_cost_usd = event
                    .get("total_cost_usd")
                    .and_then(|value| value.as_f64())
                    .or(total_cost_usd);
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
        return Err(anyhow::anyhow!("claude CLI reported an error: {error}"));
    }
    if !status.success() {
        let detail = stderr_text.trim();
        let detail = if detail.is_empty() {
            format!("claude CLI exited with status {status}")
        } else {
            format!("claude CLI exited with status {status}: {detail}")
        };
        return Err(anyhow::anyhow!(detail));
    }

    let content = result_text
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(assembled);

    let _ = tx
        .send(Ok(CompletionChunk::Done {
            reasoning_tokens: None,
            content,
            reasoning: None,
            input_tokens: input_tokens
                + cache_creation_input_tokens.unwrap_or(0)
                + cache_read_input_tokens.unwrap_or(0),
            output_tokens,
            cost_usd: total_cost_usd,
            stop_reason,
            stop_sequence: None,
            cache_creation_input_tokens,
            cache_read_input_tokens,
            server_tool_use: None,
            response_id: None,
            request_id: None,
            upstream_model,
            upstream_role: None,
            upstream_message_type: None,
            upstream_container: None,
            upstream_message: None,
            provider_final_result: None,
            upstream_thread_id: Some(session_id),
        }))
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::summarize_tool_input;

    #[test]
    fn summarize_tool_input_prefers_target_path_then_command() {
        assert_eq!(
            summarize_tool_input(Some(&serde_json::json!({ "file_path": "crates/x/y.rs" }))),
            "crates/x/y.rs"
        );
        assert_eq!(
            summarize_tool_input(Some(
                &serde_json::json!({ "command": "cargo build -p zorai-daemon" })
            )),
            "cargo build -p zorai-daemon"
        );
        assert_eq!(
            summarize_tool_input(Some(&serde_json::json!({ "unknown_key": "v" }))),
            ""
        );
        assert_eq!(summarize_tool_input(None), "");
    }

    #[test]
    fn summarize_tool_input_takes_first_line_and_truncates() {
        let long = "a".repeat(200);
        let out = summarize_tool_input(Some(
            &serde_json::json!({ "command": format!("{long}\nsecond line") }),
        ));
        assert_eq!(out.chars().count(), 80);
        assert!(!out.contains('\n'));
    }
}
