use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use zorai_protocol::{
    parse_npm_latest_version, ZoraiUpdateStatus, ZORAI_NPM_LATEST_URL, ZORAI_NPM_PACKAGE,
};

use crate::commands::common::find_sibling_binary;

const DIRECT_INSTALL_MARKER: &str = ".zorai-install-source";
const INSTALL_SCRIPT_URL: &str =
    "https://raw.githubusercontent.com/mkurman/zorai/main/scripts/install.sh";
const INSTALL_POWERSHELL_URL: &str =
    "https://raw.githubusercontent.com/mkurman/zorai/main/scripts/install.ps1";
const PROCESS_STOP_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
enum InstallSource {
    Npm,
    DirectBinary { install_dir: PathBuf },
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpgradePlan {
    Npm,
    DirectInstaller {
        install_dir: PathBuf,
        command: String,
        args: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessTarget {
    Daemon,
    Gateway,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessCommandSpec {
    program: String,
    args: Vec<String>,
}

fn process_binary_name(target: ProcessTarget, windows: bool) -> &'static str {
    match (target, windows) {
        (ProcessTarget::Daemon, true) => "zorai-daemon.exe",
        (ProcessTarget::Gateway, true) => "zorai-gateway.exe",
        (ProcessTarget::Daemon, false) => "zorai-daemon",
        (ProcessTarget::Gateway, false) => "zorai-gateway",
    }
}

fn process_label(target: ProcessTarget) -> &'static str {
    match target {
        ProcessTarget::Daemon => "zorai-daemon",
        ProcessTarget::Gateway => "zorai-gateway",
    }
}

fn kill_command_spec(target: ProcessTarget, windows: bool) -> ProcessCommandSpec {
    let process_name = process_binary_name(target, windows);
    if windows {
        ProcessCommandSpec {
            program: "taskkill".to_string(),
            args: vec![
                "/IM".to_string(),
                process_name.to_string(),
                "/F".to_string(),
            ],
        }
    } else {
        ProcessCommandSpec {
            program: "pkill".to_string(),
            args: vec!["-x".to_string(), process_name.to_string()],
        }
    }
}

fn probe_command_spec(target: ProcessTarget, windows: bool) -> ProcessCommandSpec {
    let process_name = process_binary_name(target, windows);
    if windows {
        ProcessCommandSpec {
            program: "tasklist".to_string(),
            args: vec!["/FI".to_string(), format!("IMAGENAME eq {process_name}")],
        }
    } else {
        ProcessCommandSpec {
            program: "pgrep".to_string(),
            args: vec!["-x".to_string(), process_name.to_string()],
        }
    }
}

fn running_pids(target: ProcessTarget) -> Result<Vec<u32>> {
    let spec = probe_command_spec(target, false);
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .output()
        .with_context(|| format!("failed to probe {}", process_label(target)))?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter(|pid| !process_is_zombie(*pid))
        .collect())
}

fn pid_is_alive(pid: u32) -> bool {
    match Command::new("ps")
        .args(["-o", "state=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => {
            let state = String::from_utf8_lossy(&output.stdout);
            !state.trim().is_empty() && !state_is_zombie(&state)
        }
        Err(_) => false,
    }
}

fn wait_for_pids_exit(target: ProcessTarget, pids: &[u32], timeout: Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    while pids.iter().any(|pid| pid_is_alive(*pid)) {
        if std::time::Instant::now() >= deadline {
            bail!("timed out waiting for {} to exit", process_label(target));
        }
        std::thread::sleep(PROCESS_POLL_INTERVAL);
    }
    Ok(())
}

fn process_is_running(target: ProcessTarget) -> Result<bool> {
    let windows = cfg!(windows);
    let spec = probe_command_spec(target, windows);
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .output()
        .with_context(|| format!("failed to probe {}", process_label(target)))?;

    if windows {
        let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        Ok(stdout.contains(&process_binary_name(target, true).to_ascii_lowercase()))
    } else if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .filter_map(|line| line.trim().parse::<u32>().ok())
            .any(|pid| !process_is_zombie(pid)))
    } else if output.status.code() == Some(1) {
        Ok(false)
    } else {
        Err(anyhow!(
            "failed to probe {}: {}",
            process_label(target),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn state_is_zombie(state: &str) -> bool {
    state.trim().starts_with('Z')
}

fn process_is_zombie(pid: u32) -> bool {
    match Command::new("ps")
        .args(["-o", "state=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => state_is_zombie(&String::from_utf8_lossy(&output.stdout)),
        Err(_) => false,
    }
}

fn terminate_process(target: ProcessTarget) -> Result<()> {
    let spec = kill_command_spec(target, cfg!(windows));
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .output()
        .with_context(|| format!("failed to stop {}", process_label(target)))?;

    if output.status.success() || !process_is_running(target)? {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };

    bail!("failed to stop {}: {detail}", process_label(target))
}

fn wait_for_process_exit(target: ProcessTarget, timeout: Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !process_is_running(target)? {
            return Ok(());
        }
        std::thread::sleep(PROCESS_POLL_INTERVAL);
    }

    if process_is_running(target)? {
        bail!("timed out waiting for {} to exit", process_label(target));
    }

    Ok(())
}

fn stop_all_zorai_processes() -> Result<()> {
    for target in [ProcessTarget::Gateway, ProcessTarget::Daemon] {
        if cfg!(windows) {
            terminate_process(target)?;
            wait_for_process_exit(target, PROCESS_STOP_TIMEOUT)?;
        } else {
            let pids = running_pids(target)?;
            terminate_process(target)?;
            wait_for_pids_exit(target, &pids, PROCESS_STOP_TIMEOUT)?;
        }
    }
    Ok(())
}

fn start_daemon_process() -> Result<()> {
    let mut command = Command::new(find_sibling_binary("zorai-daemon"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    command
        .spawn()
        .context("failed to start zorai-daemon after restart")?;
    Ok(())
}

pub(crate) fn run_stop() -> Result<()> {
    stop_all_zorai_processes()?;
    println!("Stopped zorai daemon and gateway.");
    Ok(())
}

pub(crate) fn run_restart() -> Result<()> {
    stop_all_zorai_processes()?;
    start_daemon_process()?;
    println!("Restarted zorai daemon.");
    Ok(())
}

pub(crate) fn npm_command() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

fn direct_installer_command() -> &'static str {
    if cfg!(windows) {
        "powershell"
    } else {
        "sh"
    }
}

fn direct_installer_args() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "-ExecutionPolicy".to_string(),
            "ByPass".to_string(),
            "-Command".to_string(),
            format!("irm {INSTALL_POWERSHELL_URL} | iex"),
        ]
    } else {
        vec![
            "-c".to_string(),
            format!("curl -fsSL {INSTALL_SCRIPT_URL} | sh"),
        ]
    }
}

fn detect_install_source(install_source_env: Option<&str>, exe_path: &Path) -> InstallSource {
    if let Some(source) = install_source_env.map(str::trim) {
        if source.eq_ignore_ascii_case("npm") {
            return InstallSource::Npm;
        }

        if source.eq_ignore_ascii_case("direct") {
            return exe_path.parent().map_or_else(
                || InstallSource::Unknown {
                    reason: "direct install source did not provide a parent directory".to_string(),
                },
                |install_dir| InstallSource::DirectBinary {
                    install_dir: install_dir.to_path_buf(),
                },
            );
        }
    }

    if looks_like_npm_install(exe_path) {
        return InstallSource::Npm;
    }

    if looks_like_source_build(exe_path) {
        return InstallSource::Unknown {
            reason: format!(
                "{} looks like a source build output directory",
                exe_path.display()
            ),
        };
    }

    let Some(install_dir) = exe_path.parent() else {
        return InstallSource::Unknown {
            reason: format!("{} has no parent directory", exe_path.display()),
        };
    };

    if has_direct_install_marker(install_dir) || has_direct_install_layout(install_dir) {
        return InstallSource::DirectBinary {
            install_dir: install_dir.to_path_buf(),
        };
    }

    InstallSource::Unknown {
        reason: format!(
            "unable to determine install source for {}",
            exe_path.display()
        ),
    }
}

fn looks_like_npm_install(exe_path: &Path) -> bool {
    let lowered: Vec<String> = exe_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();

    lowered.windows(3).any(|window| {
        window[0] == "node_modules" && window[1] == ZORAI_NPM_PACKAGE && window[2] == "bin"
    })
}

fn looks_like_source_build(exe_path: &Path) -> bool {
    let lowered: Vec<String> = exe_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();

    lowered
        .windows(2)
        .any(|window| window[0] == "target" && (window[1] == "debug" || window[1] == "release"))
}

fn has_direct_install_marker(install_dir: &Path) -> bool {
    install_dir.join(DIRECT_INSTALL_MARKER).is_file()
}

fn has_direct_install_layout(install_dir: &Path) -> bool {
    install_dir.join(direct_sibling_binary_name()).is_file()
}

fn direct_sibling_binary_name() -> &'static str {
    if cfg!(windows) {
        "zorai-daemon.exe"
    } else {
        "zorai-daemon"
    }
}

