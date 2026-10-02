use super::*;
pub(crate) async fn execute_read_terminal(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
) -> Result<String> {
    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        return Ok("No active terminal sessions.".into());
    }

    let target_id = if let Some(pane) = args.get("pane").and_then(|v| v.as_str()) {
        sessions
            .iter()
            .find(|s| s.id.to_string().contains(pane))
            .map(|s| s.id)
    } else {
        None
    };

    let sid = target_id.unwrap_or(sessions[0].id);

    match session_manager.get_scrollback(sid, None).await {
        Ok(data) => {
            if data.is_empty() {
                return Ok("(terminal buffer is empty)".into());
            }

            let stripped = strip_ansi_escapes::strip(&data);
            let text = String::from_utf8_lossy(&stripped);

            let lines: Vec<&str> = text.lines().collect();
            let start = if lines.len() > 200 {
                lines.len() - 200
            } else {
                0
            };
            let visible: Vec<&str> = lines[start..]
                .iter()
                .filter(|l| !l.trim().is_empty())
                .copied()
                .collect();

            if visible.is_empty() {
                Ok("(terminal buffer contains only whitespace/control characters)".into())
            } else {
                let mut result = visible.join("\n");
                if start > 0 {
                    result = format!("... ({} earlier lines omitted)\n\n{result}", start);
                }
                Ok(result)
            }
        }
        Err(e) => Ok(format!("Failed to read terminal: {e}")),
    }
}

pub(crate) async fn execute_run_terminal_command(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    cancel_token: Option<CancellationToken>,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let client_surface = resolve_shell_tool_client_surface(agent, thread_id, task_id).await;
    if should_use_managed_execution_for_surface(client_surface, args) {
        if session_manager.list().await.is_empty()
            && can_run_headless_when_terminal_unavailable(args)
        {
            return execute_headless_shell_command(
                args,
                session_manager,
                session_id,
                tool_names::RUN_TERMINAL_COMMAND,
                cancel_token,
                None,
            )
            .await;
        }
        let managed_args =
            managed_alias_args(args, "Run a shell command in a managed terminal session");
        execute_managed_command(
            &managed_args,
            agent,
            session_manager,
            session_id,
            event_tx,
            thread_id,
            cancel_token,
        )
        .await
    } else {
        execute_headless_shell_command(
            args,
            session_manager,
            session_id,
            tool_names::RUN_TERMINAL_COMMAND,
            cancel_token,
            None,
        )
        .await
    }
}

pub(crate) async fn execute_bash_command(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    cancel_token: Option<CancellationToken>,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let forced_background_args;
    let args = if bash_command_should_force_background(args) {
        forced_background_args = bash_command_args_with_wait_false(args);
        &forced_background_args
    } else {
        args
    };
    let client_surface = resolve_shell_tool_client_surface(agent, thread_id, task_id).await;
    let foreground_detach_after = headless_foreground_detach_after_for_surface(client_surface);
    if should_use_managed_execution_for_surface(client_surface, args) {
        if session_manager.list().await.is_empty()
            && can_run_headless_when_terminal_unavailable(args)
        {
            return execute_headless_shell_command(
                args,
                session_manager,
                session_id,
                tool_names::BASH_COMMAND,
                cancel_token,
                foreground_detach_after,
            )
            .await;
        }
        let managed_args =
            managed_alias_args(args, "Run a shell command in a managed terminal session");
        execute_managed_command(
            &managed_args,
            agent,
            session_manager,
            session_id,
            event_tx,
            thread_id,
            cancel_token,
        )
        .await
    } else {
        execute_headless_shell_command(
            args,
            session_manager,
            session_id,
            tool_names::BASH_COMMAND,
            cancel_token,
            foreground_detach_after,
        )
        .await
    }
}

const TUI_HEADLESS_FOREGROUND_GRACE_SECS: u64 = 1;

pub(crate) fn tool_waits_for_completion(args: &serde_json::Value) -> bool {
    args.get("wait_for_completion")
        .and_then(|value| value.as_bool())
        .or_else(|| {
            args.get("wait_for_response")
                .and_then(|value| value.as_bool())
        })
        .unwrap_or(true)
}

