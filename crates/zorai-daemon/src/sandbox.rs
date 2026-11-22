#![allow(dead_code)]

//! Workspace sandboxing for managed commands.
//! Uses bubblewrap (bwrap) on Linux, sandbox-exec on macOS,
//! or falls back to a passthrough when neither is available.

use std::path::{Path, PathBuf};

/// Result of sandbox wrapping a command.
pub struct SandboxedCommand {
    pub program: String,
    pub args: Vec<String>,
}

pub trait Sandbox: Send + Sync {
    fn wrap(&self, command: &str, workspace_root: &str, allow_network: bool) -> SandboxedCommand;
    fn name(&self) -> &'static str;
}

/// Linux: uses bubblewrap (bwrap) for mount namespace isolation.
pub struct BwrapSandbox;

impl Sandbox for BwrapSandbox {
    fn name(&self) -> &'static str {
        "bwrap"
    }

    fn wrap(&self, command: &str, workspace_root: &str, allow_network: bool) -> SandboxedCommand {
        let mut args = vec![
            "--die-with-parent".to_string(),
            "--ro-bind".to_string(),
            "/usr".to_string(),
            "/usr".to_string(),
            "--ro-bind".to_string(),
            "/lib".to_string(),
            "/lib".to_string(),
            "--ro-bind".to_string(),
            "/lib64".to_string(),
            "/lib64".to_string(),
            "--ro-bind".to_string(),
            "/bin".to_string(),
            "/bin".to_string(),
            "--ro-bind".to_string(),
            "/sbin".to_string(),
            "/sbin".to_string(),
            "--ro-bind".to_string(),
            "/etc".to_string(),
            "/etc".to_string(),
            "--proc".to_string(),
            "/proc".to_string(),
            "--dev".to_string(),
            "/dev".to_string(),
            "--tmpfs".to_string(),
            "/tmp".to_string(),
        ];
        for dir in toolchain_dirs(home_dir().as_deref()) {
            let path = dir.to_string_lossy().to_string();
            args.push("--ro-bind".to_string());
            args.push(path.clone());
            args.push(path);
        }
        args.extend([
            "--bind".to_string(),
            workspace_root.to_string(),
            workspace_root.to_string(),
            "--chdir".to_string(),
            workspace_root.to_string(),
        ]);

        if allow_network {
            // /etc is mounted, but systemd-resolved's resolv.conf is a symlink
            // into /run, which this sandbox does not otherwise expose.
            for source in host_resolver_bind_sources() {
                let path = source.to_string_lossy().to_string();
                args.push("--ro-bind".to_string());
                args.push(path.clone());
                args.push(path);
            }
        } else {
            args.push("--unshare-net".to_string());
        }

        args.push("--".to_string());
        args.push("bash".to_string());
        args.push("-c".to_string());
        args.push(command.to_string());

        SandboxedCommand {
            program: "bwrap".to_string(),
            args,
        }
    }
}

/// macOS: uses sandbox-exec with a generated profile.
pub struct SeatbeltSandbox;

impl Sandbox for SeatbeltSandbox {
    fn name(&self) -> &'static str {
        "seatbelt"
    }

    fn wrap(&self, command: &str, workspace_root: &str, allow_network: bool) -> SandboxedCommand {
        let network_rule = if allow_network {
            "(allow network*)"
        } else {
            "(deny network*)"
        };

        let toolchain_reads: String = toolchain_dirs(home_dir().as_deref())
            .iter()
            .map(|dir| format!(" (subpath \"{}\")", dir.display()))
            .collect();
        let profile = format!(
            r#"(version 1)
(deny default)
(allow process-exec)
(allow process-fork)
(allow file-read* (subpath "/usr") (subpath "/bin") (subpath "/sbin") (subpath "/Library") (subpath "/System") (subpath "/etc") (subpath "/dev") (subpath "/private"){toolchain_reads})
(allow file-read* file-write* (subpath "{workspace_root}"))
(allow file-read* file-write* (subpath "/tmp"))
(allow file-read* file-write* (subpath "/private/tmp"))
(allow sysctl-read)
(allow mach-lookup)
{network_rule}"#
        );

        SandboxedCommand {
            program: "sandbox-exec".to_string(),
            args: vec![
                "-p".to_string(),
                profile,
                "/bin/bash".to_string(),
                "-c".to_string(),
                command.to_string(),
            ],
        }
    }
}

