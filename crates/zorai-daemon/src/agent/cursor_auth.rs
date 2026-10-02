use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const STATUS_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) fn cursor_binary() -> Option<PathBuf> {
    which::which("agent")
        .ok()
        .or_else(|| which::which("cursor-agent").ok())
}

pub(crate) fn cursor_cli_available() -> bool {
    cursor_binary().is_some()
}

pub(crate) fn cursor_subscription_authenticated() -> bool {
    let Some(binary) = cursor_binary() else {
        return false;
    };
    let json = command_output_with_timeout(
        Command::new(&binary).args(["status", "--format", "json"]),
        STATUS_TIMEOUT,
    );
    if let Some(output) = json {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if cursor_status_output_is_authenticated(&stdout, output.status.success()) {
            return true;
        }
        if serde_json::from_str::<serde_json::Value>(stdout.trim()).is_ok() {
            return false;
        }
    }
    let Some(output) =
        command_output_with_timeout(Command::new(&binary).arg("status"), STATUS_TIMEOUT)
    else {
        return false;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    cursor_status_output_is_authenticated(&stdout, output.status.success())
}

pub(crate) fn begin_cursor_subscription_login() -> Result<()> {
    let binary = cursor_binary().context(
        "Cursor CLI (`agent`) was not found on PATH. Install the Cursor CLI, then log in.",
    )?;
    Command::new(&binary)
        .arg("login")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start '{} login'", binary.display()))?;
    Ok(())
}

pub(crate) fn logout_cursor_subscription() -> Result<()> {
    let binary = cursor_binary().context(
        "Cursor CLI (`agent`) was not found on PATH. Install the Cursor CLI, then log in.",
    )?;
    let output = command_output_with_timeout(Command::new(&binary).arg("logout"), STATUS_TIMEOUT)
        .context("Cursor CLI logout timed out")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("agent logout failed: {}", stderr.trim());
    }
    Ok(())
}

pub(crate) fn cursor_status_output_is_authenticated(stdout: &str, success: bool) -> bool {
    let trimmed = stdout.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(flag) = json_bool(&value, &["authenticated", "loggedIn", "logged_in"]) {
            return flag;
        }
        if json_nonempty_str(&value, &["email", "userEmail"]).is_some() {
            return true;
        }
        if let Some(status) = value.get("status").and_then(|field| field.as_str()) {
            return status_text_is_authenticated(status);
        }
        return false;
    }
    success && status_text_is_authenticated(trimmed)
}

fn json_bool(value: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|field| field.as_bool()))
}

fn json_nonempty_str(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|field| field.as_str())
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn status_text_is_authenticated(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("not authenticated")
        || lower.contains("not logged")
        || lower.contains("login required")
        || lower.contains("logged out")
    {
        return false;
    }
    lower.contains("logged in") || lower.contains("authenticated")
}

fn command_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Option<std::process::Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = std::io::Read::read_to_end(&mut out, &mut stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    let _ = std::io::Read::read_to_end(&mut err, &mut stderr);
                }
                return Some(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env_test_lock, EnvGuard};

    fn write_executable(path: &std::path::Path, contents: &str) {
        std::fs::write(path, contents).expect("write executable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path)
                .expect("stat executable")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).expect("chmod executable");
        }
    }

    #[test]
    fn status_json_authenticated_flag_wins_over_email() {
        assert!(cursor_status_output_is_authenticated(
            r#"{"authenticated":true,"email":"a@b.c"}"#,
            true
        ));
        assert!(!cursor_status_output_is_authenticated(
            r#"{"authenticated":false,"email":"a@b.c"}"#,
            true
        ));
        assert!(cursor_status_output_is_authenticated(
            r#"{"email":"a@b.c"}"#,
            true
        ));
        assert!(cursor_status_output_is_authenticated(
            "Logged in as a@b.c",
            true
        ));
        assert!(!cursor_status_output_is_authenticated(
            "Not logged in",
            true
        ));
    }

    #[test]
    fn logout_clears_subscription_status_reported_by_the_cli() {
        let _lock = env_test_lock();
        let _guard = EnvGuard::new(&["PATH"]);
        let root = tempfile::tempdir().expect("temp dir");
        let bin_dir = root.path().join("bin");
        std::fs::create_dir_all(&bin_dir).expect("bin");
        write_executable(
            &bin_dir.join("agent"),
            r#"#!/bin/sh
marker="$(dirname "$0")/logged-in"
if [ "$1" = "logout" ]; then
  rm -f "$marker"
  exit 0
fi
if [ "$1" = "status" ]; then
  if [ -f "$marker" ]; then
    printf '%s\n' '{"authenticated":true}'
    exit 0
  fi
  printf '%s\n' '{"authenticated":false}'
  exit 1
fi
exit 1
"#,
        );
        std::fs::write(bin_dir.join("logged-in"), "1").expect("auth marker");
        std::env::set_var("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()));

        assert!(
            cursor_subscription_authenticated(),
            "a logged-in Cursor CLI must count as the subscription provider"
        );
        logout_cursor_subscription().expect("logout");
        assert!(
            !cursor_subscription_authenticated(),
            "logout must drop the subscription auth state"
        );
    }
}