pub(crate) fn bash_command_should_force_background(args: &serde_json::Value) -> bool {
    if args
        .get("__weles_force_headless")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return false;
    }

    // An explicitly requested wait flag is authoritative: only the >600s
    // auto-background cap may override it. If the caller explicitly asked to
    // wait, never silently rewrite that into a background dispatch.
    let wait_explicitly_requested = args
        .get("wait_for_completion")
        .and_then(|value| value.as_bool())
        .or_else(|| {
            args.get("wait_for_response")
                .and_then(|value| value.as_bool())
        });
    if wait_explicitly_requested == Some(true) {
        let requested_timeout = args
            .get("timeout_seconds")
            .and_then(|value| value.as_u64())
            .unwrap_or(30);
        return requested_timeout > 600;
    }

    let wait_for_completion = tool_waits_for_completion(args);
    if !wait_for_completion {
        return false;
    }

    let requested_timeout = args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30);
    if requested_timeout > 600 {
        return false;
    }

    args.get("command")
        .and_then(|value| value.as_str())
        .is_none_or(shell_command_should_start_in_background)
}

fn bash_command_args_with_wait_false(args: &serde_json::Value) -> serde_json::Value {
    let mut mapped = args.as_object().cloned().unwrap_or_default();
    mapped.insert(
        "wait_for_completion".to_string(),
        serde_json::Value::Bool(false),
    );
    serde_json::Value::Object(mapped)
}

pub(crate) fn bash_command_can_wait_for_completion(args: &serde_json::Value) -> bool {
    if !tool_waits_for_completion(args) {
        return false;
    }

    let Some(command) = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };

    command_is_known_quick_shell_command(command)
}

fn command_is_known_quick_shell_command(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty()
        || trimmed.contains('\n')
        || trimmed.contains('|')
        || trimmed.contains('`')
        || trimmed.contains("$(")
    {
        return false;
    }

    split_shell_segments_respecting_quotes(trimmed)
        .into_iter()
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .all(|segment| shell_command_segment_is_known_quick(segment.as_str()))
}

fn shell_command_should_start_in_background(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return true;
    }

    if trimmed.contains('\n')
        || trimmed.contains('|')
        || trimmed.contains('`')
        || trimmed.contains("$(")
    {
        return true;
    }

    split_shell_segments_respecting_quotes(trimmed)
        .into_iter()
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .any(|segment| shell_command_segment_should_start_in_background(segment.as_str()))
}

/// Split a shell command into top-level segments at `;` and `&&`, respecting
/// single- and double-quote groups. Naive `.split(';')` mis-fires on inline
/// scripts such as `python3 -c "...; ...; ..."`, where the `;` lives inside
/// the quoted argument; splitting there fragments the inline code into
/// pieces where downstream segment classifiers can no longer recognize
/// long-running constructs (e.g. `time.sleep`) and the command is
/// mistakenly treated as "quick", running synchronously instead of
/// auto-backgrounding.
fn split_shell_segments_respecting_quotes(command: &str) -> Vec<String> {
    let bytes = command.as_bytes();
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut single_quote = false;
    let mut double_quote = false;
    let mut idx = 0;
    while idx < bytes.len() {
        let ch = bytes[idx] as char;
        match ch {
            '\\' if !single_quote && idx + 1 < bytes.len() => {
                current.push(ch);
                current.push(bytes[idx + 1] as char);
                idx += 2;
                continue;
            }
            '\'' if !double_quote => {
                single_quote = !single_quote;
                current.push(ch);
            }
            '"' if !single_quote => {
                double_quote = !double_quote;
                current.push(ch);
            }
            ';' if !single_quote && !double_quote => {
                segments.push(std::mem::take(&mut current));
            }
            '&' if !single_quote
                && !double_quote
                && idx + 1 < bytes.len()
                && bytes[idx + 1] as char == '&' =>
            {
                segments.push(std::mem::take(&mut current));
                idx += 2;
                continue;
            }
            _ => current.push(ch),
        }
        idx += 1;
    }
    segments.push(current);
    segments
}

