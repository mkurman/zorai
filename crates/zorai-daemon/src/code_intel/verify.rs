use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

const CHECK_TIMEOUT: Duration = Duration::from_secs(600);
const OUTPUT_TAIL_CHARS: usize = 4000;

#[derive(Debug, Clone, Default)]
pub(crate) struct VerifyBaseline {
    pub dirty: BTreeMap<String, u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct VerifyTurnState {
    pub root: PathBuf,
    pub baseline: VerifyBaseline,
    pub attempts: u8,
    pub max_retries: u8,
    pub run_tests: bool,
    pub browser_check: bool,
}

pub(crate) const BROWSER_OVERLAY_SCRIPT: &str = "(() => { const overlay = document.querySelector('vite-error-overlay, nextjs-portal, #webpack-dev-server-client-overlay'); const root = overlay && (overlay.shadowRoot || overlay); const text = root ? (root.textContent || '').replace(/\\s+/g, ' ').trim().slice(0, 3000) : ''; return JSON.stringify({ url: location.href, overlay: text }); })()";

pub(crate) fn touches_ui(changed: &[String]) -> bool {
    changed.iter().any(|file| matches!(file.rsplit('.').next().unwrap_or(""), "tsx" | "jsx" | "css" | "scss" | "less" | "html" | "vue" | "svelte"))
}

pub(crate) fn local_page(tabs: &str) -> bool {
    tabs.lines().any(|line| line.contains("localhost") || line.contains("127.0.0.1") || line.contains("[::1]"))
}

pub(crate) fn overlay_outcome(eval_output: &str, screenshot: Option<String>) -> VerifyOutcome {
    let parsed: serde_json::Value = serde_json::from_str(eval_output).unwrap_or_default();
    let overlay = parsed.get("overlay").and_then(|value| value.as_str()).unwrap_or("").to_string();
    let url = parsed.get("url").and_then(|value| value.as_str()).unwrap_or("the open page");
    let shot = screenshot.map(|path| format!(" Screenshot: {path}")).unwrap_or_default();
    if overlay.is_empty() {
        VerifyOutcome { label: "browser".into(), ok: true, output: format!("{url} reloaded without a dev-server error overlay.{shot}") }
    } else {
        VerifyOutcome { label: "browser".into(), ok: false, output: format!("{url} shows a dev-server error overlay: {overlay}{shot}") }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifyCheck {
    pub label: String,
    pub cwd: PathBuf,
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct VerifyOutcome {
    pub label: String,
    pub ok: bool,
    pub output: String,
}

fn content_hash(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::fs::read(path).unwrap_or_default().hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn snapshot(root: &Path) -> VerifyBaseline {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .current_dir(root)
        .output();
    let mut dirty = BTreeMap::new();
    if let Ok(output) = output {
        for entry in String::from_utf8_lossy(&output.stdout).split('\0') {
            if entry.len() > 3 {
                let path = entry[3..].to_string();
                dirty.insert(path.clone(), content_hash(&root.join(&path)));
            }
        }
    }
    VerifyBaseline { dirty }
}

pub(crate) fn changed_against(root: &Path, base: &str) -> Vec<String> {
    let mut files = BTreeSet::new();
    let range = if base.trim().is_empty() { "HEAD".to_string() } else { format!("{}...HEAD", base.trim()) };
    for args in [vec!["diff", "--name-only", range.as_str()], vec!["diff", "--name-only", "HEAD"], vec!["ls-files", "--others", "--exclude-standard"]] {
        if let Ok(output) = std::process::Command::new("git").args(&args).current_dir(root).output() {
            files.extend(String::from_utf8_lossy(&output.stdout).lines().filter(|line| !line.is_empty()).map(str::to_string));
        }
    }
    files.into_iter().collect()
}

pub(crate) fn changed_since(before: &VerifyBaseline, after: &VerifyBaseline) -> Vec<String> {
    after
        .dirty
        .iter()
        .filter(|(path, hash)| before.dirty.get(*path) != Some(*hash))
        .map(|(path, _)| path.clone())
        .chain(
            before
                .dirty
                .keys()
                .filter(|path| !after.dirty.contains_key(*path))
                .cloned(),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn nearest_with(root: &Path, file: &str, marker: &str) -> Option<PathBuf> {
    let mut directory = root.join(file).parent()?.to_path_buf();
    loop {
        if directory.join(marker).exists() {
            return Some(directory);
        }
        if directory == root || !directory.pop() || !directory.starts_with(root) {
            return None;
        }
    }
}

fn relative_to(base: &Path, root: &Path, file: &str) -> String {
    root.join(file)
        .strip_prefix(base)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| file.to_string())
}

pub(crate) fn plan_checks(root: &Path, changed: &[String], run_tests: bool) -> Vec<VerifyCheck> {
    let mut checks: Vec<VerifyCheck> = Vec::new();
    let mut add = |check: VerifyCheck| {
        if !checks.contains(&check) {
            checks.push(check);
        }
    };
    let extension = |file: &str| file.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    for file in changed.iter().filter(|file| root.join(file).exists()) {
        match extension(file).as_str() {
            "rs" => {
                if let Some(crate_dir) = nearest_with(root, file, "Cargo.toml") {
                    add(VerifyCheck {
                        label: "cargo check".into(),
                        cwd: crate_dir.clone(),
                        program: "cargo".into(),
                        args: vec![
                            "check".into(),
                            "--quiet".into(),
                            "--message-format=short".into(),
                        ],
                    });
                    if run_tests {
                        add(VerifyCheck {
                            label: "cargo test".into(),
                            cwd: crate_dir,
                            program: "cargo".into(),
                            args: vec!["test".into(), "--quiet".into()],
                        });
                    }
                }
            }
            "ts" | "tsx" | "mts" | "cts" => {
                if let Some(project) = nearest_with(root, file, "tsconfig.json") {
                    add(VerifyCheck {
                        label: "tsc".into(),
                        cwd: project,
                        program: "npx".into(),
                        args: vec![
                            "--no-install".into(),
                            "tsc".into(),
                            "--noEmit".into(),
                            "-p".into(),
                            ".".into(),
                        ],
                    });
                }
            }
            "py" => add(VerifyCheck {
                label: format!("python compile {file}"),
                cwd: root.to_path_buf(),
                program: "python3".into(),
                args: vec!["-m".into(), "py_compile".into(), file.clone()],
            }),
            "go" => {
                if let Some(module) = nearest_with(root, file, "go.mod") {
                    add(VerifyCheck {
                        label: "go vet".into(),
                        cwd: module,
                        program: "go".into(),
                        args: vec!["vet".into(), "./...".into()],
                    });
                }
            }
            _ => {}
        }
    }
    if run_tests {
        let scripts: Vec<&String> = changed
            .iter()
            .filter(|file| matches!(extension(file).as_str(), "ts" | "tsx" | "js" | "jsx"))
            .collect();
        let mut by_package: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
        for file in scripts {
            if let Some(package) = nearest_with(root, file, "package.json") {
                if package.join("node_modules/vitest").exists() {
                    by_package
                        .entry(package.clone())
                        .or_default()
                        .push(relative_to(&package, root, file));
                }
            }
        }
        for (package, files) in by_package {
            let mut args = vec![
                "--no-install".into(),
                "vitest".into(),
                "related".into(),
                "--run".into(),
            ];
            args.extend(files);
            add(VerifyCheck {
                label: "vitest related".into(),
                cwd: package,
                program: "npx".into(),
                args,
            });
        }
    }
    checks
}

fn tail(text: &str) -> String {
    let count = text.chars().count();
    if count <= OUTPUT_TAIL_CHARS {
        return text.to_string();
    }
    format!(
        "…{}",
        text.chars()
            .skip(count - OUTPUT_TAIL_CHARS)
            .collect::<String>()
    )
}

pub(crate) async fn run_checks(checks: &[VerifyCheck]) -> Vec<VerifyOutcome> {
    let mut outcomes = Vec::new();
    for check in checks {
        let child = tokio::process::Command::new(&check.program)
            .args(&check.args)
            .current_dir(&check.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output();
        let outcome = match tokio::time::timeout(CHECK_TIMEOUT, child).await {
            Ok(Ok(output)) => VerifyOutcome {
                label: check.label.clone(),
                ok: output.status.success(),
                output: tail(&format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )),
            },
            Ok(Err(error)) => VerifyOutcome {
                label: check.label.clone(),
                ok: true,
                output: format!("skipped: {error}"),
            },
            Err(_) => VerifyOutcome {
                label: check.label.clone(),
                ok: false,
                output: format!("timed out after {} s", CHECK_TIMEOUT.as_secs()),
            },
        };
        outcomes.push(outcome);
    }
    outcomes
}

pub(crate) fn failure_prompt(
    outcomes: &[VerifyOutcome],
    attempt: u8,
    max_retries: u8,
) -> Option<String> {
    let failures: Vec<&VerifyOutcome> = outcomes.iter().filter(|outcome| !outcome.ok).collect();
    if failures.is_empty() {
        return None;
    }
    let details = failures
        .iter()
        .map(|outcome| format!("### {}\n```\n{}\n```", outcome.label, outcome.output.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(format!(
        "Automatic verification failed after your edits (fix attempt {attempt} of {max_retries}). Fix these problems in the files you changed, then finish your answer. Do not weaken or delete tests to make them pass.\n\n{details}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_check_only_runs_for_ui_files_on_a_local_dev_page_and_fails_on_an_error_overlay() {
        assert!(touches_ui(&["src/App.tsx".into()]) && !touches_ui(&["src/lib.rs".into()]));
        assert!(local_page("* tab-1 http://localhost:5173/ — App") && !local_page("* tab-1 https://example.com/"));
        let broken = overlay_outcome(r#"{"url":"http://localhost:5173/","overlay":"[plugin:vite:react] Unexpected token"}"#, Some("/tmp/s.png".into()));
        assert!(!broken.ok && broken.output.contains("Unexpected token") && broken.output.contains("/tmp/s.png"));
        assert!(overlay_outcome(r#"{"url":"http://localhost:5173/","overlay":""}"#, None).ok);
    }

    #[test]
    fn only_files_the_turn_touched_count_as_changes() {
        let before = VerifyBaseline {
            dirty: BTreeMap::from([
                ("a.rs".into(), 1),
                ("b.rs".into(), 2),
                ("gone.rs".into(), 3),
            ]),
        };
        let after = VerifyBaseline {
            dirty: BTreeMap::from([("a.rs".into(), 1), ("b.rs".into(), 9), ("new.ts".into(), 4)]),
        };
        assert_eq!(
            changed_since(&before, &after),
            vec!["b.rs", "gone.rs", "new.ts"]
        );
    }

    #[test]
    fn checks_follow_the_languages_that_changed_and_their_nearest_project() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("crates/core/src")).unwrap();
        std::fs::write(root.path().join("crates/core/Cargo.toml"), "[package]").unwrap();
        std::fs::write(root.path().join("crates/core/src/lib.rs"), "").unwrap();
        std::fs::create_dir_all(root.path().join("web/src")).unwrap();
        std::fs::write(root.path().join("web/tsconfig.json"), "{}").unwrap();
        std::fs::write(root.path().join("web/package.json"), "{}").unwrap();
        std::fs::create_dir_all(root.path().join("web/node_modules/vitest")).unwrap();
        std::fs::write(root.path().join("web/src/a.ts"), "").unwrap();
        std::fs::write(root.path().join("README.md"), "").unwrap();
        let changed = vec![
            "crates/core/src/lib.rs".to_string(),
            "web/src/a.ts".to_string(),
            "README.md".to_string(),
        ];
        let labels: Vec<_> = plan_checks(root.path(), &changed, true)
            .into_iter()
            .map(|check| {
                (
                    check.label,
                    check
                        .cwd
                        .strip_prefix(root.path())
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    check.args.last().cloned().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            labels,
            vec![
                (
                    "cargo check".to_string(),
                    "crates/core".to_string(),
                    "--message-format=short".to_string()
                ),
                (
                    "cargo test".to_string(),
                    "crates/core".to_string(),
                    "--quiet".to_string()
                ),
                ("tsc".to_string(), "web".to_string(), ".".to_string()),
                (
                    "vitest related".to_string(),
                    "web".to_string(),
                    "src/a.ts".to_string()
                ),
            ]
        );
        assert!(plan_checks(root.path(), &changed, false)
            .iter()
            .all(|check| !check.label.contains("test") && !check.label.contains("vitest")));
    }

    #[test]
    fn failures_become_a_bounded_fix_request_and_passing_runs_stay_silent() {
        let failed = vec![
            VerifyOutcome {
                label: "tsc".into(),
                ok: false,
                output: "a.ts(1,1): error TS2322".into(),
            },
            VerifyOutcome {
                label: "cargo check".into(),
                ok: true,
                output: String::new(),
            },
        ];
        let prompt = failure_prompt(&failed, 1, 2).unwrap();
        assert!(
            prompt.contains("fix attempt 1 of 2")
                && prompt.contains("### tsc")
                && !prompt.contains("cargo check")
        );
        assert!(failure_prompt(&failed[1..], 1, 2).is_none());
        assert_eq!(
            tail(&"x".repeat(OUTPUT_TAIL_CHARS + 10)).chars().count(),
            OUTPUT_TAIL_CHARS + 1
        );
    }

    #[tokio::test]
    async fn running_a_check_reports_its_exit_status_and_output() {
        let outcomes = run_checks(&[
            VerifyCheck {
                label: "ok".into(),
                cwd: std::env::temp_dir(),
                program: "sh".into(),
                args: vec!["-c".into(), "echo fine".into()],
            },
            VerifyCheck {
                label: "bad".into(),
                cwd: std::env::temp_dir(),
                program: "sh".into(),
                args: vec!["-c".into(), "echo broken >&2; exit 2".into()],
            },
        ])
        .await;
        assert!(outcomes[0].ok && outcomes[0].output.contains("fine"));
        assert!(!outcomes[1].ok && outcomes[1].output.contains("broken"));
    }
}
