use super::*;
pub(crate) async fn execute_python_execute(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
    thread_id: &str,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    cancel_token: Option<CancellationToken>,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let code = args
        .get("code")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'code' argument"))?;
    let requested_timeout = args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30);
    let timeout_secs = requested_timeout.min(600);
    let auto_background = requested_timeout > 600;
    let wait_for_completion = if auto_background {
        false
    } else {
        tool_waits_for_completion(args)
    };
    let cwd = resolve_tool_cwd(args, session_manager, session_id).await?;
    let client_surface = resolve_shell_tool_client_surface(agent, thread_id, task_id).await;
    let foreground_detach_after = headless_foreground_detach_after_for_surface(client_surface);

    let python_bin = if tokio::process::Command::new("python3")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
    {
        "python3"
    } else {
        "python"
    }
    .to_string();

    if !wait_for_completion {
        return spawn_python_execute_background(
            &python_bin,
            code,
            cwd,
            requested_timeout,
            auto_background,
            agent,
            thread_id,
        )
        .await;
    }

    let mut process = tokio::process::Command::new(&python_bin);
    process
        .arg("-c")
        .arg(code)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = cwd.as_deref() {
        process.current_dir(cwd);
    }

    let mut child = process
        .spawn()
        .with_context(|| format!("failed to spawn {python_bin} subprocess"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("python_execute stdout capture was unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("python_execute stderr capture was unavailable"))?;
    let stdout_capture = spawn_headless_output_capture(stdout);
    let stderr_capture = spawn_headless_output_capture(stderr);

    let full_timeout = std::time::Duration::from_secs(timeout_secs);
    let foreground_wait = foreground_detach_after
        .map(|duration| duration.min(full_timeout))
        .unwrap_or(full_timeout);
    let wait_result = tokio::time::timeout(foreground_wait, child.wait());

    let status = if let Some(token) = cancel_token.as_ref() {
        tokio::select! {
            result = wait_result => result,
            _ = token.cancelled() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                let _ = collect_headless_output_capture(
                    stdout_capture,
                    usize::MAX,
                    std::time::Duration::from_secs(5),
                ).await;
                let _ = collect_headless_output_capture(
                    stderr_capture,
                    usize::MAX,
                    std::time::Duration::from_secs(5),
                ).await;
                anyhow::bail!("python_execute cancelled while waiting for command completion");
            }
        }
    } else {
        wait_result.await
    };

    let status = match status {
        Ok(result) => result.context("python_execute process wait failed")?,
        Err(_) if foreground_detach_after.is_some() && foreground_wait < full_timeout => {
            let operation = crate::server::operation_registry()
                .accept_operation(tool_names::PYTHON_EXECUTE, None);
            let operation_id = operation.operation_id.clone();
            crate::server::operation_registry().mark_started(&operation_id);
            spawn_python_execute_monitor(
                operation_id.clone(),
                python_bin.clone(),
                cwd.clone(),
                child,
                stdout_capture,
                stderr_capture,
            );
            let operation_handle = model_operation_id(agent, thread_id, &operation_id).await;
            let cwd_suffix = cwd
                .as_ref()
                .map(|path| format!(" in {}", path.display()))
                .unwrap_or_default();
            return Ok((
                format!(
                    "Python execution detached{cwd_suffix} as background operation {operation_handle} after {}s foreground grace.\nbackground_task_id: {operation_handle}\noperation_id: {operation_handle}\nwait_for_completion=true exceeded the TUI foreground grace window. {BACKGROUND_OPERATION_COMPLETION_GUIDANCE}",
                    foreground_wait.as_secs()
                ),
                None,
            ));
        }
        Err(_) => {
            anyhow::bail!("python_execute timed out after {timeout_secs}s");
        }
    };

    let stdout = collect_headless_output_capture(
        stdout_capture,
        usize::MAX,
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap_or_default();
    let stderr = collect_headless_output_capture(
        stderr_capture,
        usize::MAX,
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap_or_default();
    let cwd_suffix = cwd
        .as_ref()
        .map(|path| format!(" in {}", path.display()))
        .unwrap_or_default();

    if status.success() {
        let mut result = format!(
            "Python finished successfully{cwd_suffix} (exit_code: 0, interpreter: {python_bin})."
        );
        if !stdout.is_empty() {
            result.push_str(&format!("\n\nStdout:\n{stdout}"));
        }
        if !stderr.is_empty() {
            result.push_str(&format!("\n\nStderr:\n{stderr}"));
        }
        Ok((result, None))
    } else {
        let mut details = String::new();
        if !stdout.is_empty() {
            details.push_str(&format!("\n\nStdout:\n{stdout}"));
        }
        if !stderr.is_empty() {
            details.push_str(&format!("\n\nStderr:\n{stderr}"));
        }
        Err(anyhow::anyhow!(
            "Python failed{cwd_suffix} (exit_code: {:?}, interpreter: {}).{}",
            status.code(),
            python_bin,
            details
        ))
    }
}

async fn spawn_python_execute_background(
    python_bin: &str,
    code: &str,
    cwd: Option<PathBuf>,
    requested_timeout: u64,
    auto_background: bool,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let operation =
        crate::server::operation_registry().accept_operation(tool_names::PYTHON_EXECUTE, None);
    let operation_id = operation.operation_id.clone();

    let mut process = tokio::process::Command::new(python_bin);
    process
        .arg("-c")
        .arg(code)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = cwd.as_deref() {
        process.current_dir(cwd);
    }

    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            crate::server::operation_registry().mark_failed(&operation_id);
            return Err(error).with_context(|| format!("failed to spawn {python_bin} subprocess"));
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("python_execute stdout capture was unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("python_execute stderr capture was unavailable"))?;
    let stdout_capture = spawn_headless_output_capture(stdout);
    let stderr_capture = spawn_headless_output_capture(stderr);

    crate::server::operation_registry().mark_started(&operation_id);
    spawn_python_execute_monitor(
        operation_id.clone(),
        python_bin.to_string(),
        cwd.clone(),
        child,
        stdout_capture,
        stderr_capture,
    );

    let cwd_suffix = cwd
        .as_ref()
        .map(|path| format!(" in {}", path.display()))
        .unwrap_or_default();
    let operation_handle = model_operation_id(agent, thread_id, &operation_id).await;
    let queued_summary = format!(
        "Python execution queued{cwd_suffix} as background operation {operation_handle} (interpreter: {python_bin})."
    );

    if auto_background {
        Ok((
            format!(
                "{queued_summary}\nbackground_task_id: {operation_handle}\noperation_id: {operation_handle}\nPython execution auto-backgrounded (requested timeout {}s > max 600s). {BACKGROUND_OPERATION_COMPLETION_GUIDANCE}",
                requested_timeout,
            ),
            None,
        ))
    } else {
        Ok((
            format!(
                "{queued_summary}\nbackground_task_id: {operation_handle}\noperation_id: {operation_handle}\nNot waiting for completion (wait_for_completion=false; non-quick or long-running commands default to background). {BACKGROUND_OPERATION_COMPLETION_GUIDANCE}"
            ),
            None,
        ))
    }
}

fn spawn_python_execute_monitor(
    operation_id: String,
    interpreter: String,
    cwd: Option<PathBuf>,
    mut child: tokio::process::Child,
    stdout_capture: HeadlessOutputCapture,
    stderr_capture: HeadlessOutputCapture,
) {
    let cwd_for_task = cwd.as_ref().map(|path| path.display().to_string());
    let started_at = std::time::Instant::now();
    tokio::spawn(async move {
        let outcome = child.wait().await;
        let stdout = collect_headless_output_capture(
            stdout_capture,
            4000,
            std::time::Duration::from_millis(100),
        )
        .await;
        let stderr = collect_headless_output_capture(
            stderr_capture,
            4000,
            std::time::Duration::from_millis(100),
        )
        .await;
        let duration_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

        match outcome {
            Ok(status) if status.success() => {
                crate::server::operation_registry().mark_completed_with_terminal_result(
                    &operation_id,
                    serde_json::json!({
                        "interpreter": interpreter,
                        "cwd": cwd_for_task,
                        "exit_code": status.code(),
                        "duration_ms": duration_ms,
                        "stdout": stdout,
                        "stderr": stderr,
                    }),
                );
            }
            Ok(status) => {
                crate::server::operation_registry().mark_failed_with_terminal_result(
                    &operation_id,
                    serde_json::json!({
                        "interpreter": interpreter,
                        "cwd": cwd_for_task,
                        "exit_code": status.code(),
                        "duration_ms": duration_ms,
                        "stdout": stdout,
                        "stderr": stderr,
                    }),
                );
            }
            Err(error) => {
                crate::server::operation_registry().mark_failed_with_terminal_result(
                    &operation_id,
                    serde_json::json!({
                        "interpreter": interpreter,
                        "cwd": cwd_for_task,
                        "duration_ms": duration_ms,
                        "spawn_error": error.to_string(),
                        "stdout": stdout,
                        "stderr": stderr,
                    }),
                );
            }
        }
    });
}