/// No-op fallback when sandbox binaries are unavailable.
pub struct PassthroughSandbox;

impl Sandbox for PassthroughSandbox {
    fn name(&self) -> &'static str {
        "passthrough"
    }

    fn wrap(&self, command: &str, _workspace_root: &str, _allow_network: bool) -> SandboxedCommand {
        SandboxedCommand {
            program: "bash".to_string(),
            args: vec!["-c".to_string(), command.to_string()],
        }
    }
}

pub fn result_marker(sandbox_name: &str, allow_network: bool) -> Option<String> {
    let backend = match sandbox_name {
        "bwrap" => "bubblewrap",
        "seatbelt" => "macOS seatbelt",
        _ => return None,
    };
    let network = if allow_network {
        "network allowed"
    } else {
        "network blocked"
    };
    Some(format!("[sandboxed: {backend}, {network}]"))
}

/// Detect the best available sandbox for the current platform.
pub fn detect_sandbox() -> Box<dyn Sandbox> {
    #[cfg(target_os = "linux")]
    {
        if which_exists("bwrap") {
            tracing::info!("sandbox: using bubblewrap (bwrap)");
            return Box::new(BwrapSandbox);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if which_exists("sandbox-exec") {
            tracing::info!("sandbox: using macOS seatbelt (sandbox-exec)");
            return Box::new(SeatbeltSandbox);
        }
    }

    tracing::warn!(
        "sandbox: no sandbox binary found, using passthrough (commands run without isolation)"
    );
    Box::new(PassthroughSandbox)
}

const HOME_TOOLCHAIN_DIRS: &[&str] = &[
    ".cargo",
    ".rustup",
    ".nvm",
    ".pyenv",
    ".bun",
    ".deno",
    "go",
    "miniconda3",
    "anaconda3",
    ".local/bin",
    ".local/lib",
    ".local/share/pnpm",
];

const SYSTEM_TOOLCHAIN_DIRS: &[&str] = &["/opt", "/home/linuxbrew", "/nix"];

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn toolchain_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let home_dirs = home
        .into_iter()
        .flat_map(|home| HOME_TOOLCHAIN_DIRS.iter().map(move |dir| home.join(dir)));
    let system_dirs = SYSTEM_TOOLCHAIN_DIRS.iter().map(PathBuf::from);
    home_dirs
        .chain(system_dirs)
        .filter(|dir| dir.is_dir())
        .collect()
}

fn host_resolver_bind_sources() -> Vec<PathBuf> {
    let mut sources = Vec::new();
    for candidate in ["/run/systemd/resolve", "/run/resolvconf"] {
        let path = Path::new(candidate);
        if path.exists() {
            push_bind(&mut sources, path.to_path_buf());
        }
    }
    push_symlink_target(&mut sources, Path::new("/etc/resolv.conf"));
    push_symlink_target(&mut sources, Path::new("/etc/hosts"));
    sources
}

fn push_symlink_target(sources: &mut Vec<PathBuf>, link: &Path) {
    let Ok(target) = std::fs::read_link(link) else {
        return;
    };
    let resolved = if target.is_absolute() {
        target
    } else {
        link.parent().unwrap_or(Path::new("/")).join(target)
    };
    let Ok(canon) = std::fs::canonicalize(&resolved) else {
        return;
    };
    if let Some(bind) = bind_path_for_resolv_target(&canon) {
        push_bind(sources, bind);
    }
}

fn bind_path_for_resolv_target(canon: &Path) -> Option<PathBuf> {
    if already_visible_in_sandbox(canon) {
        return None;
    }
    let bind = match canon.parent() {
        Some(parent) if parent == Path::new("/run") || parent == Path::new("/") => {
            canon.to_path_buf()
        }
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => canon.to_path_buf(),
    };
    if bind.as_os_str().is_empty() || bind == Path::new("/") || already_visible_in_sandbox(&bind) {
        None
    } else {
        Some(bind)
    }
}

fn push_bind(sources: &mut Vec<PathBuf>, bind: PathBuf) {
    if sources.iter().any(|existing| bind.starts_with(existing)) {
        return;
    }
    sources.retain(|existing| !existing.starts_with(&bind));
    sources.push(bind);
}

