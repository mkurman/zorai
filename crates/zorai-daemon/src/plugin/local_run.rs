use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::AsyncWriteExt;

use super::api_proxy::PluginApiError;
use super::manifest::EndpointRunDef;

pub const DEFAULT_LOCAL_RUN_TIMEOUT_SECS: u64 = 120;
pub const MAX_LOCAL_RUN_TIMEOUT_SECS: u64 = 3600;
const MAX_STDOUT_BYTES: usize = 1024 * 1024;
const STDERR_TAIL_CHARS: usize = 4000;
const APPROVAL_FILE: &str = "local-run-approved";

pub fn local_run_timeout_secs(run: &EndpointRunDef) -> u64 {
    run.timeout_secs
        .unwrap_or(DEFAULT_LOCAL_RUN_TIMEOUT_SECS)
        .clamp(1, MAX_LOCAL_RUN_TIMEOUT_SECS)
}

pub fn plugin_data_dir(plugins_dir: &Path, plugin_name: &str) -> PathBuf {
    plugins_dir
        .parent()
        .unwrap_or(plugins_dir)
        .join("plugin-data")
        .join(plugin_name)
}

pub fn local_run_approved(data_dir: &Path, version: &str) -> bool {
    std::fs::read_to_string(data_dir.join(APPROVAL_FILE))
        .map(|approved| approved.trim() == version)
        .unwrap_or(false)
}

pub fn approve_local_run(data_dir: &Path, version: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(data_dir.join(APPROVAL_FILE), format!("{version}\n"))
}

pub fn sibling_cli_path(daemon_exe: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "zorai.exe" } else { "zorai" };
    let cli = daemon_exe.parent()?.join(name);
    cli.is_file().then_some(cli)
}

pub fn local_run_shell(setup: &str, command: &str) -> String {
    format!("exec 3>&1 1>&2\n{setup}\nexec 1>&3 3>&-\n{command}")
}

pub struct LocalRunRequest<'a> {
    pub plugin_name: &'a str,
    pub plugin_dir: &'a Path,
    pub data_dir: &'a Path,
    pub shell: &'a str,
    pub input: serde_json::Value,
    pub timeout_secs: u64,
}

pub async fn run_local_endpoint(request: LocalRunRequest<'_>) -> Result<String, PluginApiError> {
    std::fs::create_dir_all(request.data_dir).map_err(|error| PluginApiError::LocalRunSpawn {
        detail: format!("could not create {}: {error}", request.data_dir.display()),
    })?;

    let mut command = tokio::process::Command::new("bash");
    command
        .arg("-c")
        .arg(request.shell)
        .current_dir(request.data_dir)
        .env("ZORAI_PLUGIN_NAME", request.plugin_name)
        .env("ZORAI_PLUGIN_DIR", request.plugin_dir)
        .env("ZORAI_PLUGIN_DATA_DIR", request.data_dir)
        .envs(
            std::env::current_exe()
                .ok()
                .and_then(|exe| sibling_cli_path(&exe))
                .map(|cli| ("ZORAI_CLI", cli)),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command
        .spawn()
        .map_err(|error| PluginApiError::LocalRunSpawn {
            detail: error.to_string(),
        })?;
    let pid = child.id();

    if let Some(mut stdin) = child.stdin.take() {
        let payload = serde_json::to_vec(&request.input).unwrap_or_default();
        let _ = stdin.write_all(&payload).await;
        drop(stdin);
    }

    let timeout = std::time::Duration::from_secs(request.timeout_secs);
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(PluginApiError::LocalRunSpawn {
                detail: error.to_string(),
            })
        }
        Err(_) => {
            kill_process_group(pid);
            return Err(PluginApiError::LocalRunTimeout {
                secs: request.timeout_secs,
            });
        }
    };

    if !output.status.success() {
        return Err(PluginApiError::LocalRunFailed {
            code: output.status.code(),
            stderr: tail(&String::from_utf8_lossy(&output.stderr), STDERR_TAIL_CHARS),
        });
    }

    let stdout = &output.stdout[..output.stdout.len().min(MAX_STDOUT_BYTES)];
    Ok(String::from_utf8_lossy(stdout).trim_end().to_string())
}

fn kill_process_group(pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        let _ = std::process::Command::new("kill")
            .arg("-KILL")
            .arg("--")
            .arg(format!("-{pid}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(unix))]
    let _ = pid;
}

fn tail(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    let skipped: String = trimmed.chars().skip(count - max_chars).collect();
    format!("…{skipped}")
}

#[cfg(test)]
#[path = "local_run_tests.rs"]
mod tests;
