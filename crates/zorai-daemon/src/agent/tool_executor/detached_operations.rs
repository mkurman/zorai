use super::*;

const DETACHED_LOG_FILE: &str = "output.log";
const DETACHED_STDERR_FILE: &str = "stderr.log";
const DETACHED_EXIT_FILE: &str = "exit_code";
const DETACHED_META_FILE: &str = "meta.json";
const DETACHED_FINALIZED_FILE: &str = "finalized";
const DETACHED_LOG_TAIL_BYTES: u64 = 64 * 1024;
const DETACHED_RESULT_TAIL_CHARS: usize = 4000;
const DETACHED_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
const DETACHED_RETENTION: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct DetachedOperationMeta {
    pub(crate) operation_id: String,
    pub(crate) tool_name: String,
    pub(crate) command: String,
    pub(crate) cwd: Option<String>,
    pub(crate) pid: u32,
    pub(crate) started_at_ms: u64,
    pub(crate) thread_id: Option<String>,
    pub(crate) notify_on_completion: bool,
}

pub(crate) struct DetachedLaunch<'a> {
    pub(crate) tool_name: &'a str,
    pub(crate) interpreter: Vec<String>,
    pub(crate) script_name: &'a str,
    pub(crate) script: &'a str,
    pub(crate) display_command: &'a str,
    pub(crate) cwd: Option<&'a Path>,
    pub(crate) thread_id: Option<&'a str>,
    pub(crate) notify_on_completion: bool,
}

fn cancelled_detached_operations() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static CANCELLED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    CANCELLED.get_or_init(Default::default)
}