fn shell_command_segment_should_start_in_background(segment: &str) -> bool {
    let first = first_shell_command_word(segment);
    match first.as_str() {
        "sleep" | "watch" | "yes" => true,
        "tail" => shell_words(segment)
            .iter()
            .skip(1)
            .any(|arg| arg == "-f" || arg == "--follow" || arg.starts_with("--follow=")),
        "bash" | "sh" | "zsh" | "fish" | "node" | "ruby" | "perl" => !shell_words(segment)
            .iter()
            .skip(1)
            .any(|arg| matches!(arg.as_str(), "--version" | "-V" | "-v" | "--help" | "-h")),
        "python" | "python3" => {
            let words = shell_words(segment);
            if words
                .iter()
                .skip(1)
                .any(|arg| matches!(arg.as_str(), "--version" | "-V" | "-v" | "--help" | "-h"))
            {
                false
            } else if let Some(code) = python_inline_code_arg(&words) {
                python_inline_code_should_start_in_background(code)
            } else {
                true
            }
        }
        "npm" | "pnpm" | "yarn" => shell_words(segment).iter().skip(1).any(|arg| {
            matches!(
                arg.as_str(),
                "run" | "dev" | "start" | "test" | "build" | "install" | "ci" | "watch"
            )
        }),
        "cargo" => shell_words(segment).iter().skip(1).any(|arg| {
            matches!(
                arg.as_str(),
                "build" | "check" | "clippy" | "doc" | "run" | "test" | "watch"
            )
        }),
        "make" | "cmake" | "ninja" | "pytest" | "docker" | "docker-compose" => true,
        _ => false,
    }
}

fn shell_words(segment: &str) -> Vec<String> {
    shlex::split(segment).unwrap_or_else(|| {
        segment
            .split_whitespace()
            .map(|word| {
                word.trim_matches(|ch: char| ch == '(' || ch == ')' || ch == '"' || ch == '\'')
                    .to_string()
            })
            .filter(|word| !word.is_empty())
            .collect()
    })
}

fn first_shell_command_word(segment: &str) -> String {
    shell_words(segment)
        .first()
        .map(|word| {
            word.rsplit('/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
        })
        .unwrap_or_default()
}

fn shell_command_segment_is_known_quick(segment: &str) -> bool {
    let first = first_shell_command_word(segment);

    matches!(
        first.as_str(),
        "pwd"
            | "printf"
            | "echo"
            | "true"
            | "false"
            | "test"
            | "["
            | "ls"
            | "cat"
            | "head"
            | "tail"
            | "wc"
            | "stat"
            | "date"
            | "whoami"
            | "id"
            | "uname"
            | "basename"
            | "dirname"
            | "which"
            | "command"
            | "type"
            | "realpath"
            | "readlink"
            | "ps"
            | "pgrep"
            | "kill"
            | "pkill"
            | "jobs"
            | "lsof"
            | "df"
            | "free"
            | "env"
            | "printenv"
            | "uptime"
            | "hostname"
    )
}

fn python_inline_code_arg(words: &[String]) -> Option<&str> {
    for (index, word) in words.iter().enumerate().skip(1) {
        if word == "-c" {
            return words.get(index + 1).map(String::as_str);
        }
        if let Some(code) = word.strip_prefix("-c") {
            if !code.is_empty() {
                return Some(code);
            }
        }
    }
    None
}

fn python_inline_code_should_start_in_background(code: &str) -> bool {
    let normalized = code.to_ascii_lowercase();
    normalized.contains("time.sleep")
        || normalized.contains("sleep(")
        || normalized.contains("serve_forever")
        || normalized.contains("while true")
        || normalized.contains("while 1")
        || normalized.contains("input(")
}

pub(crate) fn headless_foreground_detach_after_for_surface(
    client_surface: Option<zorai_protocol::ClientSurface>,
) -> Option<std::time::Duration> {
    matches!(client_surface, Some(zorai_protocol::ClientSurface::Tui))
        .then(|| std::time::Duration::from_secs(TUI_HEADLESS_FOREGROUND_GRACE_SECS))
}

pub(crate) async fn resolve_shell_tool_client_surface(
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Option<zorai_protocol::ClientSurface> {
    if let Some(client_surface) = agent.get_thread_client_surface(thread_id).await {
        return Some(client_surface);
    }

    let task_id = task_id?;
    let (task_thread_ids, goal_run_id) = {
        let task = task_by_id_for_tool_scope(agent, task_id).await?;
        let mut thread_ids = Vec::new();
        if let Some(task_thread_id) = task.thread_id.as_deref() {
            thread_ids.push(task_thread_id.to_string());
        }
        if let Some(parent_thread_id) = task.parent_thread_id.as_deref() {
            thread_ids.push(parent_thread_id.to_string());
        }
        (thread_ids, task.goal_run_id.clone())
    };

    for task_thread_id in task_thread_ids {
        if task_thread_id == thread_id {
            continue;
        }
        if let Some(client_surface) = agent.get_thread_client_surface(&task_thread_id).await {
            return Some(client_surface);
        }
    }

    if let Some(goal_run_id) = goal_run_id {
        return agent.get_goal_run_client_surface(&goal_run_id).await;
    }

    None
}

pub(crate) fn should_use_managed_execution(args: &serde_json::Value) -> bool {
    should_use_managed_execution_for_surface(None, args)
}

pub(crate) fn should_use_managed_execution_for_surface(
    client_surface: Option<zorai_protocol::ClientSurface>,
    args: &serde_json::Value,
) -> bool {
    if matches!(client_surface, Some(zorai_protocol::ClientSurface::Tui)) {
        return false;
    }

    if args
        .get("__weles_force_headless")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return false;
    }

    if args
        .get("session")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }

    if args
        .get("wait_for_completion")
        .and_then(|value| value.as_bool())
        == Some(false)
    {
        return true;
    }

    if args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .is_some_and(|value| value > 600)
    {
        return true;
    }

    if args
        .get("sandbox_enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return true;
    }

    if args
        .get("allow_network")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return true;
    }

    let security_level = args.get("security_level").and_then(|value| value.as_str());

    if matches!(security_level, Some("highest")) {
        return true;
    }

    if !matches!(security_level, Some("yolo"))
        && args
            .get("command")
            .and_then(|value| value.as_str())
            .is_some_and(command_matches_policy_risk)
    {
        return true;
    }

    args.get("command")
        .and_then(|value| value.as_str())
        .map(|command| {
            command_requires_managed_state(command) || command_looks_interactive(command)
        })
        .unwrap_or(false)
}

