use super::*;
fn daemon_message_kind(msg: &DaemonMessage) -> &'static str {
    match msg {
        DaemonMessage::ManagedCommandQueued { .. } => "managed_command_queued",
        DaemonMessage::ApprovalRequired { .. } => "approval_required",
        DaemonMessage::ManagedCommandRejected { .. } => "managed_command_rejected",
        DaemonMessage::ManagedCommandStarted { .. } => "managed_command_started",
        DaemonMessage::ManagedCommandFinished { .. } => "managed_command_finished",
        _ => "other",
    }
}

#[derive(Debug)]
pub(crate) enum ManagedCommandWaitOutcome {
    Finished {
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
        output_tail: String,
    },
    Rejected {
        message: String,
    },
    Timeout {
        output_tail: String,
    },
}

fn terminal_output_tail(raw: &[u8], max_lines: usize) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let stripped = strip_ansi_escapes::strip(raw);
    let text = String::from_utf8_lossy(&stripped);
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let start = lines.len().saturating_sub(max_lines);
    let mut result = String::new();
    if start > 0 {
        result.push_str(&format!("... ({} earlier lines omitted)\n", start));
    }
    result.push_str(&lines[start..].join("\n"));
    result
}

pub(crate) async fn wait_for_managed_command_outcome(
    rx: &mut tokio::sync::broadcast::Receiver<DaemonMessage>,
    session_id: SessionId,
    execution_id: &str,
    timeout_secs: u64,
    cancel_token: Option<&CancellationToken>,
) -> Result<ManagedCommandWaitOutcome> {
    const MAX_CAPTURE_BYTES: usize = 512_000;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let mut output_buf = Vec::new();

    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Ok(ManagedCommandWaitOutcome::Timeout {
                output_tail: terminal_output_tail(&output_buf, 80),
            });
        }

        let event = if let Some(token) = cancel_token {
            tokio::select! {
                result = tokio::time::timeout(remaining, rx.recv()) => {
                    result.map_err(|_| anyhow::anyhow!("timed out waiting for managed command result"))?
                }
                _ = token.cancelled() => {
                    anyhow::bail!(
                        "managed terminal command wait cancelled; the command may still be running in the session"
                    );
                }
            }
        } else {
            tokio::time::timeout(remaining, rx.recv())
                .await
                .map_err(|_| anyhow::anyhow!("timed out waiting for managed command result"))?
        };

        let msg = match event {
            Ok(message) => message,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err(anyhow::anyhow!(
                    "terminal session event stream closed while waiting for managed command result"
                ));
            }
        };

        match msg {
            DaemonMessage::Output { id, data } if id == session_id => {
                output_buf.extend_from_slice(&data);
                if output_buf.len() > MAX_CAPTURE_BYTES {
                    let overflow = output_buf.len() - MAX_CAPTURE_BYTES;
                    output_buf.drain(..overflow);
                }
            }
            DaemonMessage::ManagedCommandFinished {
                id,
                execution_id: finished_id,
                exit_code,
                duration_ms,
                ..
            } if id == session_id && finished_id == execution_id => {
                return Ok(ManagedCommandWaitOutcome::Finished {
                    exit_code,
                    duration_ms,
                    output_tail: terminal_output_tail(&output_buf, 80),
                });
            }
            DaemonMessage::ManagedCommandRejected {
                id,
                execution_id: rejected_id,
                message,
            } if id == session_id
                && (rejected_id.as_deref() == Some(execution_id) || rejected_id.is_none()) =>
            {
                return Ok(ManagedCommandWaitOutcome::Rejected { message });
            }
            DaemonMessage::SessionExited { id, exit_code } if id == session_id => {
                let output_tail = terminal_output_tail(&output_buf, 80);
                let output_section = if output_tail.trim().is_empty() {
                    String::new()
                } else {
                    format!("\n\nTerminal output (tail):\n{output_tail}")
                };
                return Err(anyhow::anyhow!(
                    "terminal session exited while waiting for managed command result (exit_code: {:?}){}",
                    exit_code,
                    output_section
                ));
            }
            _ => {}
        }
    }
}