fn live_detached_operations(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, DetachedOperationMeta>> {
    static LIVE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, DetachedOperationMeta>>,
    > = std::sync::OnceLock::new();
    LIVE.get_or_init(Default::default)
}

pub(crate) fn detached_operations_root() -> PathBuf {
    zorai_protocol::zorai_data_dir().join("operations")
}

fn detached_operation_dir_in(root: &Path, operation_id: &str) -> PathBuf {
    root.join(operation_id)
}

pub(crate) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn detached_wrapper_script(
    interpreter: &[String],
    script_path: &Path,
    dir: &Path,
) -> String {
    let run = interpreter
        .iter()
        .map(|part| shell_single_quote(part))
        .chain(std::iter::once(shell_single_quote(
            &script_path.to_string_lossy(),
        )))
        .collect::<Vec<_>>()
        .join(" ");
    let log = shell_single_quote(&dir.join(DETACHED_LOG_FILE).to_string_lossy());
    let stderr_log = shell_single_quote(&dir.join(DETACHED_STDERR_FILE).to_string_lossy());
    let exit_tmp = shell_single_quote(
        &dir.join(format!("{DETACHED_EXIT_FILE}.tmp"))
            .to_string_lossy(),
    );
    let exit = shell_single_quote(&dir.join(DETACHED_EXIT_FILE).to_string_lossy());
    format!("{run} > {log} 2> {stderr_log} < /dev/null; code=$?; printf '%s' \"$code\" > {exit_tmp}; mv {exit_tmp} {exit}")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub(crate) async fn spawn_detached_operation(launch: DetachedLaunch<'_>) -> Result<String> {
    spawn_detached_operation_in(&detached_operations_root(), launch).await
}

pub(crate) async fn spawn_detached_operation_in(
    root: &Path,
    launch: DetachedLaunch<'_>,
) -> Result<String> {
    let registry = crate::server::operation_registry();
    let operation_id = registry
        .accept_operation(launch.tool_name, None)
        .operation_id;
    let dir = detached_operation_dir_in(root, &operation_id);
    let prepared = async {
        tokio::fs::create_dir_all(&dir).await?;
        let script_path = dir.join(launch.script_name);
        tokio::fs::write(&script_path, launch.script).await?;
        tokio::fs::write(dir.join(DETACHED_LOG_FILE), b"").await?;
        Ok::<_, std::io::Error>(script_path)
    }
    .await;
    let script_path = match prepared {
        Ok(path) => path,
        Err(error) => {
            registry.mark_failed(&operation_id);
            return Err(error).context("failed to prepare detached operation directory");
        }
    };

    let mut process = tokio::process::Command::new("bash");
    process
        .arg("-c")
        .arg(detached_wrapper_script(
            &launch.interpreter,
            &script_path,
            &dir,
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    #[cfg(unix)]
    process.process_group(0);
    if let Some(cwd) = launch.cwd {
        process.current_dir(cwd);
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            registry.mark_failed(&operation_id);
            return Err(error)
                .with_context(|| format!("failed to spawn detached {}", launch.tool_name));
        }
    };

    let meta = DetachedOperationMeta {
        operation_id: operation_id.clone(),
        tool_name: launch.tool_name.to_string(),
        command: launch.display_command.to_string(),
        cwd: launch.cwd.map(|path| path.display().to_string()),
        pid: child.id().unwrap_or_default(),
        started_at_ms: now_ms(),
        thread_id: launch.thread_id.map(str::to_string),
        notify_on_completion: launch.notify_on_completion,
    };
    if let Ok(json) = serde_json::to_vec_pretty(&meta) {
        let _ = tokio::fs::write(dir.join(DETACHED_META_FILE), json).await;
    }
    live_detached_operations()
        .lock()
        .expect("detached operations mutex poisoned")
        .insert(operation_id.clone(), meta);
    registry.mark_started(&operation_id);

    let root = root.to_path_buf();
    let monitored_id = operation_id.clone();
    tokio::spawn(async move {
        let _ = child.wait().await;
        finalize_detached_operation_in(&root, &monitored_id).await;
    });
    Ok(operation_id)
}

pub(crate) async fn read_log_tail(path: &Path, max_bytes: u64) -> Vec<u8> {
    use tokio::io::AsyncSeekExt;
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return Vec::new();
    };
    let len = file
        .metadata()
        .await
        .map(|meta| meta.len())
        .unwrap_or_default();
    if len > max_bytes
        && file
            .seek(std::io::SeekFrom::Start(len - max_bytes))
            .await
            .is_err()
    {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes).await;
    bytes
}

fn read_log_tail_blocking(path: &Path, max_bytes: u64) -> Vec<u8> {
    use std::io::{Read, Seek};
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let len = file.metadata().map(|meta| meta.len()).unwrap_or_default();
    if len > max_bytes
        && file
            .seek(std::io::SeekFrom::Start(len - max_bytes))
            .is_err()
    {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    bytes
}

pub(crate) fn detached_operation_output(operation_id: &str) -> Option<(String, Vec<u8>)> {
    let command = live_detached_operations()
        .lock()
        .expect("detached operations mutex poisoned")
        .get(operation_id)?
        .command
        .clone();
    let dir = detached_operation_dir_in(&detached_operations_root(), operation_id);
    let mut bytes = read_log_tail_blocking(&dir.join(DETACHED_LOG_FILE), DETACHED_LOG_TAIL_BYTES);
    let stderr = read_log_tail_blocking(&dir.join(DETACHED_STDERR_FILE), DETACHED_LOG_TAIL_BYTES);
    if !stderr.is_empty() {
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(&stderr);
    }
    Some((command, bytes))
}

pub(crate) fn cancel_detached_operation(operation_id: &str) -> bool {
    let pid = match live_detached_operations()
        .lock()
        .expect("detached operations mutex poisoned")
        .get(operation_id)
    {
        Some(meta) if meta.pid > 0 => meta.pid,
        _ => return false,
    };
    let signalled = std::process::Command::new("kill")
        .arg("-TERM")
        .arg("--")
        .arg(format!("-{pid}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if signalled {
        cancelled_detached_operations()
            .lock()
            .expect("detached cancel mutex poisoned")
            .insert(operation_id.to_string());
    }
    signalled
}

pub(crate) async fn finalize_detached_operation_in(root: &Path, operation_id: &str) {
    let dir = detached_operation_dir_in(root, operation_id);
    if tokio::fs::metadata(dir.join(DETACHED_FINALIZED_FILE))
        .await
        .is_ok()
    {
        return;
    }
    let live_meta = live_detached_operations()
        .lock()
        .expect("detached operations mutex poisoned")
        .remove(operation_id);
    let meta = match live_meta {
        Some(meta) => meta,
        None => match tokio::fs::read(dir.join(DETACHED_META_FILE)).await {
            Ok(bytes) => match serde_json::from_slice::<DetachedOperationMeta>(&bytes) {
                Ok(meta) => meta,
                Err(_) => return,
            },
            Err(_) => return,
        },
    };
    let exit_code = tokio::fs::read_to_string(dir.join(DETACHED_EXIT_FILE))
        .await
        .ok()
        .and_then(|text| text.trim().parse::<i32>().ok());
    let log_path = dir.join(DETACHED_LOG_FILE);
    let stderr_path = dir.join(DETACHED_STDERR_FILE);
    let stdout = compact_background_output(
        &read_log_tail(&log_path, DETACHED_LOG_TAIL_BYTES).await,
        DETACHED_RESULT_TAIL_CHARS,
    );
    let stderr = compact_background_output(
        &read_log_tail(&stderr_path, DETACHED_LOG_TAIL_BYTES).await,
        DETACHED_RESULT_TAIL_CHARS,
    );
    let cancelled = cancelled_detached_operations()
        .lock()
        .expect("detached cancel mutex poisoned")
        .remove(operation_id);
    let result = serde_json::json!({
        "command": meta.command,
        "cwd": meta.cwd,
        "exit_code": exit_code,
        "duration_ms": now_ms().saturating_sub(meta.started_at_ms),
        "stdout": stdout,
        "stderr": stderr,
        "log_path": log_path.display().to_string(),
        "stderr_path": stderr_path.display().to_string(),
        "cancelled": cancelled,
        "detached": true,
        "error": if exit_code.is_none() { Some("the process ended without reporting an exit code (killed or lost)") } else { None },
    });
    let registry = crate::server::operation_registry();
    if exit_code == Some(0) {
        registry.mark_completed_with_terminal_result(operation_id, result);
    } else {
        registry.mark_failed_with_terminal_result(operation_id, result);
    }
    let _ = tokio::fs::write(dir.join(DETACHED_FINALIZED_FILE), b"").await;
}

fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if Path::new("/proc").is_dir() {
        return Path::new(&format!("/proc/{pid}")).exists();
    }
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) async fn restore_detached_operations() -> Vec<DetachedOperationMeta> {
    restore_detached_operations_in(&detached_operations_root()).await
}

pub(crate) async fn restore_detached_operations_in(root: &Path) -> Vec<DetachedOperationMeta> {
    let mut restored = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return restored;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let dir = entry.path();
        if tokio::fs::metadata(dir.join(DETACHED_FINALIZED_FILE))
            .await
            .is_ok()
        {
            let expired = tokio::fs::metadata(&dir)
                .await
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > DETACHED_RETENTION);
            if expired {
                let _ = tokio::fs::remove_dir_all(&dir).await;
            }
            continue;
        }
        let Ok(bytes) = tokio::fs::read(dir.join(DETACHED_META_FILE)).await else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<DetachedOperationMeta>(&bytes) else {
            continue;
        };
        let registry = crate::server::operation_registry();
        registry.restore_operation(&meta.operation_id, &meta.tool_name);
        registry.mark_started(&meta.operation_id);
        live_detached_operations()
            .lock()
            .expect("detached operations mutex poisoned")
            .insert(meta.operation_id.clone(), meta.clone());
        let root = root.to_path_buf();
        let watched = meta.clone();
        tokio::spawn(async move {
            let dir = detached_operation_dir_in(&root, &watched.operation_id);
            loop {
                if tokio::fs::metadata(dir.join(DETACHED_EXIT_FILE))
                    .await
                    .is_ok()
                    || !pid_is_alive(watched.pid)
                {
                    finalize_detached_operation_in(&root, &watched.operation_id).await;
                    return;
                }
                tokio::time::sleep(DETACHED_POLL_INTERVAL).await;
            }
        });
        restored.push(meta);
    }
    restored
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn launch<'a>(script: &'a str, root_cmd: &'a str) -> DetachedLaunch<'a> {
        DetachedLaunch {
            tool_name: "bash_command",
            interpreter: vec!["bash".to_string()],
            script_name: "command.sh",
            script,
            display_command: root_cmd,
            cwd: None,
            thread_id: Some("thread-1"),
            notify_on_completion: true,
        }
    }

    async fn wait_terminal(operation_id: &str) -> serde_json::Value {
        for _ in 0..200 {
            if let Some(result) = crate::server::operation_registry().terminal_result(operation_id)
            {
                return result;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("detached operation never finished");
    }

    #[test]
    fn background_bash_runs_detached_unless_it_needs_a_managed_terminal_for_another_reason() {
        let detached = |args: serde_json::Value| bash_background_runs_detached(&args);
        assert!(detached(
            serde_json::json!({"command": "python train.py", "wait_for_completion": false})
        ));
        assert!(detached(
            serde_json::json!({"command": "python train.py", "timeout_seconds": 3600})
        ));
        assert!(!detached(serde_json::json!({"command": "ls"})));
        assert!(!detached(
            serde_json::json!({"command": "python train.py", "wait_for_completion": false, "session": "abc"})
        ));
        assert!(!detached(
            serde_json::json!({"command": "python train.py", "wait_for_completion": false, "sandbox_enabled": true})
        ));
        assert!(!detached(
            serde_json::json!({"command": "python train.py", "wait_for_completion": false, "security_level": "highest"})
        ));
    }

    #[test]
    fn captured_background_output_keeps_only_a_bounded_tail() {
        let mut buffer = vec![b'a'; HEADLESS_OUTPUT_KEEP_BYTES * 2];
        trim_captured_output(&mut buffer);
        assert_eq!(buffer.len(), HEADLESS_OUTPUT_KEEP_BYTES * 2);
        buffer.extend_from_slice(b"latest");
        trim_captured_output(&mut buffer);
        assert_eq!(buffer.len(), HEADLESS_OUTPUT_KEEP_BYTES);
        assert!(buffer.ends_with(b"latest"));
    }

    #[test]
    fn single_quoting_survives_embedded_quotes() {
        assert_eq!(shell_single_quote("it's"), "'it'\\''s'");
    }

    #[tokio::test]
    async fn output_goes_to_a_log_file_and_the_exit_code_is_reported_without_streaming_through_the_daemon(
    ) {
        let root = tempfile::tempdir().unwrap();
        let id = spawn_detached_operation_in(
            root.path(),
            launch("echo hello; echo oops >&2; exit 3", "demo"),
        )
        .await
        .unwrap();
        let result = wait_terminal(&id).await;
        assert_eq!(result["exit_code"], 3);
        assert!(result["stdout"].as_str().unwrap().contains("hello"));
        assert!(result["stderr"].as_str().unwrap().contains("oops"));
        assert!(root.path().join(&id).join(DETACHED_FINALIZED_FILE).exists());
        let log = std::fs::read_to_string(root.path().join(&id).join(DETACHED_LOG_FILE)).unwrap();
        assert!(log.contains("hello"));
    }

    #[tokio::test]
    async fn jobs_run_in_their_own_process_group_so_daemon_signals_miss_them_but_cancel_stops_them()
    {
        let root = tempfile::tempdir().unwrap();
        let id = spawn_detached_operation_in(root.path(), launch("sleep 30", "sleep 30"))
            .await
            .unwrap();
        let pid = live_detached_operations()
            .lock()
            .unwrap()
            .get(&id)
            .unwrap()
            .pid;
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let fields = stat
            .rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        let process_group: u32 = fields[2].parse().unwrap();
        assert_eq!(process_group, pid);
        assert_ne!(process_group, std::process::id());
        assert!(cancel_detached_operation(&id));
        let result = wait_terminal(&id).await;
        assert_ne!(result["exit_code"], 0);
        assert_eq!(result["cancelled"], true);
    }

    #[tokio::test]
    async fn a_daemon_restart_picks_up_jobs_that_finished_while_it_was_down() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("op-from-previous-daemon");
        std::fs::create_dir_all(&dir).unwrap();
        let meta = DetachedOperationMeta {
            operation_id: "op-from-previous-daemon".to_string(),
            tool_name: "python_execute".to_string(),
            command: "train.py".to_string(),
            cwd: None,
            pid: 0,
            started_at_ms: now_ms(),
            thread_id: Some("thread-9".to_string()),
            notify_on_completion: true,
        };
        std::fs::write(
            dir.join(DETACHED_META_FILE),
            serde_json::to_vec(&meta).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join(DETACHED_LOG_FILE), "epoch 10 done\n").unwrap();
        std::fs::write(dir.join(DETACHED_EXIT_FILE), "0").unwrap();

        let restored = restore_detached_operations_in(root.path()).await;
        assert_eq!(restored, vec![meta]);
        let result = wait_terminal("op-from-previous-daemon").await;
        assert_eq!(result["exit_code"], 0);
        assert!(result["stdout"].as_str().unwrap().contains("epoch 10 done"));
        assert!(restore_detached_operations_in(root.path()).await.is_empty());
    }

    #[tokio::test]
    async fn a_job_that_vanished_without_an_exit_code_is_reported_as_failed() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("op-lost");
        std::fs::create_dir_all(&dir).unwrap();
        let meta = DetachedOperationMeta {
            operation_id: "op-lost".to_string(),
            tool_name: "bash_command".to_string(),
            command: "sleep".to_string(),
            cwd: None,
            pid: 0,
            started_at_ms: now_ms(),
            thread_id: None,
            notify_on_completion: false,
        };
        std::fs::write(
            dir.join(DETACHED_META_FILE),
            serde_json::to_vec(&meta).unwrap(),
        )
        .unwrap();
        restore_detached_operations_in(root.path()).await;
        let result = wait_terminal("op-lost").await;
        assert!(result["exit_code"].is_null());
        assert!(result["error"]
            .as_str()
            .unwrap()
            .contains("without reporting an exit code"));
    }
}
