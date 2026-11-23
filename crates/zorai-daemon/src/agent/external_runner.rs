//! Manages external agent processes (OpenClaw, Hermes) as subprocesses.
//!
//! When `agent_backend` is set to "openclaw" or "hermes", the daemon routes
//! messages through the external agent's CLI rather than using the built-in
//! LLM client. The agent uses its own provider, tools, memory, and gateway
//! connections.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::types::AgentEvent;

#[path = "external_runner_helpers.rs"]
mod helpers;

pub(in crate::agent) use helpers::ensure_zorai_mcp_configured;
use helpers::{
    build_gateway_args, build_one_shot_args, check_zorai_mcp_configured, find_executable,
    is_tui_noise, parse_structured_response, strip_ansi,
};

/// Maximum time (seconds) to wait for a one-shot agent response.
const ONE_SHOT_TIMEOUT_SECS: u64 = 300;

#[derive(Debug)]
struct StreamCancelledError;

impl std::fmt::Display for StreamCancelledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stream cancelled")
    }
}

impl std::error::Error for StreamCancelledError {}

pub fn is_stream_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<StreamCancelledError>().is_some()
}

pub struct ExternalAgentRunner {
    agent_type: String,
    executable: Option<String>,
    event_tx: broadcast::Sender<AgentEvent>,
    gateway_process: tokio::sync::Mutex<Option<tokio::process::Child>>,
    /// Whether zorai-mcp is configured in this agent's MCP config.
    has_zorai_mcp: bool,
}

/// Information about an external agent's availability.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExternalAgentStatus {
    pub agent_type: String,
    pub available: bool,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub has_zorai_mcp: bool,
}

impl ExternalAgentRunner {
    pub fn new(agent_type: &str, event_tx: broadcast::Sender<AgentEvent>) -> Self {
        let executable = find_executable(agent_type);
        let has_zorai_mcp = check_zorai_mcp_configured(agent_type);
        Self {
            agent_type: agent_type.to_string(),
            executable,
            event_tx,
            gateway_process: tokio::sync::Mutex::new(None),
            has_zorai_mcp,
        }
    }

    /// Whether zorai-mcp is configured in this agent's MCP settings.
    pub fn has_zorai_mcp(&self) -> bool {
        self.has_zorai_mcp
    }

    /// Whether the external agent executable was found on PATH.
    pub fn is_available(&self) -> bool {
        self.executable.is_some()
    }

    /// Get status information about this external agent.
    pub fn status(&self) -> ExternalAgentStatus {
        ExternalAgentStatus {
            agent_type: self.agent_type.clone(),
            available: self.executable.is_some(),
            executable: self.executable.clone(),
            version: None,
            has_zorai_mcp: self.has_zorai_mcp,
        }
    }

