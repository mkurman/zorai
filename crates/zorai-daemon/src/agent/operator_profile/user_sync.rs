#![allow(dead_code)]

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use anyhow::Result;

use super::super::memory::MemoryTarget;
use crate::history::HistoryStore;

const USER_SYNC_STATE_CLEAN: &str = "clean";
const USER_SYNC_STATE_DIRTY: &str = "dirty";
const USER_SYNC_STATE_RECONCILING: &str = "reconciling";
const USER_PROFILE_IMPORT_SENTINEL: &str = "__legacy_user_import_done";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::agent) enum UserProfileSyncState {
    Clean,
    Dirty,
    Reconciling,
}

impl UserProfileSyncState {
    pub(in crate::agent) fn as_str(self) -> &'static str {
        match self {
            Self::Clean => USER_SYNC_STATE_CLEAN,
            Self::Dirty => USER_SYNC_STATE_DIRTY,
            Self::Reconciling => USER_SYNC_STATE_RECONCILING,
        }
    }

    pub(in crate::agent) fn from_str(value: &str) -> Self {
        match value {
            USER_SYNC_STATE_DIRTY => Self::Dirty,
            USER_SYNC_STATE_RECONCILING => Self::Reconciling,
            _ => Self::Clean,
        }
    }
}

fn sync_state_guard() -> &'static Mutex<UserProfileSyncState> {
    static STATE: OnceLock<Mutex<UserProfileSyncState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(UserProfileSyncState::Clean))
}

pub(in crate::agent) fn current_user_sync_state() -> UserProfileSyncState {
    *sync_state_guard()
        .lock()
        .expect("user profile sync state mutex poisoned")
}

fn set_user_sync_state(state: UserProfileSyncState) {
    *sync_state_guard()
        .lock()
        .expect("user profile sync state mutex poisoned") = state;
}

/// Atomically transition from any non-`Reconciling` state to `Reconciling`.
///
/// Returns `true` if the caller acquired the reconcile slot (and is now responsible
/// for driving the reconcile to completion), or `false` if a reconcile was already
/// in progress.  Using this instead of a separate check + set eliminates the TOCTOU
/// window where two concurrent callers could both believe they own the reconcile.
fn try_acquire_reconcile() -> bool {
    let mut guard = sync_state_guard()
        .lock()
        .expect("user profile sync state mutex poisoned");
    if *guard == UserProfileSyncState::Reconciling {
        return false;
    }
    *guard = UserProfileSyncState::Reconciling;
    true
}

fn active_memory_dir(agent_data_dir: &Path) -> std::path::PathBuf {
    super::super::active_memory_dir(agent_data_dir)
}

fn user_memory_path(agent_data_dir: &Path) -> std::path::PathBuf {
    active_memory_dir(agent_data_dir).join(MemoryTarget::User.file_name())
}

async fn bootstrap_legacy_user_import(agent_data_dir: &Path, history: &HistoryStore) -> Result<()> {
    if history
        .get_profile_field(USER_PROFILE_IMPORT_SENTINEL)
        .await?
        .is_some()
    {
        return Ok(());
    }

    let path = user_memory_path(agent_data_dir);
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let trimmed = existing.trim();
    if !trimmed.is_empty() && !is_generated_profile_markdown(trimmed) {
        history
            .upsert_profile_field(
                "legacy_user_md",
                &serde_json::to_string(trimmed)?,
                0.30,
                "legacy_import",
            )
            .await?;
        history
            .append_profile_event(
                &format!("op_evt_{}", uuid::Uuid::new_v4()),
                "legacy_user_import",
                Some("legacy_user_md"),
                Some(&serde_json::to_string(trimmed)?),
                "legacy_import",
                None,
            )
            .await?;
    }

    history
        .upsert_profile_field(
            USER_PROFILE_IMPORT_SENTINEL,
            "\"true\"",
            1.0,
            "legacy_import",
        )
        .await?;
    Ok(())
}

const GENERATED_PROFILE_HEADER: &str =
    "Profile summary is generated from SQLite-backed operator profile.";

fn is_generated_profile_markdown(content: &str) -> bool {
    content.contains(GENERATED_PROFILE_HEADER)
}

/// Legacy profile values are JSON-encoded markdown blobs. Earlier daemon
/// versions could re-import an already-generated USER.md, nesting a full
/// generated document (with its own escaped legacy fields) inside the value.
/// Rendering that raw would re-escape it on every reconcile cycle. Unwrap
/// generated wrappers recursively and emit only the innermost plain lines.
fn append_flattened_legacy_value(value: &str, out: &mut Vec<String>) {
    if is_generated_profile_markdown(value) {
        for line in value.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("- ") else {
                continue;
            };
            let Some((_, raw_value)) = rest.split_once(": ") else {
                continue;
            };
            match serde_json::from_str::<String>(raw_value.trim()) {
                Ok(decoded) => append_flattened_legacy_value(&decoded, out),
                Err(_) => out.push(trimmed.to_string()),
            }
        }
        return;
    }

    for line in value.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "# User" {
            continue;
        }
        if trimmed.starts_with('-') || trimmed.starts_with('#') {
            out.push(trimmed.to_string());
        } else {
            out.push(format!("- {trimmed}"));
        }
    }
}