fn build_upgrade_plan(source: InstallSource) -> Result<UpgradePlan> {
    match source {
        InstallSource::Npm => Ok(UpgradePlan::Npm),
        InstallSource::DirectBinary { install_dir } => Ok(UpgradePlan::DirectInstaller {
            install_dir,
            command: direct_installer_command().to_string(),
            args: direct_installer_args(),
        }),
        InstallSource::Unknown { reason } => bail!(
            "zorai upgrade only supports npm installs and direct installer installs ({reason}). If you built zorai from source, pull the latest code and rebuild."
        ),
    }
}

fn direct_upgrade_env(
    install_dir: &Path,
    wait_pid: u32,
    start_daemon_after_install: bool,
) -> Vec<(String, String)> {
    let mut envs = vec![
        (
            "ZORAI_INSTALL_DIR".to_string(),
            install_dir.display().to_string(),
        ),
        ("ZORAI_UPGRADE_WAIT_PID".to_string(), wait_pid.to_string()),
    ];

    if start_daemon_after_install {
        envs.push((
            "ZORAI_START_DAEMON_AFTER_INSTALL".to_string(),
            "1".to_string(),
        ));
    }

    envs
}

fn spawn_direct_upgrade(
    command_name: &str,
    args: &[String],
    install_dir: &Path,
    start_daemon_after_install: bool,
) -> Result<()> {
    let mut command = Command::new(command_name);
    command.args(args).stdin(Stdio::null());

    for (key, value) in direct_upgrade_env(install_dir, process::id(), start_daemon_after_install) {
        command.env(key, value);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    command.spawn().with_context(|| {
        format!(
            "failed to launch the direct installer via {command_name}; rerun the platform install script manually"
        )
    })?;

    Ok(())
}

fn run_npm_upgrade() -> Result<()> {
    let package = format!("{ZORAI_NPM_PACKAGE}@latest");
    let status = Command::new(npm_command())
        .args(["install", "-g"])
        .arg(&package)
        .status()
        .context("failed to launch npm; ensure Node.js and npm are installed and on PATH")?;

    if !status.success() {
        bail!("npm install -g {package} failed");
    }

    Ok(())
}

fn execute_upgrade_plan<Stop, Npm, Direct>(
    plan: UpgradePlan,
    stop_processes: Stop,
    run_npm_upgrade: Npm,
    spawn_direct_upgrade: Direct,
) -> Result<()>
where
    Stop: FnOnce() -> Result<()>,
    Npm: FnOnce() -> Result<()>,
    Direct: FnOnce(&str, &[String], &Path) -> Result<()>,
{
    stop_processes()?;

    match plan {
        UpgradePlan::Npm => {
            println!("Upgrading zorai via npm...");
            run_npm_upgrade()?;
            println!("Upgrade complete. zorai daemon and gateway are stopped.");
        }
        UpgradePlan::DirectInstaller {
            install_dir,
            command,
            args,
        } => {
            println!("Upgrading zorai via direct installer...");
            spawn_direct_upgrade(&command, &args, &install_dir)?;
            println!(
                "Upgrade started. zorai will refresh binaries in {} after this process exits. Daemon and gateway will remain stopped.",
                install_dir.display()
            );
        }
    }

    Ok(())
}

pub(crate) async fn fetch_update_status(current_version: &str) -> Result<ZoraiUpdateStatus> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(ZORAI_NPM_LATEST_URL)
        .header(
            reqwest::header::USER_AGENT,
            format!("zorai-cli/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .context("failed to query npm registry")?
        .error_for_status()
        .context("npm registry returned an error status")?;

    let body = response
        .text()
        .await
        .context("failed to read npm response")?;
    let latest_version = parse_npm_latest_version(&body)
        .ok_or_else(|| anyhow!("npm registry response did not include a valid version"))?;

    ZoraiUpdateStatus::from_versions(current_version, &latest_version)
        .ok_or_else(|| anyhow!("failed to compare current version against npm @latest"))
}

pub(crate) async fn print_upgrade_notice_if_available(current_version: &str) {
    if std::env::var_os("ZORAI_DISABLE_UPDATE_CHECK").is_some() {
        tracing::debug!("skipping update notice because ZORAI_DISABLE_UPDATE_CHECK is set");
        return;
    }

    let current_version = current_version.to_string();
    tokio::spawn(async move {
        match fetch_update_status(&current_version).await {
            Ok(status) => {
                if let Some(notice) = status.cli_notice() {
                    eprintln!("{notice}");
                }
            }
            Err(error) => {
                tracing::debug!(%error, "skipping update notice after npm lookup failure");
            }
        }
    });
}

fn current_upgrade_plan() -> Result<UpgradePlan> {
    let install_source_env = std::env::var("ZORAI_INSTALL_SOURCE").ok();
    let current_exe = std::env::current_exe().context("failed to resolve current zorai path")?;
    build_upgrade_plan(detect_install_source(
        install_source_env.as_deref(),
        &current_exe,
    ))
}

fn run_direct_upgrade(command_name: &str, args: &[String], install_dir: &Path) -> Result<()> {
    let mut command = Command::new(command_name);
    command
        .args(args)
        .stdin(Stdio::null())
        .env("ZORAI_INSTALL_DIR", install_dir.display().to_string());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    let status = command.status().with_context(|| {
        format!(
            "failed to launch the direct installer via {command_name}; rerun the platform install script manually"
        )
    })?;

    if !status.success() {
        bail!("direct installer failed ({status})");
    }

    Ok(())
}

pub(crate) fn run_upgrade_keep_running() -> Result<()> {
    match current_upgrade_plan()? {
        UpgradePlan::Npm => {
            println!("Upgrading zorai via npm...");
            run_npm_upgrade()?;
        }
        UpgradePlan::DirectInstaller {
            install_dir,
            command,
            args,
        } => {
            println!("Upgrading zorai via direct installer...");
            run_direct_upgrade(&command, &args, &install_dir)?;
        }
    }
    println!("Upgrade installed. Restart zorai to use the new version.");
    Ok(())
}

pub(crate) fn run_upgrade() -> Result<()> {
    let plan = current_upgrade_plan()?;

    execute_upgrade_plan(
        plan,
        stop_all_zorai_processes,
        run_npm_upgrade,
        |command, args, install_dir| spawn_direct_upgrade(command, args, install_dir, false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn detects_npm_install_source_from_wrapper_env() {
        let source = detect_install_source(Some("npm"), Path::new("/tmp/zorai"));

        assert!(matches!(source, InstallSource::Npm));
    }

    #[test]
    fn detects_npm_install_source_from_node_modules_path() {
        let source = detect_install_source(
            None,
            Path::new("/usr/local/lib/node_modules/zor-ai/bin/zorai"),
        );

        assert!(matches!(source, InstallSource::Npm));
    }

    #[test]
    fn detects_direct_binary_install_from_sibling_layout() {
        let root = tempdir().expect("tempdir");
        std::fs::write(root.path().join("zorai"), b"binary").expect("write zorai");
        std::fs::write(root.path().join("zorai-daemon"), b"binary").expect("write daemon");

        let source = detect_install_source(None, &root.path().join("zorai"));

        assert_eq!(
            source,
            InstallSource::DirectBinary {
                install_dir: root.path().to_path_buf(),
            }
        );
    }

    #[test]
    fn does_not_treat_source_build_output_as_direct_install() {
        let source = detect_install_source(None, Path::new("/repo/target/release/zorai"));

        assert!(matches!(source, InstallSource::Unknown { .. }));
    }

    #[test]
    fn builds_npm_upgrade_plan_for_npm_install() {
        let plan = build_upgrade_plan(InstallSource::Npm).expect("plan");

        assert!(matches!(plan, UpgradePlan::Npm));
    }

    #[test]
    fn builds_direct_upgrade_plan_for_direct_install() {
        let plan = build_upgrade_plan(InstallSource::DirectBinary {
            install_dir: Path::new("/home/test/.local/bin").to_path_buf(),
        })
        .expect("plan");

        assert!(matches!(
            plan,
            UpgradePlan::DirectInstaller { install_dir, .. }
            if install_dir == Path::new("/home/test/.local/bin")
        ));
    }

    #[test]
    fn unix_process_control_uses_exact_process_names() {
        let daemon_kill = kill_command_spec(ProcessTarget::Daemon, false);
        let gateway_kill = kill_command_spec(ProcessTarget::Gateway, false);
        let daemon_probe = probe_command_spec(ProcessTarget::Daemon, false);
        let gateway_probe = probe_command_spec(ProcessTarget::Gateway, false);

        assert_eq!(daemon_kill.program, "pkill");
        assert_eq!(daemon_kill.args, vec!["-x", "zorai-daemon"]);
        assert_eq!(gateway_kill.program, "pkill");
        assert_eq!(gateway_kill.args, vec!["-x", "zorai-gateway"]);

        assert_eq!(daemon_probe.program, "pgrep");
        assert_eq!(daemon_probe.args, vec!["-x", "zorai-daemon"]);
        assert_eq!(gateway_probe.program, "pgrep");
        assert_eq!(gateway_probe.args, vec!["-x", "zorai-gateway"]);
    }

    #[cfg(unix)]
    #[test]
    fn stop_waits_only_for_the_processes_it_killed_not_a_replacement_started_meanwhile() {
        let mut stopped = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let mut replacement = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let stopped_pid = stopped.id();
        stopped.kill().expect("kill sleep");
        stopped.wait().expect("reap sleep");

        wait_for_pids_exit(
            ProcessTarget::Daemon,
            &[stopped_pid],
            Duration::from_millis(500),
        )
        .expect("an exited process must not keep stop waiting");
        let err = wait_for_pids_exit(
            ProcessTarget::Daemon,
            &[replacement.id()],
            Duration::from_millis(300),
        )
        .expect_err("a process that is still alive must time out");
        assert!(err
            .to_string()
            .contains("timed out waiting for zorai-daemon"));

        replacement.kill().expect("kill replacement");
        replacement.wait().expect("reap replacement");
    }

    #[test]
    fn zombie_states_are_not_treated_as_running() {
        assert!(state_is_zombie("Z"));
        assert!(state_is_zombie("Z+"));
        assert!(state_is_zombie("Z+\n"));
        assert!(!state_is_zombie("S"));
        assert!(!state_is_zombie("Ssl"));
        assert!(!state_is_zombie("R+"));
    }

    #[test]
    fn windows_process_control_uses_exact_image_names() {
        let daemon_kill = kill_command_spec(ProcessTarget::Daemon, true);
        let gateway_kill = kill_command_spec(ProcessTarget::Gateway, true);
        let daemon_probe = probe_command_spec(ProcessTarget::Daemon, true);
        let gateway_probe = probe_command_spec(ProcessTarget::Gateway, true);

        assert_eq!(daemon_kill.program, "taskkill");
        assert_eq!(daemon_kill.args, vec!["/IM", "zorai-daemon.exe", "/F"]);
        assert_eq!(gateway_kill.program, "taskkill");
        assert_eq!(gateway_kill.args, vec!["/IM", "zorai-gateway.exe", "/F"]);

        assert_eq!(daemon_probe.program, "tasklist");
        assert_eq!(
            daemon_probe.args,
            vec!["/FI", "IMAGENAME eq zorai-daemon.exe"]
        );
        assert_eq!(gateway_probe.program, "tasklist");
        assert_eq!(
            gateway_probe.args,
            vec!["/FI", "IMAGENAME eq zorai-gateway.exe"]
        );
    }

    #[test]
    fn npm_upgrade_stops_processes_before_install_and_leaves_daemon_stopped() {
        let events = RefCell::new(Vec::new());

        execute_upgrade_plan(
            UpgradePlan::Npm,
            || {
                events.borrow_mut().push("stop");
                Ok(())
            },
            || {
                events.borrow_mut().push("npm");
                Ok(())
            },
            |_, _, _| {
                events.borrow_mut().push("direct");
                Ok(())
            },
        )
        .expect("upgrade should succeed");

        assert_eq!(*events.borrow(), vec!["stop", "npm"]);
    }

    #[test]
    fn direct_upgrade_stops_processes_before_spawning_installer() {
        let events = RefCell::new(Vec::new());

        execute_upgrade_plan(
            UpgradePlan::DirectInstaller {
                install_dir: Path::new("/tmp/zorai").to_path_buf(),
                command: "sh".to_string(),
                args: vec!["-c".to_string(), "echo upgrade".to_string()],
            },
            || {
                events.borrow_mut().push("stop");
                Ok(())
            },
            || {
                events.borrow_mut().push("npm");
                Ok(())
            },
            |command, args, install_dir| {
                events.borrow_mut().push("direct");
                assert_eq!(command, "sh");
                assert_eq!(args, vec!["-c".to_string(), "echo upgrade".to_string()]);
                assert_eq!(install_dir, Path::new("/tmp/zorai"));
                Ok(())
            },
        )
        .expect("upgrade should succeed");

        assert_eq!(*events.borrow(), vec!["stop", "direct"]);
    }

    #[test]
    fn direct_upgrade_env_omits_daemon_restart_after_install() {
        let envs = direct_upgrade_env(Path::new("/opt/zorai"), 4242, false);

        assert_eq!(
            envs,
            vec![
                ("ZORAI_INSTALL_DIR".to_string(), "/opt/zorai".to_string()),
                ("ZORAI_UPGRADE_WAIT_PID".to_string(), "4242".to_string()),
            ]
        );
    }
}
