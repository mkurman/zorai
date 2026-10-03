use super::*;

pub(crate) const BACKGROUND_OPERATION_COMPLETION_GUIDANCE: &str =
    "This thread auto-resumes when the command completes. Do not poll get_operation_status. If you need the result before doing more work, call get_operation_status once with wait=true.";

const OPERATION_STATUS_POLL_HINT: &str =
    "Do not poll this tool. Set wait=true to block until completion in one call, or continue other work; this thread auto-resumes when the operation finishes.";

const WAIT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 600;
const MAX_WAIT_TIMEOUT_SECS: u64 = 3600;

pub(crate) async fn execute_get_background_task_status(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    agent: Option<&AgentEngine>,
    cancel_token: Option<CancellationToken>,
    thread_id: Option<&str>,
) -> Result<String> {
    let background_task_id = args
        .get("background_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'background_task_id' argument"))?;

    execute_operation_status_lookup(
        background_task_id,
        session_manager,
        true,
        wait_args(args),
        agent,
        thread_id,
        cancel_token.as_ref(),
    )
    .await
}

pub(crate) async fn execute_get_operation_status(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    agent: Option<&AgentEngine>,
    cancel_token: Option<CancellationToken>,
    thread_id: Option<&str>,
) -> Result<String> {
    let operation_id = args
        .get("operation_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'operation_id' argument"))?;

    execute_operation_status_lookup(
        operation_id,
        session_manager,
        false,
        wait_args(args),
        agent,
        thread_id,
        cancel_token.as_ref(),
    )
    .await
}

struct WaitArgs {
    wait: bool,
    timeout_seconds: u64,
}

fn wait_args(args: &serde_json::Value) -> WaitArgs {
    WaitArgs {
        wait: args
            .get("wait")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        timeout_seconds: args
            .get("timeout_seconds")
            .and_then(|value| value.as_u64())
            .unwrap_or(DEFAULT_WAIT_TIMEOUT_SECS)
            .min(MAX_WAIT_TIMEOUT_SECS),
    }
}

fn operation_state_is_terminal(payload: &serde_json::Value) -> bool {
    matches!(
        payload.get("state").and_then(|value| value.as_str()),
        Some("completed" | "failed" | "cancelled")
    )
}

async fn execute_operation_status_lookup(
    operation_id: &str,
    session_manager: &Arc<SessionManager>,
    compatibility_alias: bool,
    wait: WaitArgs,
    agent: Option<&AgentEngine>,
    thread_id: Option<&str>,
    cancel_token: Option<&CancellationToken>,
) -> Result<String> {
    let (resolved_id, mut payload) = lookup_operation_status_payload(
        operation_id,
        session_manager,
        compatibility_alias,
        agent,
        thread_id,
    )
    .await?;

    if !wait.wait || operation_state_is_terminal(&payload) {
        if wait.wait && operation_state_is_terminal(&payload) {
            payload["waited"] = serde_json::Value::Bool(true);
            if let Some(agent) = agent {
                agent.claim_operation_wakeup(&resolved_id).await;
            }
        } else if !wait.wait && !operation_state_is_terminal(&payload) {
            payload["next_step"] =
                serde_json::Value::String(OPERATION_STATUS_POLL_HINT.to_string());
        }
        return Ok(payload.to_string());
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait.timeout_seconds);
    loop {
        if let Some(token) = cancel_token {
            if token.is_cancelled() {
                payload["wait_cancelled"] = serde_json::Value::Bool(true);
                payload["next_step"] =
                    serde_json::Value::String(OPERATION_STATUS_POLL_HINT.to_string());
                return Ok(payload.to_string());
            }
        }

        let now = std::time::Instant::now();
        if now >= deadline {
            payload["wait_timed_out"] = serde_json::Value::Bool(true);
            payload["waited_seconds"] = serde_json::Value::Number(wait.timeout_seconds.into());
            payload["next_step"] = serde_json::Value::String(
                "Wait timed out while the operation was still running. Call get_operation_status with wait=true again, or continue other work; this thread auto-resumes on completion."
                    .to_string(),
            );
            return Ok(payload.to_string());
        }

        let sleep_for = deadline
            .saturating_duration_since(now)
            .min(WAIT_POLL_INTERVAL);
        if let Some(token) = cancel_token {
            tokio::select! {
                _ = tokio::time::sleep(sleep_for) => {}
                _ = token.cancelled() => {
                    payload["wait_cancelled"] = serde_json::Value::Bool(true);
                    payload["next_step"] =
                        serde_json::Value::String(OPERATION_STATUS_POLL_HINT.to_string());
                    return Ok(payload.to_string());
                }
            }
        } else {
            tokio::time::sleep(sleep_for).await;
        }

        let Some(updated) =
            operation_status_payload(&resolved_id, session_manager, compatibility_alias).await?
        else {
            anyhow::bail!("unknown operation id: {resolved_id}");
        };
        payload = updated;
        if operation_state_is_terminal(&payload) {
            payload["waited"] = serde_json::Value::Bool(true);
            if let Some(agent) = agent {
                agent.claim_operation_wakeup(&resolved_id).await;
            }
            return Ok(payload.to_string());
        }
    }
}