fn render_user_profile_markdown(fields: &[crate::history::OperatorProfileFieldRow]) -> String {
    let mut lines = vec![
        "# User".to_string(),
        GENERATED_PROFILE_HEADER.to_string(),
        "".to_string(),
    ];

    let mut seen = std::collections::HashSet::new();
    for row in fields {
        if row.field_key.starts_with("legacy_") {
            let decoded = serde_json::from_str::<String>(&row.field_value_json)
                .unwrap_or_else(|_| row.field_value_json.clone());
            let mut flattened = Vec::new();
            append_flattened_legacy_value(&decoded, &mut flattened);
            for line in flattened {
                if seen.insert(line.clone()) {
                    lines.push(line);
                }
            }
        } else {
            lines.push(format!("- {}: {}", row.field_key, row.field_value_json));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

pub(in crate::agent) async fn reconcile_user_profile_from_db(
    agent_data_dir: &Path,
    history: &HistoryStore,
) -> Result<()> {
    if !try_acquire_reconcile() {
        return Ok(());
    }
    reconcile_inner(agent_data_dir, history).await
}

/// Drive the reconcile body.  **Caller must have already set state to `Reconciling`**
/// (either via `set_user_sync_state` or `try_acquire_reconcile`).
/// All error paths reset state to `Dirty` so the slot is never left stuck.
async fn reconcile_inner(agent_data_dir: &Path, history: &HistoryStore) -> Result<()> {
    if let Err(error) = bootstrap_legacy_user_import(agent_data_dir, history).await {
        set_user_sync_state(UserProfileSyncState::Dirty);
        return Err(error);
    }

    let rows = match history
        .list_profile_fields_excluding_ordered_by_key(USER_PROFILE_IMPORT_SENTINEL)
        .await
    {
        Ok(r) => r,
        Err(error) => {
            set_user_sync_state(UserProfileSyncState::Dirty);
            return Err(error);
        }
    };

    let content = render_user_profile_markdown(&rows);
    let path = user_memory_path(agent_data_dir);
    match tokio::fs::write(&path, content).await {
        Ok(()) => {
            set_user_sync_state(UserProfileSyncState::Clean);
            Ok(())
        }
        Err(error) => {
            set_user_sync_state(UserProfileSyncState::Dirty);
            Err(error.into())
        }
    }
}

pub(in crate::agent) async fn stage_legacy_user_memory_write(
    history: &HistoryStore,
    content: &str,
) -> Result<()> {
    stage_legacy_user_memory_write_events(history, content).await?;
    set_user_sync_state(UserProfileSyncState::Dirty);
    Ok(())
}

/// Write the legacy-append events to the DB **without** touching sync state.
/// Used by `handle_user_memory_append_with_reconcile` so that the caller can
/// manage the state transition atomically around the reconcile slot acquisition.
async fn stage_legacy_user_memory_write_events(
    history: &HistoryStore,
    content: &str,
) -> Result<()> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value_json = serde_json::to_string(trimmed)?;
    history
        .upsert_profile_field("legacy_user_signal", &value_json, 0.55, "legacy_append")
        .await?;
    history
        .append_profile_event(
            &format!("op_evt_{}", uuid::Uuid::new_v4()),
            "legacy_user_memory_append",
            Some("legacy_user_signal"),
            Some(&value_json),
            "legacy_append",
            None,
        )
        .await?;
    Ok(())
}

pub(in crate::agent) async fn handle_user_memory_append_with_reconcile(
    agent_data_dir: &Path,
    history: &HistoryStore,
    content: &str,
) -> Result<()> {
    let acquired = try_acquire_reconcile();

    if let Err(error) = stage_legacy_user_memory_write_events(history, content).await {
        if acquired {
            set_user_sync_state(UserProfileSyncState::Dirty);
        }
        return Err(error);
    }

    if !acquired {
        return Ok(());
    }

    reconcile_inner(agent_data_dir, history).await
}

pub(in crate::agent) async fn replace_user_profile_from_operator_edit(
    agent_data_dir: &Path,
    history: &HistoryStore,
    content: &str,
) -> Result<()> {
    bootstrap_legacy_user_import(agent_data_dir, history).await?;
    let rows = history
        .list_profile_fields_excluding_ordered_by_key(USER_PROFILE_IMPORT_SENTINEL)
        .await?;
    let structured_lines = rows
        .iter()
        .filter(|row| !row.field_key.starts_with("legacy_"))
        .map(|row| format!("- {}: {}", row.field_key, row.field_value_json))
        .collect::<std::collections::HashSet<_>>();
    let body = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != "# User"
                && trimmed != GENERATED_PROFILE_HEADER
                && !structured_lines.contains(trimmed)
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    for row in rows.iter().filter(|row| row.field_key.starts_with("legacy_")) {
        history.delete_profile_field(&row.field_key).await?;
    }
    let value_json = serde_json::to_string(&body)?;
    if !body.is_empty() {
        history
            .upsert_profile_field("legacy_user_md", &value_json, 1.0, "operator_edit")
            .await?;
    }
    history
        .append_profile_event(
            &format!("op_evt_{}", uuid::Uuid::new_v4()),
            "operator_user_md_edit",
            Some("legacy_user_md"),
            Some(&value_json),
            "operator_edit",
            None,
        )
        .await?;
    reconcile_user_profile_from_db(agent_data_dir, history).await
}

#[cfg(test)]
#[path = "user_sync/test_support.rs"]
mod test_support;
#[cfg(test)]
pub(in crate::agent) use test_support::{
    acquire_user_sync_test_guard, set_user_sync_state_for_test,
};
#[cfg(test)]
#[path = "user_sync/tests.rs"]
mod tests;
