use anyhow::{Context, Result};
use futures::{StreamExt, TryStreamExt};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const GITHUB_SKILLS_TREE_URL: &str =
    "https://api.github.com/repos/mkurman/zorai/git/trees/main?recursive=1";
const GITHUB_RAW_MAIN_URL: &str = "https://raw.githubusercontent.com/mkurman/zorai/main";
const SKILL_DOWNLOAD_CONCURRENCY: usize = 8;
const SKILL_DOWNLOAD_ATTEMPTS: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteSkillDocument {
    pub(super) relative_path: String,
    pub(super) content: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SkillSyncSummary {
    pub(super) installed: usize,
    pub(super) skipped_existing: usize,
    pub(super) skipped_locked: usize,
    pub(super) overwritten: usize,
}

#[derive(Debug, Deserialize)]
struct GitTreeResponse {
    tree: Vec<GitTreeItem>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct GitTreeItem {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

pub(super) async fn fetch_remote_skill_documents() -> Result<Vec<RemoteSkillDocument>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to create skill sync HTTP client")?;
    let user_agent = format!("zorai-cli/{}", env!("CARGO_PKG_VERSION"));

    let response = client
        .get(GITHUB_SKILLS_TREE_URL)
        .header(reqwest::header::USER_AGENT, &user_agent)
        .send()
        .await
        .context("failed to query zorai skill tree")?
        .error_for_status()
        .context("zorai skill tree request returned an error status")?;

    let tree = response
        .json::<GitTreeResponse>()
        .await
        .context("failed to parse zorai skill tree response")?;
    if tree.truncated {
        anyhow::bail!("zorai skill tree response was truncated by GitHub");
    }

    let mut paths = tree
        .tree
        .into_iter()
        .filter(|item| item.kind == "blob")
        .filter_map(|item| {
            let relative = item.path.strip_prefix("skills/")?;
            if relative.trim().is_empty() {
                return None;
            }
            let relative_path = relative.to_string();
            Some((item.path, relative_path))
        })
        .collect::<Vec<_>>();
    paths.sort_by(|(_, left), (_, right)| left.cmp(right));

    let documents = futures::stream::iter(paths.into_iter().map(|(repo_path, relative_path)| {
        let client = &client;
        let user_agent = &user_agent;
        async move {
            let content = download_skill_file(client, user_agent, &repo_path).await?;
            Ok::<_, anyhow::Error>(RemoteSkillDocument {
                relative_path,
                content,
            })
        }
    }))
    .buffered(SKILL_DOWNLOAD_CONCURRENCY)
    .try_collect::<Vec<_>>()
    .await?;

    if documents.is_empty() {
        anyhow::bail!("no skill files were found in the zorai repository");
    }
    Ok(documents)
}

async fn download_skill_file(
    client: &reqwest::Client,
    user_agent: &str,
    repo_path: &str,
) -> Result<Vec<u8>> {
    let raw_url = format!("{GITHUB_RAW_MAIN_URL}/{}", percent_encode_path(repo_path));
    let mut attempt = 1;
    loop {
        let result = client
            .get(&raw_url)
            .header(reqwest::header::USER_AGENT, user_agent)
            .send()
            .await;
        let retryable = match &result {
            Ok(response) => {
                response.status().is_server_error()
                    || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
            }
            Err(error) => error.is_timeout() || error.is_connect() || error.is_request(),
        };
        if retryable && attempt < SKILL_DOWNLOAD_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
            attempt += 1;
            continue;
        }
        return Ok(result
            .with_context(|| format!("failed to download skill file {repo_path}"))?
            .error_for_status()
            .with_context(|| format!("skill file download returned an error status: {repo_path}"))?
            .bytes()
            .await
            .with_context(|| format!("failed to read skill file response: {repo_path}"))?
            .to_vec());
    }
}

pub(super) fn locked_skill_keys(config: &serde_json::Value) -> Vec<String> {
    config
        .pointer("/skill_library/locked")
        .and_then(|value| value.as_array())
        .map(|keys| {
            keys.iter()
                .filter_map(|key| key.as_str())
                .filter_map(normalize_skill_key)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_skill_key(raw: &str) -> Option<String> {
    let normalized = raw.trim().replace('\\', "/");
    let parts = normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| *part == "..") {
        return None;
    }
    Some(parts.join("/"))
}

fn is_locked_skill_path(locked: &[String], relative_path: &str) -> bool {
    locked.iter().any(|key| {
        relative_path == key
            || relative_path
                .strip_prefix(key.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

pub(super) fn sync_skill_documents(
    skills_root: &Path,
    documents: &[RemoteSkillDocument],
    force: bool,
    locked: &[String],
) -> Result<SkillSyncSummary> {
    std::fs::create_dir_all(skills_root).with_context(|| {
        format!(
            "failed to create skills directory {}",
            skills_root.display()
        )
    })?;

    let mut summary = SkillSyncSummary {
        installed: 0,
        skipped_existing: 0,
        skipped_locked: 0,
        overwritten: 0,
    };

    for document in documents {
        let relative = validate_remote_skill_path(&document.relative_path)?;
        if is_locked_skill_path(locked, document.relative_path.trim()) {
            summary.skipped_locked += 1;
            continue;
        }
        let destination = skills_root.join(relative);
        let exists = destination.exists();
        if exists && !force {
            summary.skipped_existing += 1;
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create skill directory {}", parent.display())
            })?;
        }
        std::fs::write(&destination, &document.content)
            .with_context(|| format!("failed to write skill file {}", destination.display()))?;
        if exists {
            summary.overwritten += 1;
        } else {
            summary.installed += 1;
        }
    }

    Ok(summary)
}

fn validate_remote_skill_path(value: &str) -> Result<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("remote skill path must not be empty");
    }
    if trimmed.contains('\\') {
        anyhow::bail!("remote skill path must use forward slashes: {trimmed}");
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        anyhow::bail!("remote skill path must be relative: {trimmed}");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => anyhow::bail!("remote skill path must not escape skills root: {trimmed}"),
        }
    }

    Ok(path.to_path_buf())
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_skills_skips_existing_files_without_force() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("skills");
        std::fs::create_dir_all(root.join("development/debug")).expect("skills dir");
        std::fs::write(root.join("development/debug/SKILL.md"), "local").expect("write local");

        let documents = vec![
            RemoteSkillDocument {
                relative_path: "development/debug/SKILL.md".to_string(),
                content: b"remote".to_vec(),
            },
            RemoteSkillDocument {
                relative_path: "development/debug/references/checklist.md".to_string(),
                content: b"checklist".to_vec(),
            },
        ];

        let summary = sync_skill_documents(&root, &documents, false, &[]).expect("sync");

        assert_eq!(summary.installed, 1);
        assert_eq!(summary.overwritten, 0);
        assert_eq!(summary.skipped_existing, 1);
        assert_eq!(
            std::fs::read_to_string(root.join("development/debug/SKILL.md")).expect("read"),
            "local"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("development/debug/references/checklist.md"))
                .expect("read"),
            "checklist"
        );
    }

    #[test]
    fn sync_skills_overwrites_existing_files_with_force() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("skills");
        std::fs::create_dir_all(root.join("development/debug")).expect("skills dir");
        std::fs::write(root.join("development/debug/SKILL.md"), "local").expect("write local");

        let documents = vec![RemoteSkillDocument {
            relative_path: "development/debug/SKILL.md".to_string(),
            content: b"remote".to_vec(),
        }];

        let summary = sync_skill_documents(&root, &documents, true, &[]).expect("sync");

        assert_eq!(summary.installed, 0);
        assert_eq!(summary.overwritten, 1);
        assert_eq!(summary.skipped_existing, 0);
        assert_eq!(
            std::fs::read_to_string(root.join("development/debug/SKILL.md")).expect("read"),
            "remote"
        );
    }

    #[test]
    fn force_sync_never_replaces_or_adds_files_inside_a_locked_skill() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("skills");
        std::fs::create_dir_all(root.join("development/debug")).expect("skills dir");
        std::fs::write(root.join("development/debug/SKILL.md"), "local").expect("write local");

        let documents = vec![
            RemoteSkillDocument {
                relative_path: "development/debug/SKILL.md".to_string(),
                content: b"remote".to_vec(),
            },
            RemoteSkillDocument {
                relative_path: "development/debug/references/new.md".to_string(),
                content: b"new".to_vec(),
            },
            RemoteSkillDocument {
                relative_path: "development/debugger/SKILL.md".to_string(),
                content: b"sibling".to_vec(),
            },
        ];
        let config = serde_json::json!({ "skill_library": { "locked": ["development/debug/"] } });

        let summary = sync_skill_documents(&root, &documents, true, &locked_skill_keys(&config))
            .expect("sync");

        assert_eq!(summary.skipped_locked, 2);
        assert_eq!(summary.installed, 1);
        assert_eq!(
            std::fs::read_to_string(root.join("development/debug/SKILL.md")).expect("read"),
            "local"
        );
        assert!(!root.join("development/debug/references/new.md").exists());
        assert!(root.join("development/debugger/SKILL.md").exists());
    }

    #[test]
    fn sync_skills_rejects_paths_that_escape_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let documents = vec![RemoteSkillDocument {
            relative_path: "../outside/SKILL.md".to_string(),
            content: b"# Outside\n".to_vec(),
        }];

        let error = sync_skill_documents(temp.path(), &documents, true, &[])
            .expect_err("path traversal should be rejected");

        assert!(error.to_string().contains("must not escape"));
    }
}