async fn lookup_operation_status_payload(
    operation_id: &str,
    session_manager: &Arc<SessionManager>,
    compatibility_alias: bool,
    agent: Option<&AgentEngine>,
    thread_id: Option<&str>,
) -> Result<(String, serde_json::Value)> {
    if let Some(payload) =
        operation_status_payload(operation_id, session_manager, compatibility_alias).await?
    {
        return Ok((operation_id.to_string(), payload));
    }

    if let (Some(agent), Some(thread_id)) = (agent, thread_id) {
        if let Some(resolved_id) = lookup_operation_id(agent, thread_id, operation_id).await? {
            if let Some(mut payload) =
                operation_status_payload(&resolved_id, session_manager, compatibility_alias).await?
            {
                payload["operation_id"] = serde_json::Value::String(operation_id.to_string());
                if payload.get("background_task_id").is_some() {
                    payload["background_task_id"] =
                        serde_json::Value::String(operation_id.to_string());
                }
                return Ok((resolved_id, payload));
            }
        }
    }

    if let Some(resolved_id) =
        crate::server::operation_registry().resolve_unique_id_by_first_segment(operation_id)
    {
        if let Some(mut payload) =
            operation_status_payload(&resolved_id, session_manager, compatibility_alias).await?
        {
            payload["requested_operation_id"] = serde_json::Value::String(operation_id.to_string());
            payload["status_note"] = serde_json::Value::String(format!(
                "Requested operation id {operation_id} was not found; it was resolved to {resolved_id} because that is the only registered operation sharing the same leading UUID segment. Use the exact operation_id {resolved_id} in future calls."
            ));
            return Ok((resolved_id, payload));
        }
    }

    Err(anyhow::anyhow!("unknown operation id: {operation_id}"))
}

async fn operation_status_payload(
    operation_id: &str,
    session_manager: &Arc<SessionManager>,
    compatibility_alias: bool,
) -> Result<Option<serde_json::Value>> {
    if let Some(status) = session_manager
        .get_background_task_status(operation_id)
        .await?
    {
        let mut payload = serde_json::json!({
            "operation_id": status.background_task_id,
            "kind": status.kind,
            "state": status.state,
            "background_task_id": operation_id,
        });

        if let Some(session_id) = status.session_id {
            payload["session_id"] = serde_json::Value::String(session_id);
        }
        if let Some(position) = status.position {
            payload["position"] = serde_json::Value::Number(position.into());
        }
        if let Some(command) = status.command {
            payload["command"] = serde_json::Value::String(command);
        }
        if let Some(exit_code) = status.exit_code {
            payload["exit_code"] = serde_json::Value::Number(exit_code.into());
        }
        if let Some(duration_ms) = status.duration_ms {
            payload["duration_ms"] = serde_json::Value::Number(duration_ms.into());
        }
        if let Some(snapshot_path) = status.snapshot_path {
            payload["snapshot_path"] = serde_json::Value::String(snapshot_path);
        }
        if !compatibility_alias {
            payload
                .as_object_mut()
                .map(|obj| obj.remove("background_task_id"));
        }

        return Ok(Some(payload));
    }

    if let Some(snapshot) = crate::server::operation_registry().snapshot(operation_id) {
        let mut payload = serde_json::json!({
            "operation_id": snapshot.operation_id,
            "kind": snapshot.kind,
            "state": snapshot.state,
            "revision": snapshot.revision,
        });
        if let Some(dedup) = snapshot.dedup {
            payload["dedup"] = serde_json::Value::String(dedup);
        }
        if let Some(terminal_result) =
            crate::server::operation_registry().terminal_result(operation_id)
        {
            if let Some(exit_code) = terminal_result
                .get("exit_code")
                .and_then(|value| value.as_i64())
            {
                payload["exit_code"] = serde_json::Value::Number(exit_code.into());
            }
            payload["terminal_result"] = terminal_result;
        } else if matches!(
            payload["kind"].as_str(),
            Some(tool_names::BASH_COMMAND | tool_names::RUN_TERMINAL_COMMAND)
        ) && matches!(payload["state"].as_str(), Some("accepted" | "started"))
        {
            payload["status_hint"] = serde_json::Value::String(
                "Final terminal payload will appear under `terminal_result` once this background headless command reaches completed or failed. Do not rerun it in foreground just to inspect output.".to_string(),
            );
        }
        if compatibility_alias {
            payload["background_task_id"] = serde_json::Value::String(operation_id.to_string());
        }
        return Ok(Some(payload));
    }

    Ok(None)
}