async fn execute_terminal_python_capture(
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    requested_session: Option<&str>,
    script: &str,
    token: &str,
    rationale: &str,
    timeout_secs: u64,
) -> Result<String> {
    const MAX_CAPTURE_BYTES: usize = 512_000;
    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        anyhow::bail!("No active terminal sessions are available");
    }

    let resolved_session_id = if let Some(session_ref) = requested_session {
        sessions
            .iter()
            .find(|session| {
                session.id.to_string() == session_ref
                    || session.id.to_string().contains(session_ref)
            })
            .map(|session| session.id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_ref}"))?
    } else {
        preferred_session_id.unwrap_or(sessions[0].id)
    };

    let (mut rx, _) = session_manager.subscribe(resolved_session_id).await?;
    let script_b64 = base64::engine::general_purpose::STANDARD.encode(script.as_bytes());
    let command = format!(
        "if command -v python3 >/dev/null 2>&1; then \
             python3 -c \"import base64;exec(base64.b64decode('{script_b64}').decode('utf-8'))\"; \
         else \
             python -c \"import base64;exec(base64.b64decode('{script_b64}').decode('utf-8'))\"; \
         fi"
    );
    let request = ManagedCommandRequest {
        command,
        rationale: rationale.to_string(),
        allow_network: false,
        sandbox_enabled: false,
        security_level: SecurityLevel::Lowest,
        cwd: None,
        language_hint: Some("python".to_string()),
        source: ManagedCommandSource::Agent,
    };

    let queued = session_manager
        .execute_managed_command(resolved_session_id, request)
        .await?;
    let execution_id = match queued {
        DaemonMessage::ManagedCommandQueued { execution_id, .. } => execution_id,
        DaemonMessage::ApprovalRequired { approval, .. } => {
            return Err(anyhow::anyhow!(
                "terminal capture command requires approval before execution (approval_id: {})",
                approval.approval_id
            ));
        }
        DaemonMessage::ManagedCommandRejected { message, .. } => {
            return Err(anyhow::anyhow!(
                "terminal capture command rejected: {message}"
            ));
        }
        other => {
            return Err(anyhow::anyhow!(
                "unexpected managed command response: {}",
                daemon_message_kind(&other)
            ));
        }
    };

    let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let mut output_buf: Vec<u8> = Vec::new();
    loop {
        let remaining = wait_deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow::anyhow!(
                "timed out waiting for terminal capture command completion (execution_id: {execution_id})"
            ));
        }

        let event = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "timed out waiting for terminal capture command completion (execution_id: {execution_id})"
                )
            })?;

        let msg = match event {
            Ok(message) => message,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err(anyhow::anyhow!(
                    "terminal session event stream closed while waiting for command output"
                ));
            }
        };

        match msg {
            DaemonMessage::Output { id, data } if id == resolved_session_id => {
                output_buf.extend_from_slice(&data);
                if output_buf.len() > MAX_CAPTURE_BYTES {
                    let overflow = output_buf.len() - MAX_CAPTURE_BYTES;
                    output_buf.drain(..overflow);
                }
            }
            DaemonMessage::ManagedCommandFinished {
                id,
                execution_id: finished_id,
                exit_code,
                ..
            } if id == resolved_session_id && finished_id == execution_id => {
                let (captured_status, captured_output) = parse_capture_output(&output_buf, token)
                    .ok_or_else(|| {
                    anyhow::anyhow!(
                        "failed to parse captured command output (execution_id: {execution_id})"
                    )
                })?;

                if captured_status == 0 && exit_code == Some(0) {
                    return Ok(captured_output);
                }

                return Err(anyhow::anyhow!(
                    "terminal capture command failed (execution_id: {execution_id}, exit_code: {:?}): {}",
                    exit_code,
                    captured_output
                ));
            }
            DaemonMessage::ManagedCommandRejected {
                id,
                execution_id: rejected_id,
                message,
            } if id == resolved_session_id
                && (rejected_id.as_deref() == Some(execution_id.as_str())
                    || rejected_id.is_none()) =>
            {
                return Err(anyhow::anyhow!(
                    "terminal capture command rejected (execution_id: {execution_id}): {message}"
                ));
            }
            _ => {}
        }
    }
}

pub(crate) fn parse_capture_output(output: &[u8], token: &str) -> Option<(i32, String)> {
    let stripped = strip_ansi_escapes::strip(output);
    let text = String::from_utf8_lossy(&stripped);

    let begin_marker = format!("__ZORAI_CAPTURE_BEGIN_{token}__");
    let end_prefix = format!("__ZORAI_CAPTURE_END_{token}__:");

    let begin_idx = text.rfind(&begin_marker)?;
    let after_begin = &text[begin_idx + begin_marker.len()..];
    let after_begin = after_begin.trim_start_matches(['\r', '\n']);

    let end_idx = after_begin.find(&end_prefix)?;
    let encoded_payload = after_begin[..end_idx]
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    if encoded_payload.is_empty() {
        return Some((0, String::new()));
    }

    let after_end = &after_begin[end_idx + end_prefix.len()..];
    let status_raw = after_end
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect::<String>();
    let status = status_raw.parse::<i32>().ok()?;

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded_payload)
        .ok()?;
    let payload = String::from_utf8_lossy(&decoded).into_owned();
    Some((status, payload))
}