fn can_run_headless_when_terminal_unavailable(args: &serde_json::Value) -> bool {
    if args
        .get("session")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return false;
    }

    let Some(command) = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };

    if command_requires_managed_state(command) || command_looks_interactive(command) {
        return false;
    }

    let security_level = args.get("security_level").and_then(|value| value.as_str());
    if matches!(security_level, Some("highest")) {
        return false;
    }

    matches!(security_level, Some("yolo")) || !command_matches_policy_risk(command)
}

pub(crate) fn command_matches_policy_risk(command: &str) -> bool {
    if command.trim().is_empty() {
        return false;
    }
    let request = ManagedCommandRequest {
        command: command.to_string(),
        rationale: "policy preflight".to_string(),
        allow_network: false,
        sandbox_enabled: false,
        security_level: SecurityLevel::Lowest,
        cwd: None,
        language_hint: None,
        source: ManagedCommandSource::Agent,
    };
    matches!(
        crate::policy::evaluate_command("tool-exec-routing-check".to_string(), &request, None),
        crate::policy::PolicyDecision::RequireApproval(_)
    )
}

pub(crate) fn command_requires_managed_state(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }

    let normalized = trimmed.to_ascii_lowercase();
    let first = normalized
        .split(|ch: char| ch.is_whitespace() || ch == ';' || ch == '&' || ch == '|')
        .next()
        .unwrap_or_default();

    let requires_persistent_session = matches!(
        first,
        "cd" | "pushd"
            | "popd"
            | "export"
            | "unset"
            | "alias"
            | "unalias"
            | "source"
            | "."
            | "set"
            | "ulimit"
            | "umask"
            | "bind"
            | "shopt"
            | "complete"
            | "compgen"
            | "fg"
            | "bg"
            | "jobs"
    );

    requires_persistent_session && !command_has_followup_work(trimmed)
}

fn command_has_followup_work(command: &str) -> bool {
    ["&&", "||", ";", "\n"].iter().any(|separator| {
        command
            .find(separator)
            .map(|index| !command[index + separator.len()..].trim().is_empty())
            .unwrap_or(false)
    })
}

pub(crate) fn command_looks_interactive(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }

    let mut parts = trimmed.split_whitespace();
    let first = parts.next().unwrap_or_default().to_ascii_lowercase();
    let second = parts.next().map(|value| value.to_ascii_lowercase());

    match first.as_str() {
        "vim" | "nvim" | "nano" | "less" | "more" | "top" | "htop" | "watch" => true,
        "tail" => matches!(second.as_deref(), Some("-f")),
        "ssh" | "sftp" | "scp" | "man" => true,
        "bash" | "zsh" | "fish" => matches!(
            second.as_deref(),
            None | Some("-i") | Some("--interactive") | Some("-l") | Some("--login")
        ),
        "python" | "python3" | "ipython" | "node" => second.is_none(),
        _ => false,
    }
}