    /// Send a one-shot message to the external agent and collect the response.
    ///
    /// Spawns the agent CLI in one-shot mode, reads stdout line-by-line to
    /// stream deltas, strips TUI noise, and enforces a timeout.
    pub async fn send_message(
        &self,
        thread_id: &str,
        prompt: &str,
        cancel_token: Option<CancellationToken>,
    ) -> Result<String> {
        let request_started_at = Instant::now();
        let exe = self
            .executable
            .as_deref()
            .context(format!("{} executable not found on PATH", self.agent_type))?;

        let args = build_one_shot_args(&self.agent_type, prompt);

        tracing::info!(
            agent = %self.agent_type,
            exe = %exe,
            "external agent: sending message"
        );

        let mut child = tokio::process::Command::new(exe)
            .args(&args)
            .env("TERM", "dumb")
            .env("NO_COLOR", "1")
            .env("CI", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .spawn()
            .context(format!("failed to spawn {} process", self.agent_type))?;

        let stdout = child.stdout.take().context("failed to capture stdout")?;
        let stderr = child.stderr.take().context("failed to capture stderr")?;

        let tid = thread_id.to_string();
        let agent_type = self.agent_type.clone();
        let event_tx = self.event_tx.clone();
        let is_json_mode = agent_type == "openclaw";

        let read_future = async {
            let mut reader = BufReader::new(stdout).lines();
            let mut collected = Vec::<String>::new();
            let mut first_output_at: Option<Instant> = None;

            while let Some(line) = reader.next_line().await? {
                if is_json_mode {
                    if first_output_at.is_none() && !line.trim().is_empty() {
                        first_output_at = Some(Instant::now());
                    }
                    collected.push(line);
                } else {
                    let clean = strip_ansi(&line);
                    let clean = clean.trim();

                    if clean.is_empty() || is_tui_noise(clean) {
                        continue;
                    }

                    if first_output_at.is_none() {
                        first_output_at = Some(Instant::now());
                    }
                    collected.push(clean.to_string());

                    let content = if collected.len() > 1 {
                        format!("\n{clean}")
                    } else {
                        clean.to_string()
                    };
                    let _ = event_tx.send(AgentEvent::Delta {
                        thread_id: tid.clone(),
                        content,
                    });
                }
            }

            let mut stderr_reader = BufReader::new(stderr).lines();
            let mut stderr_lines = Vec::new();
            while let Some(line) = stderr_reader.next_line().await? {
                let clean = strip_ansi(&line);
                let trimmed = clean.trim();
                if !trimmed.is_empty() {
                    stderr_lines.push(trimmed.to_string());
                }
            }

            Ok::<(Vec<String>, Vec<String>, Option<Instant>), anyhow::Error>((
                collected,
                stderr_lines,
                first_output_at,
            ))
        };

        let result = if let Some(token) = cancel_token.as_ref() {
            tokio::select! {
                _ = token.cancelled() => {
                    let _ = child.kill().await;
                    return Err(StreamCancelledError.into());
                }
                timed = tokio::time::timeout(
                    std::time::Duration::from_secs(ONE_SHOT_TIMEOUT_SECS),
                    read_future
                ) => timed,
            }
        } else {
            tokio::time::timeout(
                std::time::Duration::from_secs(ONE_SHOT_TIMEOUT_SECS),
                read_future,
            )
            .await
        };

        let (collected, stderr_lines, first_output_at) = match result {
            Ok(Ok(data)) => data,
            Ok(Err(e)) => {
                let _ = child.kill().await;
                let _ = self.event_tx.send(AgentEvent::Error {
                    thread_id: thread_id.to_string(),
                    message: format!("{}: {e}", self.agent_type),
                });
                return Err(e);
            }
            Err(_elapsed) => {
                let _ = child.kill().await;
                let msg = format!(
                    "{} timed out after {}s",
                    self.agent_type, ONE_SHOT_TIMEOUT_SECS
                );
                let _ = self.event_tx.send(AgentEvent::Error {
                    thread_id: thread_id.to_string(),
                    message: msg.clone(),
                });
                anyhow::bail!(msg);
            }
        };

        let status = if let Some(token) = cancel_token.as_ref() {
            tokio::select! {
                _ = token.cancelled() => {
                    let _ = child.kill().await;
                    return Err(StreamCancelledError.into());
                }
                status = child.wait() => status?,
            }
        } else {
            child.wait().await?
        };

        if !status.success() && collected.is_empty() {
            let error_msg = if stderr_lines.is_empty() {
                format!("{} exited with status {}", self.agent_type, status)
            } else {
                format!("{}: {}", self.agent_type, stderr_lines.join("\n"))
            };
            let _ = self.event_tx.send(AgentEvent::Error {
                thread_id: thread_id.to_string(),
                message: error_msg.clone(),
            });
            anyhow::bail!(error_msg);
        }

        let raw_output = if collected.is_empty() {
            if !stderr_lines.is_empty() {
                stderr_lines.join("\n")
            } else {
                String::new()
            }
        } else {
            collected.join("\n")
        };

        let parsed = parse_structured_response(&self.agent_type, &raw_output);
        let generation_secs = first_output_at
            .unwrap_or(request_started_at)
            .elapsed()
            .as_secs_f64();
        let (generation_ms, tps) =
            super::types::compute_generation_stats(generation_secs, parsed.output_tokens);

        if is_json_mode && !parsed.text.is_empty() {
            let _ = self.event_tx.send(AgentEvent::Delta {
                thread_id: thread_id.to_string(),
                content: parsed.text.clone(),
            });
        }

        let _ = self.event_tx.send(AgentEvent::Done {
            reasoning_tokens: None,
            thinking_ms: None,
            thread_id: thread_id.to_string(),
            input_tokens: parsed.input_tokens,
            output_tokens: parsed.output_tokens,
            cost: None,
            provider: parsed.provider.or_else(|| Some(self.agent_type.clone())),
            model: parsed.model,
            tps,
            generation_ms,
            reasoning: None,
            upstream_message: None,
            provider_final_result: None,
            message_id: None,
        });

        tracing::info!(
            agent = %self.agent_type,
            response_len = parsed.text.len(),
            "external agent: message complete"
        );

        Ok(parsed.text)
    }

    /// Start the external agent in gateway mode (long-running process).
    ///
    /// The agent manages its own platform connections (Slack, Discord, Telegram)
    /// via its own gateway infrastructure.
    pub async fn start_gateway(&self) -> Result<()> {
        let exe = self
            .executable
            .as_deref()
            .context(format!("{} executable not found on PATH", self.agent_type))?;

        self.stop_gateway().await;

        let args = build_gateway_args(&self.agent_type);

        tracing::info!(
            agent = %self.agent_type,
            exe = %exe,
            "external agent: starting gateway mode"
        );

        let child = tokio::process::Command::new(exe)
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .spawn()
            .context(format!(
                "failed to spawn {} gateway process",
                self.agent_type
            ))?;

        tracing::info!(
            agent = %self.agent_type,
            pid = ?child.id(),
            "external agent: gateway process started"
        );

        *self.gateway_process.lock().await = Some(child);
        Ok(())
    }

    /// Stop the external agent's gateway process.
    pub async fn stop_gateway(&self) {
        let mut proc = self.gateway_process.lock().await;
        if let Some(ref mut child) = *proc {
            tracing::info!(
                agent = %self.agent_type,
                "external agent: stopping gateway process"
            );
            let _ = child.kill().await;
        }
        *proc = None;
    }

    /// Stop all processes managed by this runner.
    pub async fn stop(&self) {
        self.stop_gateway().await;
    }
}