fn already_visible_in_sandbox(path: &Path) -> bool {
    ["/etc", "/usr", "/lib", "/lib64", "/bin", "/sbin"]
        .iter()
        .any(|root| path.starts_with(root))
}

fn which_exists(binary: &str) -> bool {
    std::process::Command::new("which")
        .arg(binary)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolv_file_under_systemd_resolve_binds_that_directory() {
        let bind = bind_path_for_resolv_target(Path::new("/run/systemd/resolve/stub-resolv.conf"))
            .expect("stub resolver should be mounted");
        assert_eq!(bind, PathBuf::from("/run/systemd/resolve"));
    }

    #[test]
    fn resolv_file_directly_in_run_binds_only_the_file() {
        let bind = bind_path_for_resolv_target(Path::new("/run/resolv.conf"))
            .expect("standalone resolv.conf should be mounted");
        assert_eq!(bind, PathBuf::from("/run/resolv.conf"));
    }

    #[test]
    fn resolv_target_inside_etc_is_already_visible() {
        assert!(bind_path_for_resolv_target(Path::new("/etc/resolv.conf")).is_none());
    }

    #[test]
    fn broader_bind_replaces_a_narrower_one() {
        let mut sources = vec![PathBuf::from("/run/systemd/resolve/stub-resolv.conf")];
        push_bind(&mut sources, PathBuf::from("/run/systemd/resolve"));
        assert_eq!(sources, vec![PathBuf::from("/run/systemd/resolve")]);
    }

    #[test]
    fn networked_bwrap_mounts_resolver_sources_and_keeps_host_net() {
        let wrapped = BwrapSandbox.wrap("getent hosts example.com", "/tmp", true);
        assert!(wrapped.args.iter().all(|arg| arg != "--unshare-net"));
        for source in host_resolver_bind_sources() {
            let text = source.to_string_lossy().to_string();
            let index = wrapped
                .args
                .iter()
                .position(|arg| arg == &text)
                .unwrap_or_else(|| panic!("missing resolver bind {text}"));
            assert_eq!(wrapped.args[index - 1], "--ro-bind");
            assert_eq!(wrapped.args[index + 1], text);
        }
    }

    #[test]
    fn sandbox_exposes_installed_toolchains_read_only_but_nothing_else_from_home() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".cargo/bin")).unwrap();
        std::fs::create_dir_all(home.path().join(".local/bin")).unwrap();
        std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
        std::fs::create_dir_all(home.path().join(".local/share/keyrings")).unwrap();

        let dirs = toolchain_dirs(Some(home.path()));

        assert!(dirs.contains(&home.path().join(".cargo")));
        assert!(dirs.contains(&home.path().join(".local/bin")));
        assert!(!dirs.iter().any(|dir| dir.ends_with(".ssh")));
        assert!(!dirs
            .iter()
            .any(|dir| dir.starts_with(home.path().join(".local/share/keyrings"))));
        assert!(!dirs.contains(&home.path().join(".nvm")));
    }

    #[test]
    fn bwrap_runs_bash_with_toolchains_mounted_before_the_writable_workspace() {
        let wrapped = BwrapSandbox.wrap("[[ -n x ]] && echo bash", "/tmp/work", false);
        let tail = &wrapped.args[wrapped.args.len() - 3..];
        assert_eq!(tail, ["bash", "-c", "[[ -n x ]] && echo bash"]);
        let workspace_bind = wrapped
            .args
            .windows(3)
            .position(|w| w == ["--bind", "/tmp/work", "/tmp/work"])
            .expect("workspace should be writable");
        for dir in toolchain_dirs(home_dir().as_deref()) {
            let text = dir.to_string_lossy().to_string();
            let index = wrapped
                .args
                .windows(3)
                .position(|w| w[0] == "--ro-bind" && w[1] == text)
                .unwrap_or_else(|| panic!("missing read-only toolchain {text}"));
            assert!(index < workspace_bind);
        }
    }

    #[test]
    fn offline_bwrap_unshares_net_without_resolver_mounts() {
        let wrapped = BwrapSandbox.wrap("echo hi", "/tmp", false);
        assert!(wrapped.args.iter().any(|arg| arg == "--unshare-net"));
        for source in host_resolver_bind_sources() {
            let text = source.to_string_lossy().to_string();
            assert!(
                wrapped.args.iter().all(|arg| arg != &text),
                "offline sandbox mounted {text}"
            );
        }
    }
}