fn compact_background_output(raw: &[u8], max_chars: usize) -> String {
    let text = String::from_utf8_lossy(raw).trim().to_string();
    if text.chars().count() <= max_chars {
        return text;
    }

    let tail: String = text
        .chars()
        .rev()
        .take(max_chars)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("... truncated ...\n{tail}")
}

pub(crate) struct HeadlessOutputCapture {
    pub(crate) buffer: Arc<std::sync::Mutex<Vec<u8>>>,
    pub(crate) task: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

pub(crate) fn spawn_headless_output_capture<R>(stream: R) -> HeadlessOutputCapture
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let buffer = Arc::new(std::sync::Mutex::new(Vec::new()));
    let task_buffer = Arc::clone(&buffer);
    let task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stream);
        let mut chunk = [0u8; 8192];
        loop {
            let read = reader.read(&mut chunk).await?;
            if read == 0 {
                return Ok(());
            }
            let mut captured = task_buffer
                .lock()
                .expect("headless command output buffer mutex poisoned");
            captured.extend_from_slice(&chunk[..read]);
        }
    });

    HeadlessOutputCapture { buffer, task }
}

pub(crate) async fn collect_headless_output_capture(
    mut capture: HeadlessOutputCapture,
    max_chars: usize,
    drain_grace: std::time::Duration,
) -> Option<String> {
    tokio::select! {
        _ = &mut capture.task => {}
        _ = tokio::time::sleep(drain_grace) => {
            capture.task.abort();
            let _ = capture.task.await;
        }
    }

    let bytes = capture
        .buffer
        .lock()
        .expect("headless command output buffer mutex poisoned")
        .clone();
    Some(compact_background_output(&bytes, max_chars)).filter(|value| !value.is_empty())
}

pub(crate) async fn execute_headless_shell_command(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    tool_name: &str,
    cancel_token: Option<CancellationToken>,
    foreground_detach_after: Option<std::time::Duration>,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'command' argument"))?;
    let requested_timeout = args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30);
    let auto_background = requested_timeout > 600;
    let wait_for_completion = if auto_background {
        false
    } else {
        tool_waits_for_completion(args)
    };
    let timeout_secs = requested_timeout.min(600);
    let cwd = resolve_tool_cwd(args, session_manager, session_id).await?;

    if !wait_for_completion {
        return spawn_headless_shell_command_background(
            command,
            cwd,
            tool_name,
            requested_timeout,
            auto_background,
        );
    }

    let mut process = tokio::process::Command::new("bash");
    process
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = cwd.as_deref() {
        process.current_dir(cwd);
    }

    let mut child = process
        .spawn()
        .with_context(|| format!("failed to spawn {tool_name} subprocess"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("{tool_name} stdout capture was unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("{tool_name} stderr capture was unavailable"))?;
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
                anyhow::bail!("{tool_name} cancelled while waiting for command completion");
            }
        }
    } else {
        wait_result.await
    };

    let status = match status {
        Ok(result) => result.with_context(|| format!("{tool_name} process wait failed"))?,
        Err(_) if foreground_detach_after.is_some() && foreground_wait < full_timeout => {
            let operation = crate::server::operation_registry().accept_operation(tool_name, None);
            let operation_id = operation.operation_id.clone();
            crate::server::operation_registry().mark_started(&operation_id);
            spawn_headless_shell_monitor(
                operation_id.clone(),
                command.to_string(),
                cwd.clone(),
                child,
                stdout_capture,
                stderr_capture,
            );
            let cwd_suffix = cwd
                .as_ref()
                .map(|path| format!(" in {}", path.display()))
                .unwrap_or_default();
            return Ok((
                format!(
                    "Headless command detached{cwd_suffix} as background operation {operation_id} after {}s foreground grace.\nbackground_task_id: {operation_id}\noperation_id: {operation_id}\nwait_for_completion=true exceeded the TUI foreground grace window. {BACKGROUND_OPERATION_COMPLETION_GUIDANCE}",
                    foreground_wait.as_secs()
                ),
                None,
            ));
        }
        Err(_) => {
            anyhow::bail!("{tool_name} timed out after {timeout_secs}s");
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
        let mut result = format!("Command finished successfully{cwd_suffix} (exit_code: 0).");
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
            "Command failed{cwd_suffix} (exit_code: {:?}).{}",
            status.code(),
            details
        ))
    }
}

