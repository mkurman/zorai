#![allow(dead_code)]

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use uuid::Uuid;
use zorai_protocol::{
    AgentDbMessage, AgentDbThread, AgentEventRow, ApprovalDecision, ApprovalPayload,
    CommandLogEntry, DaemonMessage, HistorySearchHit, ManagedCommandRequest, SessionId,
    SessionInfo, SnapshotIndexEntry, SnapshotInfo, SymbolMatch, TelemetryLedgerStatus,
    TranscriptIndexEntry, WorkspaceTopology,
};

use crate::history::{
    DatabaseRowUpdate, DatabaseSqlResult, DatabaseTablePage, DatabaseTableSummary, HistoryStore,
};
use crate::pty_session::PtySession;
use crate::snapshot::SnapshotStore;
use crate::state::{save_state, DaemonState, SavedSession};
use crate::validation::{find_symbol, validate_command};

mod history_api;
mod session_ops;

/// Central session registry — the source of truth for all running terminals.
pub struct SessionManager {
    sessions: RwLock<HashMap<SessionId, Arc<Mutex<PtySession>>>>,
    state_path: std::path::PathBuf,
    history: Arc<HistoryStore>,
    snapshots: SnapshotStore,
    pending_approvals: RwLock<HashMap<String, PendingApproval>>,
    session_approval_grants: RwLock<HashMap<SessionId, Vec<SessionApprovalGrant>>>,
    pty_channel_capacity: usize,
}

#[derive(Clone)]
struct PendingApproval {
    session_id: SessionId,
    workspace_id: Option<String>,
    execution_id: String,
    request: ManagedCommandRequest,
    approval: ApprovalPayload,
    policy_fingerprint: String,
    constraints: Vec<crate::governance::GovernanceConstraint>,
    transition_kind: crate::governance::TransitionKind,
    requested_at: u64,
    expires_at: Option<u64>,
}

#[derive(Clone)]
struct SessionApprovalGrant {
    approval_id: String,
    policy_fingerprint: String,
    expires_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskState {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundTaskStatus {
    pub background_task_id: String,
    pub kind: String,
    pub state: BackgroundTaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_path: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionActivitySnapshot {
    pub id: SessionId,
    pub workspace_id: Option<String>,
    pub last_activity_at_ms: u64,
    pub has_active_command: bool,
    pub busy: bool,
}

impl SessionManager {
    #[cfg(test)]
    pub async fn new_test(root: &std::path::Path) -> Arc<Self> {
        let history = Arc::new(
            HistoryStore::new_test_store(root)
                .await
                .expect("test history store initialization failed"),
        );
        let manager = Self::new_with_history(history, 256);
        manager
            .hydrate_pending_approvals()
            .await
            .expect("test approval inbox hydration failed");
        manager
    }

    pub(crate) fn history(&self) -> &Arc<HistoryStore> {
        &self.history
    }

    pub fn new_with_history(history: Arc<HistoryStore>, pty_channel_capacity: usize) -> Arc<Self> {
        let snapshots = SnapshotStore::new_with_history((*history).clone());
        Arc::new(Self {
            sessions: RwLock::new(HashMap::new()),
            state_path: crate::state::default_state_path(),
            history,
            snapshots,
            pending_approvals: RwLock::new(HashMap::new()),
            session_approval_grants: RwLock::new(HashMap::new()),
            pty_channel_capacity,
        })
    }
}

#[cfg(test)]
#[path = "session_manager/tests.rs"]
mod tests;
