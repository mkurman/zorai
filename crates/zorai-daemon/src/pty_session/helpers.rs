use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) fn pty_reader_loop(
    id: SessionId,
    mut reader: Box<dyn Read + Send>,
    master_write: Arc<std::sync::Mutex<Box<dyn Write + Send>>>,
    tx: broadcast::Sender<DaemonMessage>,
    scrollback: Arc<std::sync::Mutex<Vec<u8>>>,
    dead: Arc<AtomicBool>,
    managed_lane: Arc<std::sync::Mutex<ManagedLaneState>>,
    active_command: Arc<std::sync::Mutex<Option<String>>>,
    tracked_cwd: Arc<std::sync::Mutex<Option<String>>>,
    last_activity_at_ms: Arc<AtomicU64>,
    history: HistoryStore,
    workspace_id: Option<String>,
    cwd: Option<String>,
    rt_handle: tokio::runtime::Handle,
) {
    let mut buf = [0u8; 4096];
    let mut marker_tail = Vec::<u8>::new();
    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                tracing::info!(%id, "PTY reader reached EOF");
                notify_session_exited(id, &tx, &dead, None);
                break;
            }
            Ok(n) => {
                last_activity_at_ms.store(super::unix_millis(), Ordering::Relaxed);
                tracing::trace!(%id, bytes = n, "PTY reader got output");
                let mut chunk = Vec::with_capacity(marker_tail.len() + n);
                if !marker_tail.is_empty() {
                    chunk.extend_from_slice(&marker_tail);
                    marker_tail.clear();
                }
                chunk.extend_from_slice(&buf[..n]);

                let (markers, after_markers, trailing_markers) = extract_command_markers(&chunk);
                marker_tail = trailing_markers;
                let (notifications, data) = parse_osc_notifications(&after_markers);

                {
                    let mut sb = scrollback.lock().unwrap();
                    sb.extend_from_slice(&data);
                    if sb.len() > SCROLLBACK_CAPACITY {
                        let excess = sb.len() - SCROLLBACK_CAPACITY;
                        sb.drain(..excess);
                    }
                }

                let _ = tx.send(DaemonMessage::Output { id, data });

                for notification in notifications {
                    let _ = tx.send(DaemonMessage::OscNotification { id, notification });
                }

                for marker in markers {
                    match marker {
                        CommandLifecycleMarker::Started(command) => {
                            *active_command.lock().unwrap() = Some(command.clone());
                            let _ = tx.send(DaemonMessage::CommandStarted {
                                id,
                                command: command.clone(),
                            });
                            let lane = managed_lane.lock().unwrap();
                            if let Some(active) = lane.active.as_ref() {
                                let _ = tx.send(DaemonMessage::ManagedCommandStarted {
                                    id,
                                    execution_id: active.execution_id.clone(),
                                    command,
                                    source: active.request.source,
                                });
                            }
                        }
                        CommandLifecycleMarker::Finished(exit_code) => {
                            *active_command.lock().unwrap() = None;
                            let _ = tx.send(DaemonMessage::CommandFinished { id, exit_code });

                            let completed = {
                                let mut lane = managed_lane.lock().unwrap();
                                let completed = lane.active.take();
                                if let Some(next) = lane.queue.pop_front() {
                                    if let Err(error) = dispatch_managed_command(
                                        &master_write,
                                        &next.request,
                                        cwd.as_deref(),
                                    ) {
                                        tracing::error!(%id, error = %error, "failed to dispatch queued managed command");
                                    } else {
                                        lane.active = Some(ActiveManagedCommand {
                                            execution_id: next.execution_id,
                                            request: next.request,
                                            snapshot: next.snapshot,
                                            enqueued_at: Instant::now(),
                                        });
                                    }
                                }
                                completed
                            };

                            if let Some(active) = completed {
                                let duration_ms = active.enqueued_at.elapsed().as_millis() as u64;
                                let _ = tx.send(DaemonMessage::ManagedCommandFinished {
                                    id,
                                    execution_id: active.execution_id.clone(),
                                    command: active.request.command.clone(),
                                    exit_code,
                                    duration_ms: Some(duration_ms),
                                    snapshot: active.snapshot.clone(),
                                });

                                let record = ManagedHistoryRecord {
                                    execution_id: active.execution_id,
                                    session_id: id.to_string(),
                                    workspace_id: workspace_id.clone(),
                                    command: active.request.command,
                                    rationale: active.request.rationale,
                                    source: format!("{:?}", active.request.source)
                                        .to_ascii_lowercase(),
                                    exit_code,
                                    duration_ms: Some(duration_ms),
                                    snapshot_path: active
                                        .snapshot
                                        .as_ref()
                                        .map(|snapshot| snapshot.path.clone()),
                                };
                                if let Err(error) =
                                    rt_handle.block_on(history.record_managed_finish(&record))
                                {
                                    tracing::error!(%id, error = %error, cwd = ?cwd, "failed to persist managed history");
                                }

                                if record.exit_code == Some(0) {
                                    if let Ok(candidates) =
                                        rt_handle.block_on(history.detect_skill_candidates())
                                    {
                                        for (title, _hits) in candidates.iter().take(1) {
                                            if let Ok((_title, path)) = rt_handle.block_on(
                                                history.generate_skill(Some(title), Some(title)),
                                            ) {
                                                tracing::info!(skill_path = %path, "auto-generated skill from workflow pattern");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        CommandLifecycleMarker::Cwd(dir) => {
                            *tracked_cwd.lock().unwrap() = Some(dir.clone());
                            let _ = tx.send(DaemonMessage::CwdChanged { id, cwd: dir });
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!(%id, error = %e, "PTY read error");
                notify_session_exited(id, &tx, &dead, None);
                break;
            }
        }
    }
}

pub(super) fn dispatch_managed_command(
    master_write: &Arc<std::sync::Mutex<Box<dyn Write + Send>>>,
    request: &ManagedCommandRequest,
    fallback_cwd: Option<&str>,
) -> Result<()> {
    let command_line = if request.sandbox_enabled {
        let workspace_root = request.cwd.as_deref().or(fallback_cwd).unwrap_or(".");
        let sandbox_impl = sandbox::detect_sandbox();
        let wrapped = sandbox_impl.wrap(&request.command, workspace_root, request.allow_network);
        tracing::info!(
            sandbox = sandbox_impl.name(),
            workspace_root,
            allow_network = request.allow_network,
            "dispatching managed command with sandbox"
        );
        shell_join(&wrapped.program, &wrapped.args)
    } else if request.allow_network {
        tracing::warn!("sandbox disabled for managed command; dispatching raw command");
        request.command.clone()
    } else {
        let wrapped = network::wrap_network(&request.command, request.allow_network);
        tracing::warn!("sandbox disabled for managed command; using network wrapper only");
        shell_join(&wrapped.program, &wrapped.args)
    };

    let mut writer = master_write.lock().unwrap();
    writer.write_all(command_line.as_bytes())?;
    writer.write_all(b"\r")?;
    writer.flush()?;
    Ok(())
}

fn shell_join(program: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(shell_escape(program));
    for arg in args {
        parts.push(shell_escape(arg));
    }
    parts.join(" ")
}

fn shell_escape(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

fn extract_command_markers(data: &[u8]) -> (Vec<CommandLifecycleMarker>, Vec<u8>, Vec<u8>) {
    let mut markers = Vec::new();
    let mut cleaned = Vec::with_capacity(data.len());
    let mut trailing = Vec::new();
    let mut i = 0;

    while i < data.len() {
        if i + 1 < data.len() && data[i] == 0x1b && data[i + 1] == b']' {
            let osc_start = i + 2;
            let mut end = osc_start;
            let mut terminator_len = 0;

            while end < data.len() {
                if data[end] == 0x07 {
                    terminator_len = 1;
                    break;
                }
                if end + 1 < data.len() && data[end] == 0x1b && data[end + 1] == b'\\' {
                    terminator_len = 2;
                    break;
                }
                end += 1;
            }

            if terminator_len > 0 {
                let payload = &data[osc_start..end];
                if let Ok(text) = std::str::from_utf8(payload) {
                    if let Some(command_b64) = text.strip_prefix("133;C;") {
                        let command = base64::engine::general_purpose::STANDARD
                            .decode(command_b64)
                            .ok()
                            .and_then(|bytes| String::from_utf8(bytes).ok())
                            .unwrap_or_else(|| command_b64.to_string());
                        markers.push(CommandLifecycleMarker::Started(command));
                        i = end + terminator_len;
                        continue;
                    }

                    if let Some(rest) = text.strip_prefix("133;D") {
                        let exit_code = rest
                            .trim_start_matches(';')
                            .split(';')
                            .next()
                            .and_then(|value| value.parse::<i32>().ok());
                        markers.push(CommandLifecycleMarker::Finished(exit_code));
                        i = end + terminator_len;
                        continue;
                    }

                    if let Some(cwd_b64) = text.strip_prefix("133;P;") {
                        let dir = base64::engine::general_purpose::STANDARD
                            .decode(cwd_b64)
                            .ok()
                            .and_then(|bytes| String::from_utf8(bytes).ok())
                            .unwrap_or_else(|| cwd_b64.to_string());
                        markers.push(CommandLifecycleMarker::Cwd(dir));
                        i = end + terminator_len;
                        continue;
                    }
                }
            } else {
                trailing.extend_from_slice(&data[i..]);
                break;
            }
        }

        cleaned.push(data[i]);
        i += 1;
    }

    (markers, cleaned, trailing)
}

pub fn sanitize_scrollback_for_replay(data: &[u8]) -> Vec<u8> {
    let mut cleaned = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        if i + 1 < data.len() && data[i] == 0x1b && data[i + 1] == b'[' {
            let mut j = i + 2;
            while j < data.len() && (0x30..=0x3F).contains(&data[j]) {
                j += 1;
            }
            while j < data.len() && (0x20..=0x2F).contains(&data[j]) {
                j += 1;
            }
            if j < data.len() && (0x40..=0x7E).contains(&data[j]) {
                let final_byte = data[j];
                let params_start = i + 2;
                let should_strip = match final_byte {
                    b'I' | b'O' => true,
                    b'c' => {
                        let first = data.get(params_start).copied();
                        matches!(first, Some(b'?') | Some(b'>') | Some(b'=')) || params_start == j
                    }
                    b'R' => data[params_start..j]
                        .iter()
                        .all(|&b| b.is_ascii_digit() || b == b';'),
                    _ => false,
                };
                if should_strip {
                    i = j + 1;
                    continue;
                }
            }
        }
        cleaned.push(data[i]);
        i += 1;
    }

    cleaned
}

fn notify_session_exited(
    id: SessionId,
    tx: &broadcast::Sender<DaemonMessage>,
    dead: &Arc<AtomicBool>,
    exit_code: Option<i32>,
) {
    if dead
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        let _ = tx.send(DaemonMessage::SessionExited { id, exit_code });
    }
}

pub(super) fn default_shell() -> String {
    #[cfg(windows)]
    {
        detect_windows_shell()
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

const BASH_ZORAI_RC: &str = r#"# zorai shell integration for command lifecycle markers
if [ -f /etc/bash.bashrc ]; then
    . /etc/bash.bashrc
fi

if [ -f ~/.bashrc ]; then
    . ~/.bashrc
fi

__zorai_precmd() {
    local exit_code=$?
    printf '\033]133;D;%s\a' "$exit_code"
    local cwd_b64
    cwd_b64="$(printf '%s' "$PWD" | base64 | tr -d '\r\n')"
    printf '\033]133;P;%s\a' "$cwd_b64"
}

__zorai_preexec() {
    local cmd="$BASH_COMMAND"

    case "$cmd" in
        __zorai_precmd*|__zorai_preexec*|history*|fc*|"" )
            return
            ;;
    esac

    local cmd_b64
    cmd_b64="$(printf '%s' "$cmd" | base64 | tr -d '\r\n')"
    printf '\033]133;C;%s\a' "$cmd_b64"
}

trap '__zorai_preexec' DEBUG

if [ -n "${PROMPT_COMMAND:-}" ]; then
    PROMPT_COMMAND="__zorai_precmd;${PROMPT_COMMAND}"
else
    PROMPT_COMMAND="__zorai_precmd"
fi
"#;

fn ensure_bash_rc() -> Result<std::path::PathBuf> {
    let data_dir = zorai_protocol::ensure_zorai_data_dir()?;
    let integration_dir = data_dir.join("shell");
    std::fs::create_dir_all(&integration_dir)?;
    let bash_rc_path = integration_dir.join("bash_zorai_rc.sh");
    std::fs::write(&bash_rc_path, BASH_ZORAI_RC)?;
    Ok(bash_rc_path)
}

#[cfg(windows)]
fn windows_path_to_wsl(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    let bytes = s.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = &s[3..];
        format!("/mnt/{}/{}", drive, rest.replace('\\', "/"))
    } else {
        s.replace('\\', "/")
    }
}

#[cfg(windows)]
pub(super) fn configure_shell_command(
    shell_program: &str,
    cmd: &mut CommandBuilder,
    cwd: Option<&str>,
) -> Result<()> {
    let shell_name = std::path::Path::new(shell_program)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase());

    match shell_name.as_deref() {
        Some("powershell.exe") | Some("pwsh.exe") => {
            cmd.arg("-NoLogo");
            cmd.arg("-NoExit");
        }
        Some("wsl") | Some("wsl.exe") => {
            if let Some(dir) = cwd {
                cmd.arg("--cd");
                cmd.arg(dir);
            }
            match ensure_bash_rc() {
                Ok(rc_path) => {
                    let wsl_rc_path = windows_path_to_wsl(&rc_path);
                    cmd.arg("--");
                    cmd.arg("bash");
                    cmd.arg("--rcfile");
                    cmd.arg(wsl_rc_path);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to write bash RC for WSL; launching without integration");
                }
            }
        }
        _ => {}
    }

    Ok(())
}

#[cfg(not(windows))]
pub(super) fn configure_shell_command(
    shell_program: &str,
    cmd: &mut CommandBuilder,
    _cwd: Option<&str>,
) -> Result<()> {
    let shell_name = std::path::Path::new(shell_program)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase());

    if matches!(shell_name.as_deref(), Some("bash")) {
        let bash_rc_path = ensure_bash_rc()?;
        cmd.arg("--rcfile");
        cmd.arg(bash_rc_path.to_string_lossy().to_string());
    }

    Ok(())
}

#[cfg(windows)]
fn detect_windows_shell() -> String {
    use std::path::PathBuf;

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(
            PathBuf::from(&program_files)
                .join("PowerShell")
                .join("7")
                .join("pwsh.exe"),
        );
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(&local_app_data)
                .join("Microsoft")
                .join("WindowsApps")
                .join("pwsh.exe"),
        );
    }
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        let system_root = PathBuf::from(system_root);
        candidates.push(
            system_root
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe"),
        );
        candidates.push(system_root.join("System32").join("cmd.exe"));
    }
    if let Some(comspec) = std::env::var_os("COMSPEC") {
        candidates.push(PathBuf::from(comspec));
    }
    if let Some(path_hit) = find_in_path("pwsh.exe") {
        candidates.insert(0, path_hit);
    }
    for candidate in candidates {
        if candidate.is_file() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    if let Some(path_hit) = find_in_path("powershell.exe") {
        return path_hit.to_string_lossy().into_owned();
    }
    if let Some(path_hit) = find_in_path("cmd.exe") {
        return path_hit.to_string_lossy().into_owned();
    }
    "cmd.exe".to_string()
}

#[cfg(windows)]
fn find_in_path(binary: &str) -> Option<std::path::PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