fn spawn_headless_shell_command_background(
    command: &str,
    cwd: Option<std::path::PathBuf>,
    tool_name: &str,
    requested_timeout: u64,
    auto_background: bool,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let operation = crate::server::operation_registry().accept_operation(tool_name, None);
    let operation_id = operation.operation_id.clone();

    let mut process = tokio::process::Command::new("bash");
    process
        .arg("-lc")
        .arg(command)
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
            return Err(error).with_context(|| format!("failed to spawn {tool_name} subprocess"));
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("{tool_name} stdout capture was unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("{tool_name} stderr capture was unavailable"))?;
    let stdout_capture = spawn_headless_output_capture(stdout);
    let stderr_capture = spawn_headless_output_capture(stderr);

    crate::server::operation_registry().mark_started(&operation_id);
    spawn_headless_shell_monitor(
        operation_id.clone(),
        command.to_string(),
        cwd.clone(),
        child,
        stdout_capture,
        stderr_capture,
    );

    let cwd_suffix = cwd
        .as_ref()
        .map(|path| format!(" in {}", path.display()))
        .unwrap_or_default();
    let queued_summary =
        format!("Headless command queued{cwd_suffix} as background operation {operation_id}.");

    if auto_background {
        Ok((
            format!(
                "{queued_summary}\nbackground_task_id: {operation_id}\noperation_id: {operation_id}\nCommand auto-backgrounded (requested timeout {}s > max 600s). {BACKGROUND_OPERATION_COMPLETION_GUIDANCE}",
                requested_timeout,
            ),
            None,
        ))
    } else {
        Ok((
            format!(
                "{queued_summary}\nbackground_task_id: {operation_id}\noperation_id: {operation_id}\nNot waiting for completion (wait_for_completion=false; non-quick or long-running commands default to background). {BACKGROUND_OPERATION_COMPLETION_GUIDANCE}"
            ),
            None,
        ))
    }
}

fn headless_operation_cancel_tokens(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, CancellationToken>> {
    static TOKENS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, CancellationToken>>,
    > = std::sync::OnceLock::new();
    TOKENS.get_or_init(Default::default)
}

pub(crate) fn cancel_headless_operation(operation_id: &str) -> bool {
    let tokens = headless_operation_cancel_tokens()
        .lock()
        .expect("headless operation cancel mutex poisoned");
    match tokens.get(operation_id) {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
}

fn spawn_headless_shell_monitor(
    operation_id: String,
    command: String,
    cwd: Option<PathBuf>,
    mut child: tokio::process::Child,
    stdout_capture: HeadlessOutputCapture,
    stderr_capture: HeadlessOutputCapture,
) {
    let cwd_for_task = cwd.as_ref().map(|path| path.display().to_string());
    let started_at = std::time::Instant::now();
    let kill_token = CancellationToken::new();
    headless_operation_cancel_tokens()
        .lock()
        .expect("headless operation cancel mutex poisoned")
        .insert(operation_id.clone(), kill_token.clone());
    tokio::spawn(async move {
        let outcome = tokio::select! {
            outcome = child.wait() => outcome,
            _ = kill_token.cancelled() => {
                let _ = child.start_kill();
                child.wait().await
            }
        };
        let was_cancelled = kill_token.is_cancelled();
        headless_operation_cancel_tokens()
            .lock()
            .expect("headless operation cancel mutex poisoned")
            .remove(&operation_id);
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
            Ok(status) if status.success() && !was_cancelled => {
                crate::server::operation_registry().mark_completed_with_terminal_result(
                    &operation_id,
                    serde_json::json!({
                        "command": command,
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
                        "command": command,
                        "cwd": cwd_for_task,
                        "exit_code": status.code(),
                        "duration_ms": duration_ms,
                        "cancelled": was_cancelled,
                        "stdout": stdout,
                        "stderr": stderr,
                    }),
                );
            }
            Err(error) => {
                crate::server::operation_registry().mark_failed_with_terminal_result(
                    &operation_id,
                    serde_json::json!({
                        "command": command,
                        "cwd": cwd_for_task,
                        "duration_ms": duration_ms,
                        "cancelled": was_cancelled,
                        "spawn_error": error.to_string(),
                        "stdout": stdout,
                        "stderr": stderr,
                    }),
                );
            }
        }
    });
}
