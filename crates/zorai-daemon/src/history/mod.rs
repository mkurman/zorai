#![allow(dead_code)]

use crate::agent::liveness::state_layers::CheckpointType;
use crate::agent::types::{
    AgentTask, AgentTaskLogEntry, GoalRun, GoalRunEvent, GoalRunStatus, GoalRunStep,
    GoalRunStepKind, GoalRunStepStatus, TaskLogLevel, TaskPriority, TaskStatus,
};
use crate::governance::{GovernanceConstraint, TransitionKind};
use anyhow::{Context, Result};
#[cfg(test)]
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_rusqlite;
use zorai_protocol::{
    AgentDbMessage, AgentDbThread, AgentEventRow, AgentStatisticsSnapshot, AgentStatisticsTotals,
    AgentStatisticsWindow, ApprovalPayload, CommandLogEntry, GatewayHealthState, HistorySearchHit,
    ManagedCommandRequest, ModelStatisticsRow, ProviderStatisticsRow, SnapshotIndexEntry,
    TranscriptIndexEntry, WormChainTip,
};

/// Helper trait to convert any error into `tokio_rusqlite::Error` inside `.call()` closures.
trait IntoCallError<T> {
    fn call_err(self) -> std::result::Result<T, tokio_rusqlite::Error>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> IntoCallError<T>
    for std::result::Result<T, E>
{
    fn call_err(self) -> std::result::Result<T, tokio_rusqlite::Error> {
        self.map_err(|e| tokio_rusqlite::Error::Other(Box::new(e)))
    }
}
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use sysinfo::System;

/// Result of verifying a single WORM ledger file's hash-chain integrity.
pub struct WormIntegrityResult {
    pub kind: String,
    pub total_entries: usize,
    pub valid: bool,
    pub first_invalid_seq: Option<usize>,
    pub message: String,
}

/// Maximum wall-clock per `ReadPool::call`. If a chosen connection's background
/// thread is wedged (long-running query, blocked I/O, etc.), the call errors
/// instead of awaiting forever — caller can retry, and the next `pick()` will
/// land on a less-loaded connection. Generous default; under healthy load
/// every read finishes in <1s.
const READ_POOL_CALL_TIMEOUT_SECS: u64 = 30;
/// Above this threshold we emit a `WARN` so slow reads are visible in logs
/// without spamming for every routine query.
const READ_POOL_SLOW_CALL_WARN_MS: u64 = 2_000;

/// One pooled connection plus its in-flight call counter.
///
/// `in_flight` is incremented before forwarding to `tokio_rusqlite::Connection::call`
/// and decremented when that future settles (Ok, Err, or timeout). It's used
/// only for load-balanced picking — atomicity around the count itself is best-
/// effort: a brief mid-pick race may put two callers on the same conn, but it
/// self-corrects on the next pick.
struct PoolEntry {
    conn: tokio_rusqlite::Connection,
    in_flight: std::sync::atomic::AtomicUsize,
}

/// Load-balanced pool of read-only `tokio_rusqlite::Connection`s.
///
/// Each underlying connection has its own background thread; calls to a single
/// connection are serialized through that thread. Distributing calls across N
/// connections lets up to N read queries run truly concurrently (SQLite WAL
/// permits unbounded concurrent readers).
///
/// `pick()` picks the connection with the smallest in-flight count rather than
/// blindly round-robin'ing. Round-robin's failure mode was: if conn[k] got
/// stuck on a slow query, every Nth caller still routed to it and awaited
/// indefinitely — the user-visible signature was "the daemon hangs every few
/// requests". Least-loaded selection naturally skips a stuck conn (its
/// in_flight count grows) until it recovers, and the per-call timeout cap
/// turns any unrecoverable wedge into a clean error rather than an invisible
/// hang.
///
/// API-compatible with `tokio_rusqlite::Connection`: callers keep using
/// `.call(...)` unchanged.
#[derive(Clone)]
pub(crate) struct ReadPool {
    entries: std::sync::Arc<Vec<PoolEntry>>,
    cursor: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl ReadPool {
    pub(crate) fn new(conns: Vec<tokio_rusqlite::Connection>) -> Self {
        debug_assert!(
            !conns.is_empty(),
            "ReadPool requires at least one connection"
        );
        let entries = conns
            .into_iter()
            .map(|conn| PoolEntry {
                conn,
                in_flight: std::sync::atomic::AtomicUsize::new(0),
            })
            .collect::<Vec<_>>();
        Self {
            entries: std::sync::Arc::new(entries),
            cursor: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Returns the index of the connection with the smallest `in_flight`
    /// count. Ties are broken by a rolling cursor so equally-idle conns get
    /// fair round-robin treatment.
    fn pick_idx(&self) -> usize {
        use std::sync::atomic::Ordering;
        let len = self.entries.len();
        let start = self.cursor.fetch_add(1, Ordering::Relaxed) % len;
        let mut best_idx = start;
        let mut best_load = self.entries[start].in_flight.load(Ordering::Relaxed);
        for offset in 1..len {
            let idx = (start + offset) % len;
            let load = self.entries[idx].in_flight.load(Ordering::Relaxed);
            if load < best_load {
                best_idx = idx;
                best_load = load;
                if best_load == 0 {
                    break;
                }
            }
        }
        best_idx
    }

    pub(crate) async fn call<F, R>(&self, function: F) -> tokio_rusqlite::Result<R>
    where
        F: FnOnce(&mut rusqlite::Connection) -> tokio_rusqlite::Result<R> + 'static + Send,
        R: Send + 'static,
    {
        use std::sync::atomic::Ordering;
        let idx = self.pick_idx();
        let entry = &self.entries[idx];
        entry.in_flight.fetch_add(1, Ordering::Relaxed);
        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(READ_POOL_CALL_TIMEOUT_SECS),
            entry.conn.call(function),
        )
        .await;
        entry.in_flight.fetch_sub(1, Ordering::Relaxed);
        let elapsed_ms = started.elapsed().as_millis() as u64;
        match outcome {
            Ok(result) => {
                if elapsed_ms >= READ_POOL_SLOW_CALL_WARN_MS {
                    tracing::warn!(
                        elapsed_ms,
                        conn_idx = idx,
                        pool_size = self.entries.len(),
                        "ReadPool::call slow query — investigate stuck conn or unindexed scan"
                    );
                }
                result
            }
            Err(_) => {
                tracing::warn!(
                    elapsed_ms,
                    conn_idx = idx,
                    pool_size = self.entries.len(),
                    timeout_secs = READ_POOL_CALL_TIMEOUT_SECS,
                    "ReadPool::call timed out — connection likely wedged, returning TimedOut error"
                );
                Err(tokio_rusqlite::Error::Other(Box::new(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "ReadPool::call timed out after {READ_POOL_CALL_TIMEOUT_SECS}s on conn {idx}"
                    ),
                ))))
            }
        }
    }
}

/// Per-key caches for hot read paths. All three are bounded LRU-ish
/// (drop on overflow) with a short TTL safety net plus explicit
/// invalidation on writes — see `cache.rs` for the design rationale.
#[derive(Debug)]
pub(crate) struct HistoryCaches {
    /// `thread_metadata_json(thread_id) -> Option<String>`. Invalidated by
    /// any thread upsert/delete touching the same id.
    thread_metadata_json: cache::TtlCache<String, Option<String>>,
    /// `get_skill_variant(variant_id) -> Option<SkillVariantRecord>`.
    /// Invalidated by upsert/update/delete on the variant.
    skill_variant: cache::TtlCache<String, Option<SkillVariantRecord>>,
    /// `latest_goal_run_for_thread(thread_id) -> Option<GoalRun>`.
    /// Goal-run writes can change which run is "latest" for a given
    /// thread; the safest cheap invalidation is `clear()` on any
    /// goal_run upsert/delete (the alternative — enumerating every
    /// thread_id touched — is fragile because a goal_run can be tied to
    /// multiple thread refs simultaneously).
    latest_goal_run_for_thread: cache::TtlCache<String, Option<GoalRun>>,
}

impl HistoryCaches {
    fn new() -> Self {
        use std::time::Duration;
        Self {
            thread_metadata_json: cache::TtlCache::new(Duration::from_secs(2), 256),
            skill_variant: cache::TtlCache::new(Duration::from_secs(5), 128),
            latest_goal_run_for_thread: cache::TtlCache::new(Duration::from_secs(2), 256),
        }
    }
}

#[derive(Clone)]
pub struct HistoryStore {
    pub(crate) conn: tokio_rusqlite::Connection,
    /// Dedicated WRITER connection for embedding-queue maintenance
    /// (`claim_embedding_jobs`, `complete_embedding_job`, etc.). Without
    /// this, every embedding tick queued its writes behind the main
    /// daemon's writer (persist_thread_by_id after every turn, task
    /// persists, goal-run persists, etc.) — and a single batch of 16
    /// embedding HTTP requests + LanceDB writes can hold up the writer
    /// thread for many seconds. This separate connection runs on its own
    /// `tokio_rusqlite` background thread, so embedding work is *truly*
    /// off the main lane.
    pub(crate) embedding_writer_conn: tokio_rusqlite::Connection,
    /// Reader pool for background work (syncs, indexers, supervision loops).
    /// These can flood the queue without blocking UI.
    pub(crate) read_conn: ReadPool,
    /// Reader pool reserved for user-interactive paths (welcome, thread load,
    /// message list). Background sync loops never touch these connections, so
    /// a UI-facing read can never queue behind hundreds of batch jobs. We've
    /// seen the unified pool wedge for hundreds of seconds under load — split
    /// pools are the only architectural fix that survives bursty workloads.
    pub(crate) interactive_read_conn: ReadPool,
    /// Engine-neutral facade over the writer connection. Call sites are being
    /// migrated from the raw `conn`/`read_conn` `.call(...)` closures onto these
    /// (`db::DbConn`) handles so an alternate engine (libSQL) can be swapped in.
    /// During the migration both the raw connection and its facade wrapper point
    /// at the same underlying SQLite connection.
    pub(crate) conn_db: std::sync::Arc<dyn db::DbConn>,
    pub(crate) embedding_writer_db: std::sync::Arc<dyn db::DbConn>,
    pub(crate) read_db: std::sync::Arc<dyn db::DbConn>,
    pub(crate) interactive_read_db: std::sync::Arc<dyn db::DbConn>,
    pub(crate) caches: std::sync::Arc<HistoryCaches>,
    skill_dir: PathBuf,
    telemetry_dir: PathBuf,
    worm_dir: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AgentThreadListQuery {
    pub created_after: Option<i64>,
    pub created_before: Option<i64>,
    pub updated_after: Option<i64>,
    pub updated_before: Option<i64>,
    pub agent_names: Vec<String>,
    pub include_empty_agent_name: bool,
    pub title_query: Option<String>,
    pub title_excluded_prefixes: Vec<String>,
    pub pinned: Option<bool>,
    pub min_message_count: Option<i64>,
    pub include_internal: bool,
    pub excluded_ids: Vec<String>,
    pub hidden_id_prefixes: Vec<String>,
    pub hidden_message_substrings: Vec<String>,
    pub limit: Option<usize>,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThreadDelegatePayloadMessageRef {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThreadDelegatePayloadContext {
    pub title: String,
    pub messages: Vec<ThreadDelegatePayloadMessageRef>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AgentTaskListQuery {
    pub id: Option<String>,
    /// Batch variant of `id`: returns rows for any task id in the slice.
    /// Used by `goal_related_task_ids` to collapse a per-id N+1 loop into
    /// a single query. Empty vec is treated as "no filter on this axis"
    /// (same as `id = None`).
    pub ids: Vec<String>,
    pub status: Option<String>,
    pub statuses: Vec<String>,
    pub source: Option<String>,
    pub thread_id: Option<String>,
    pub thread_ids: Vec<String>,
    pub goal_run_id: Option<String>,
    pub parent_task_id: Option<String>,
    /// Batch variant of `parent_task_id`. Same N+1 fix as `ids`.
    pub parent_task_ids: Vec<String>,
    pub awaiting_approval_id: Option<String>,
    pub supervisor_config_present: bool,
    pub exclude_terminal_statuses: bool,
    pub order_by_recent_activity_desc: bool,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentTaskGoalContext {
    pub goal_run_id: Option<String>,
    pub goal_step_id: Option<String>,
    pub session_id: Option<String>,
    pub source: String,
    pub parent_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentTaskOperationalRef {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub progress: u8,
    pub awaiting_approval_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentTaskSummaryRef {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentTaskQuietRecoveryRef {
    pub id: String,
    pub source: String,
    pub status: TaskStatus,
    pub progress: u8,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub goal_run_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentTaskSubagentHierarchyRef {
    pub id: String,
    pub parent_task_id: Option<String>,
    pub containment_scope: Option<String>,
}

impl From<&AgentTask> for AgentTaskOperationalRef {
    fn from(task: &AgentTask) -> Self {
        Self {
            id: task.id.clone(),
            title: task.title.clone(),
            status: task.status,
            progress: task.progress,
            awaiting_approval_id: task.awaiting_approval_id.clone(),
        }
    }
}

impl From<&AgentTask> for AgentTaskQuietRecoveryRef {
    fn from(task: &AgentTask) -> Self {
        Self {
            id: task.id.clone(),
            source: task.source.clone(),
            status: task.status,
            progress: task.progress,
            created_at: task.created_at,
            started_at: task.started_at,
            goal_run_id: task.goal_run_id.clone(),
            parent_task_id: task.parent_task_id.clone(),
            thread_id: task.thread_id.clone(),
        }
    }
}

impl From<&AgentTask> for AgentTaskSubagentHierarchyRef {
    fn from(task: &AgentTask) -> Self {
        Self {
            id: task.id.clone(),
            parent_task_id: task.parent_task_id.clone(),
            containment_scope: task.containment_scope.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunOperationalRef {
    pub id: String,
    pub status: GoalRunStatus,
    pub title: String,
    pub current_step_index: usize,
    pub step_count: usize,
}

impl From<&GoalRun> for GoalRunOperationalRef {
    fn from(goal_run: &GoalRun) -> Self {
        Self {
            id: goal_run.id.clone(),
            status: goal_run.status,
            title: goal_run.title.clone(),
            current_step_index: goal_run.current_step_index,
            step_count: goal_run.steps.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunStuckCheckRef {
    pub id: String,
    pub status: GoalRunStatus,
    pub title: String,
    pub updated_at: u64,
    pub last_error: Option<String>,
}

impl From<&GoalRun> for GoalRunStuckCheckRef {
    fn from(goal_run: &GoalRun) -> Self {
        Self {
            id: goal_run.id.clone(),
            status: goal_run.status,
            title: goal_run.title.clone(),
            updated_at: goal_run.updated_at,
            last_error: goal_run.last_error.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunBriefRef {
    pub id: String,
    pub status: GoalRunStatus,
    pub title: String,
    pub updated_at: u64,
    pub thread_id: Option<String>,
    pub current_step_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunQuietRecoveryRef {
    pub id: String,
    pub status: GoalRunStatus,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub thread_id: Option<String>,
    pub root_thread_id: Option<String>,
    pub execution_thread_ids: Vec<String>,
    pub current_step_index: usize,
    pub current_step_id: Option<String>,
    pub current_step_title: Option<String>,
    pub active_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunStatusReplyRef {
    pub id: String,
    pub status: GoalRunStatus,
    pub title: String,
    pub updated_at: u64,
    pub current_step_title: Option<String>,
    pub plan_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunTaskContextRef {
    pub current_step_index: usize,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunRepoContextRef {
    pub id: String,
    pub session_id: Option<String>,
    pub current_step_index: usize,
}

impl From<&GoalRun> for GoalRunRepoContextRef {
    fn from(goal_run: &GoalRun) -> Self {
        Self {
            id: goal_run.id.clone(),
            session_id: goal_run.session_id.clone(),
            current_step_index: goal_run.current_step_index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunProgressMetricsRef {
    pub steps_completed: usize,
    pub steps_total: usize,
}

impl From<&GoalRun> for GoalRunProgressMetricsRef {
    fn from(goal_run: &GoalRun) -> Self {
        Self {
            steps_completed: goal_run
                .steps
                .iter()
                .filter(|step| step.status == GoalRunStepStatus::Completed)
                .count(),
            steps_total: goal_run.steps.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunPolicyContextRef {
    pub goal: String,
    pub title: String,
    pub current_step_title: Option<String>,
    pub steps_completed: usize,
    pub steps_total: usize,
}

impl From<&GoalRun> for GoalRunPolicyContextRef {
    fn from(goal_run: &GoalRun) -> Self {
        let progress = GoalRunProgressMetricsRef::from(goal_run);
        Self {
            goal: goal_run.goal.clone(),
            title: goal_run.title.clone(),
            current_step_title: goal_run.current_step_title.clone().or_else(|| {
                goal_run
                    .steps
                    .get(goal_run.current_step_index)
                    .map(|step| step.title.clone())
            }),
            steps_completed: progress.steps_completed,
            steps_total: progress.steps_total,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunTodoContextRef {
    pub step_index: usize,
    pub step_id: Option<String>,
    pub step_status: Option<GoalRunStepStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunWorkspaceRuntimeRef {
    pub id: String,
    pub status: GoalRunStatus,
    pub last_error: Option<String>,
    pub reflection_summary: Option<String>,
    pub plan_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunCompactionScopeRef {
    pub id: String,
    pub active_task_id: Option<String>,
    pub title: String,
    pub goal: String,
    pub status: GoalRunStatus,
    pub root_thread_id: Option<String>,
    pub active_thread_id: Option<String>,
    pub execution_thread_ids: Vec<String>,
    pub current_step_title: Option<String>,
    pub current_step_status: Option<GoalRunStepStatus>,
    pub current_step_summary: Option<String>,
    pub plan_summary: Option<String>,
    pub latest_error: Option<String>,
    pub recent_events: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GatewayTurnAutoSendProjection {
    pub used_send_tool: bool,
    pub latest_assistant_response: Option<String>,
}

impl From<&GoalRun> for GoalRunStatusReplyRef {
    fn from(goal_run: &GoalRun) -> Self {
        Self {
            id: goal_run.id.clone(),
            status: goal_run.status,
            title: goal_run.title.clone(),
            updated_at: goal_run.updated_at,
            current_step_title: goal_run.current_step_title.clone().or_else(|| {
                goal_run
                    .steps
                    .get(goal_run.current_step_index)
                    .map(|step| step.title.clone())
            }),
            plan_summary: goal_run.plan_summary.clone(),
        }
    }
}

impl From<&GoalRun> for GoalRunQuietRecoveryRef {
    fn from(goal_run: &GoalRun) -> Self {
        let current_step = goal_run.steps.get(goal_run.current_step_index);
        Self {
            id: goal_run.id.clone(),
            status: goal_run.status,
            created_at: goal_run.created_at,
            started_at: goal_run.started_at,
            thread_id: goal_run.thread_id.clone(),
            root_thread_id: goal_run.root_thread_id.clone(),
            execution_thread_ids: goal_run.execution_thread_ids.clone(),
            current_step_index: goal_run.current_step_index,
            current_step_id: current_step.map(|step| step.id.clone()),
            current_step_title: goal_run
                .current_step_title
                .clone()
                .or_else(|| current_step.map(|step| step.title.clone())),
            active_task_id: goal_run.active_task_id.clone(),
        }
    }
}

impl From<&GoalRun> for GoalRunBriefRef {
    fn from(goal_run: &GoalRun) -> Self {
        Self {
            id: goal_run.id.clone(),
            status: goal_run.status,
            title: goal_run.title.clone(),
            updated_at: goal_run.updated_at,
            thread_id: goal_run.thread_id.clone(),
            current_step_title: goal_run.current_step_title.clone().or_else(|| {
                goal_run
                    .steps
                    .get(goal_run.current_step_index)
                    .map(|step| step.title.clone())
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRunThreadRef {
    pub id: String,
    pub status: GoalRunStatus,
    pub updated_at: u64,
    pub thread_id: Option<String>,
    pub root_thread_id: Option<String>,
    pub active_thread_id: Option<String>,
    pub execution_thread_ids: Vec<String>,
}

impl From<&GoalRun> for GoalRunThreadRef {
    fn from(goal_run: &GoalRun) -> Self {
        Self {
            id: goal_run.id.clone(),
            status: goal_run.status,
            updated_at: goal_run.updated_at,
            thread_id: goal_run.thread_id.clone(),
            root_thread_id: goal_run.root_thread_id.clone(),
            active_thread_id: goal_run.active_thread_id.clone(),
            execution_thread_ids: goal_run.execution_thread_ids.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThreadRecallMatchRow {
    pub thread_id: String,
    pub title: String,
    pub updated_at: u64,
    pub message_count: u32,
    pub metadata_json: Option<String>,
    pub message_role: Option<String>,
    pub message_content: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ConciergeGoalContext {
    pub latest_goal_run: Option<GoalRun>,
    pub running_goal_total: usize,
    pub paused_goal_total: usize,
}

mod database_viewer;
pub(crate) use database_viewer::{
    DatabaseRowUpdate, DatabaseSqlResult, DatabaseTablePage, DatabaseTableSummary,
};

pub struct ManagedHistoryRecord {
    pub execution_id: String,
    pub session_id: String,
    pub workspace_id: Option<String>,
    pub command: String,
    pub rationale: String,
    pub source: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub snapshot_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ManagedCommandFinishedRecord {
    pub command: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub snapshot_path: Option<String>,
}

pub struct MemoryProvenanceRecord<'a> {
    pub id: &'a str,
    pub target: &'a str,
    pub mode: &'a str,
    pub source_kind: &'a str,
    pub content: &'a str,
    pub fact_keys: &'a [String],
    pub thread_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub goal_run_id: Option<&'a str>,
    pub created_at: u64,
    pub sign: bool,
}

#[derive(Debug, Clone)]
pub struct CollaborationSessionRow {
    pub parent_task_id: String,
    pub session_json: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct DebateSessionRow {
    pub session_id: String,
    pub session_json: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct DebateArgumentRow {
    pub session_id: String,
    pub argument_json: String,
    pub created_at: u64,
}

#[derive(Debug, Clone)]
pub struct DebateVerdictRow {
    pub session_id: String,
    pub verdict_json: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct CritiqueSessionRow {
    pub session_id: String,
    pub session_json: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct CritiqueArgumentRow {
    pub session_id: String,
    pub role: String,
    pub claim: String,
    pub weight: f64,
    pub evidence_json: String,
    pub created_at: u64,
}

#[derive(Debug, Clone)]
pub struct CritiqueResolutionRow {
    pub session_id: String,
    pub decision: String,
    pub resolution_json: String,
    pub risk_score: f64,
    pub confidence: f64,
    pub resolved_at: u64,
}

#[derive(Debug, Clone)]
pub struct EmergentProtocolRow {
    pub protocol_id: String,
    pub token: String,
    pub description: String,
    pub agent_a: String,
    pub agent_b: String,
    pub thread_id: String,
    pub normalized_pattern: String,
    pub signal_kind: String,
    pub context_signature_json: String,
    pub created_at: u64,
    pub activated_at: u64,
    pub last_used_at: Option<u64>,
    pub usage_count: u64,
    pub success_rate: f64,
    pub source_candidate_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProtocolStepRow {
    pub protocol_id: String,
    pub step_index: u64,
    pub intent: String,
    pub tool_name: Option<String>,
    pub args_template_json: String,
}

#[derive(Debug, Clone)]
pub struct ProtocolUsageLogRow {
    pub id: String,
    pub protocol_id: String,
    pub used_at: u64,
    pub execution_time_ms: Option<u64>,
    pub success: bool,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MetaCognitionModelRow {
    pub id: i64,
    pub agent_id: String,
    pub calibration_offset: f64,
    pub last_updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct CognitiveBiasRow {
    pub id: i64,
    pub model_id: i64,
    pub name: String,
    pub trigger_pattern_json: String,
    pub mitigation_prompt: String,
    pub severity: f64,
    pub occurrence_count: u64,
}

#[derive(Debug, Clone)]
pub struct WorkflowProfileRow {
    pub id: i64,
    pub model_id: i64,
    pub name: String,
    pub avg_success_rate: f64,
    pub avg_steps: u64,
    pub typical_tools_json: String,
}

#[derive(Debug, Clone)]
pub struct ImplicitSignalRow {
    pub id: String,
    pub session_id: String,
    pub signal_type: String,
    pub weight: f64,
    pub timestamp_ms: u64,
    pub context_snapshot_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SatisfactionScoreRow {
    pub id: String,
    pub session_id: String,
    pub score: f64,
    pub computed_at_ms: u64,
    pub label: String,
    pub signal_count: u64,
}

#[derive(Debug, Clone)]
pub struct EventTriggerRow {
    pub id: String,
    pub event_family: String,
    pub event_kind: String,
    pub agent_id: Option<String>,
    pub target_state: Option<String>,
    pub thread_id: Option<String>,
    pub enabled: bool,
    pub cooldown_secs: u64,
    pub risk_label: String,
    pub notification_kind: String,
    pub prompt_template: Option<String>,
    pub tool_name: Option<String>,
    pub tool_payload_json: Option<String>,
    pub title_template: String,
    pub body_template: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_fired_at: Option<u64>,
    pub max_retries: u32,
}

#[derive(Debug, Clone)]
pub struct TriggerFireHistoryRow {
    pub id: String,
    pub trigger_id: String,
    pub event_family: String,
    pub event_kind: String,
    pub status: String,
    pub fired_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub retry_count: u64,
    pub error_message: Option<String>,
    pub created_task_id: Option<String>,
    pub notice_id: Option<String>,
    pub payload_json: String,
}

#[derive(Debug, Clone)]
pub struct AgentWakeupRow {
    pub id: String,
    pub thread_id: String,
    pub message: String,
    pub interval_ms: u64,
    pub next_fire_at: u64,
    pub repetitions_remaining: Option<u64>,
    pub wakeup_kind: String,
    pub goal_run_id: Option<String>,
    pub reason: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone)]
pub struct RoutineDefinitionRow {
    pub id: String,
    pub title: String,
    pub description: String,
    pub enabled: bool,
    pub paused_at: Option<u64>,
    pub schedule_expression: String,
    pub target_kind: String,
    pub target_payload_json: String,
    pub schema_version: u32,
    pub next_run_at: Option<u64>,
    pub last_run_at: Option<u64>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub last_success_summary: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct RoutineRunRow {
    pub id: String,
    pub routine_id: String,
    pub trigger_kind: String,
    pub status: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub created_task_id: Option<String>,
    pub created_goal_run_id: Option<String>,
    pub payload_json: String,
    pub result_summary: Option<String>,
    pub error: Option<String>,
    pub rerun_of_run_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EventLogRow {
    pub id: String,
    pub event_family: String,
    pub event_kind: String,
    pub state: Option<String>,
    pub thread_id: Option<String>,
    pub payload_json: String,
    pub risk_label: String,
    pub handled_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessStateRecordRow {
    pub entry_id: String,
    pub entity_id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub record_kind: String,
    pub status: Option<String>,
    pub summary: String,
    pub payload_json: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct IntentPredictionRow {
    pub id: String,
    pub session_id: String,
    pub context_state_hash: String,
    pub predicted_action: String,
    pub confidence: f64,
    pub actual_action: Option<String>,
    pub was_correct: Option<bool>,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SystemOutcomePredictionRow {
    pub id: String,
    pub session_id: String,
    pub prediction_type: String,
    pub predicted_outcome: String,
    pub confidence: f64,
    pub actual_outcome: Option<String>,
    pub was_correct: Option<bool>,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct IntentModelRow {
    pub id: Option<i64>,
    pub agent_id: String,
    pub model_blob: Option<Vec<u8>>,
    pub created_at_ms: u64,
    pub accuracy_score: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct DreamCycleRow {
    pub id: Option<i64>,
    pub started_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub idle_duration_ms: u64,
    pub tasks_analyzed: u64,
    pub counterfactuals_generated: u64,
    pub counterfactuals_successful: u64,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct CounterfactualEvaluationRow {
    pub id: Option<i64>,
    pub dream_cycle_id: i64,
    pub source_task_id: String,
    pub variation_type: String,
    pub counterfactual_description: String,
    pub estimated_token_saving: Option<f64>,
    pub estimated_time_saving_ms: Option<i64>,
    pub estimated_revision_reduction: Option<u64>,
    pub score: f64,
    pub threshold_met: bool,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct CognitiveResonanceSampleRow {
    pub id: Option<i64>,
    pub sampled_at_ms: u64,
    pub revision_velocity_ms: Option<u64>,
    pub session_entropy: Option<f64>,
    pub approval_latency_ms: Option<u64>,
    pub tool_hesitation_count: u64,
    pub cognitive_state: String,
    pub state_confidence: f64,
    pub resonance_score: f64,
    pub verbosity_adjustment: f64,
    pub risk_adjustment: f64,
    pub proactiveness_adjustment: f64,
    pub memory_urgency_adjustment: f64,
}

#[derive(Debug, Clone)]
pub struct BehaviorAdjustmentLogRow {
    pub id: Option<i64>,
    pub adjusted_at_ms: u64,
    pub parameter: String,
    pub old_value: f64,
    pub new_value: f64,
    pub trigger_reason: String,
    pub resonance_score: f64,
}

#[derive(Debug, Clone)]
pub struct TemporalPatternRow {
    pub id: Option<i64>,
    pub pattern_type: String,
    pub timescale: String,
    pub pattern_description: String,
    pub context_filter: Option<String>,
    pub frequency: u64,
    pub last_observed_ms: u64,
    pub first_observed_ms: u64,
    pub confidence: f64,
    pub decay_rate: f64,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct TemporalPredictionRow {
    pub id: Option<i64>,
    pub pattern_id: i64,
    pub predicted_action: String,
    pub predicted_at_ms: u64,
    pub confidence: f64,
    pub actual_action: Option<String>,
    pub was_accepted: Option<bool>,
    pub accuracy_score: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct PrecomputationLogRow {
    pub id: Option<i64>,
    pub prediction_id: i64,
    pub precomputation_type: String,
    pub precomputation_details: String,
    pub started_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub was_used: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct OperatorProfileSessionRow {
    pub session_id: String,
    pub kind: String,
    pub session_json: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct ProviderAuthStateRow {
    pub provider_id: String,
    pub auth_mode: String,
    pub state_json: serde_json::Value,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalRuntimeProfileRow {
    pub runtime: String,
    pub profile_json: String,
    pub session_id: Option<String>,
    pub source_config_path: Option<String>,
    pub source_fingerprint: Option<String>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalRuntimeImportSessionRow {
    pub session_id: String,
    pub runtime: String,
    pub source_config_path: String,
    pub source_fingerprint: String,
    pub dry_run: bool,
    pub conflict_policy: String,
    pub source_surface: String,
    pub session_json: String,
    pub imported_at_ms: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportedRuntimeAssetRow {
    pub asset_id: String,
    pub session_id: String,
    pub runtime: String,
    pub asset_kind: String,
    pub bucket: String,
    pub severity: String,
    pub recommended_action: Option<String>,
    pub source_path: Option<String>,
    pub source_fingerprint: Option<String>,
    pub conflict_policy: String,
    pub asset_json: String,
    pub created_at_ms: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalRuntimeShadowRunRow {
    pub run_id: String,
    pub runtime: String,
    pub session_id: String,
    pub workflow: String,
    pub readiness_score: u8,
    pub blocker_count: u32,
    pub summary: String,
    pub payload_json: String,
    pub created_at_ms: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProfileRow {
    pub profile_id: String,
    pub label: String,
    pub profile_dir: String,
    pub browser_kind: Option<String>,
    pub workspace_id: Option<String>,
    pub health_state: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_used_at: Option<u64>,
    pub last_auth_success_at: Option<u64>,
    pub last_auth_failure_at: Option<u64>,
    pub last_auth_failure_reason: Option<String>,
}

/// A single learned or answered profile field for the operator.
#[derive(Debug, Clone)]
pub struct OperatorProfileFieldRow {
    pub field_key: String,
    pub field_value_json: String,
    pub confidence: f64,
    pub source: String,
    pub updated_at: i64,
}

/// Consent record for a specific data-collection or behaviour consent key.
#[derive(Debug, Clone)]
pub struct OperatorProfileConsentRow {
    pub consent_key: String,
    pub granted: bool,
    pub updated_at: i64,
}

/// An event in the operator-profile event log (answer, inference, prompt, skip, deferral).
#[derive(Debug, Clone)]
pub struct OperatorProfileEventRow {
    pub id: String,
    pub event_type: String,
    pub field_key: Option<String>,
    pub value_json: Option<String>,
    pub source: String,
    pub metadata_json: Option<String>,
    pub created_at: i64,
}

/// A scheduled check-in for the operator profile questionnaire flow.
#[derive(Debug, Clone)]
pub struct OperatorProfileCheckinRow {
    pub id: String,
    pub kind: String,
    pub scheduled_at: i64,
    pub shown_at: Option<i64>,
    pub status: String,
    pub response_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillVariantFitnessHistoryRow {
    pub id: String,
    pub variant_id: String,
    pub recorded_at: i64,
    pub outcome: String,
    pub fitness_score: f64,
}

/// Row type for heartbeat_history table. Per D-12.
#[derive(Debug, Clone)]
pub struct HeartbeatHistoryRow {
    pub id: String,
    pub cycle_timestamp: i64,
    pub checks_json: String,
    pub synthesis_json: Option<String>,
    pub actionable: bool,
    pub digest_text: Option<String>,
    pub llm_tokens_used: i64,
    pub duration_ms: i64,
    pub status: String,
}

/// Row type for action_audit table. Per D-06/TRNS-03.
#[derive(Debug, Clone)]
pub struct AuditEntryRow {
    pub id: String,
    pub timestamp: i64,
    pub action_type: String,
    pub summary: String,
    pub explanation: Option<String>,
    pub confidence: Option<f64>,
    pub confidence_band: Option<String>,
    pub causal_trace_id: Option<String>,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub raw_data_json: Option<String>,
}

/// Row type for memory_tombstones table (Phase 5 — consolidation).
#[derive(Debug, Clone)]
pub struct MemoryTombstoneRow {
    pub id: String,
    pub target: String,
    pub original_content: String,
    pub fact_key: Option<String>,
    pub replaced_by: Option<String>,
    pub replaced_at: i64,
    pub source_kind: String,
    pub provenance_id: Option<String>,
    pub created_at: i64,
}

/// Row type for execution_traces query results (Phase 5 — consolidation).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionTraceRow {
    pub id: String,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub task_type: Option<String>,
    pub outcome: Option<String>,
    pub quality_score: Option<f64>,
    pub tool_sequence_json: Option<String>,
    pub metrics_json: Option<String>,
    pub duration_ms: Option<i64>,
    pub tokens_used: Option<i64>,
    pub created_at: i64,
}

/// Row type for context_archive query results (Phase 5 — cross-session continuity).
#[derive(Debug, Clone)]
pub struct ContextArchiveRow {
    pub id: String,
    pub thread_id: String,
    pub original_role: Option<String>,
    pub compressed_content: String,
    pub summary: Option<String>,
    pub relevance_score: f64,
    pub token_count_original: i64,
    pub token_count_compressed: i64,
    pub metadata_json: Option<String>,
    pub archived_at: i64,
    pub last_accessed_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct WhatsAppProviderStateRow {
    pub provider_id: String,
    pub linked_phone: Option<String>,
    pub auth_json: Option<String>,
    pub metadata_json: Option<String>,
    pub last_reset_at: Option<u64>,
    pub last_linked_at: Option<u64>,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct GatewayReplayCursorRow {
    pub platform: String,
    pub channel_id: String,
    pub cursor_value: String,
    pub cursor_type: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct GatewayHealthSnapshotRow {
    pub platform: String,
    pub state_json: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMessageCursor {
    pub created_at: i64,
    pub message_id: String,
}

impl AgentMessageCursor {
    pub fn from_message(message: &AgentDbMessage) -> Self {
        Self {
            created_at: message.created_at,
            message_id: message.id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentMessageSpan {
    Range {
        start: AgentMessageCursor,
        end: AgentMessageCursor,
    },
    LastTurn {
        message: AgentMessageCursor,
    },
}

impl AgentMessageSpan {
    pub fn legacy_label(&self) -> String {
        match self {
            Self::Range { start, end } => format!("{}..{}", start.message_id, end.message_id),
            Self::LastTurn { .. } => "last_turn".to_string(),
        }
    }

    pub fn end_cursor(&self) -> AgentMessageCursor {
        match self {
            Self::Range { end, .. } => end.clone(),
            Self::LastTurn { message } => message.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryDistillationProgressRow {
    pub source_thread_id: String,
    pub last_processed_cursor: AgentMessageCursor,
    pub last_processed_span: Option<AgentMessageSpan>,
    pub last_run_at_ms: i64,
    pub updated_at_ms: i64,
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryDistillationLogRow {
    pub id: i64,
    pub source_thread_id: String,
    pub source_message_range: Option<String>,
    pub source_message_span: Option<AgentMessageSpan>,
    pub distilled_fact: String,
    pub target_file: String,
    pub category: String,
    pub confidence: f64,
    pub created_at_ms: i64,
    pub applied_to_memory: bool,
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgePassLogRow {
    pub id: i64,
    pub agent_id: String,
    pub period_start_ms: i64,
    pub period_end_ms: i64,
    pub traces_analyzed: i64,
    pub patterns_found: i64,
    pub hints_applied: i64,
    pub hints_logged: i64,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HandoffRoutingRow {
    pub id: String,
    pub to_specialist_id: String,
    pub capability_tags_json: Option<String>,
    pub routing_method: String,
    pub routing_score: f64,
    pub fallback_used: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalRecordRow {
    pub approval_id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub thread_id: Option<String>,
    pub transition_kind: String,
    pub stage_id: Option<String>,
    pub scope_summary: Option<String>,
    pub target_scope_json: String,
    pub constraints_json: String,
    pub risk_class: String,
    pub rationale_json: String,
    pub policy_fingerprint: String,
    pub requested_at: u64,
    pub resolved_at: Option<u64>,
    pub expires_at: Option<u64>,
    pub resolution: Option<String>,
    pub invalidated_at: Option<u64>,
    pub invalidation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalInboxEntry {
    pub approval_id: String,
    pub session_id: String,
    pub workspace_id: Option<String>,
    pub execution_id: String,
    pub request: ManagedCommandRequest,
    pub approval: ApprovalPayload,
    pub policy_fingerprint: String,
    pub constraints: Vec<GovernanceConstraint>,
    pub transition_kind: TransitionKind,
    pub requested_at: u64,
    pub expires_at: Option<u64>,
    pub gateway_surface: Option<String>,
    pub gateway_channel: Option<String>,
    pub gateway_thread: Option<String>,
    pub rendered_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceEvaluationRow {
    pub id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub thread_id: Option<String>,
    pub transition_kind: String,
    pub input_json: String,
    pub verdict_json: String,
    pub policy_fingerprint: String,
    pub created_at: u64,
}

pub struct ProvenanceEventRecord<'a> {
    pub event_type: &'a str,
    pub summary: &'a str,
    pub details: &'a serde_json::Value,
    pub agent_id: &'a str,
    pub goal_run_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub thread_id: Option<&'a str>,
    pub approval_id: Option<&'a str>,
    pub causal_trace_id: Option<&'a str>,
    pub compliance_mode: &'a str,
    pub sign: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillVariantRecord {
    pub variant_id: String,
    pub skill_name: String,
    pub variant_name: String,
    pub relative_path: String,
    pub parent_variant_id: Option<String>,
    pub version: String,
    pub context_tags: Vec<String>,
    pub use_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub fitness_score: f64,
    pub status: String,
    pub last_used_at: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenePoolEntry {
    pub parent_a: String,
    pub parent_b: String,
    pub offspring_id: String,
    pub lifecycle_state: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillVariantFitnessSnapshot {
    pub recorded_at: u64,
    pub fitness_score: f64,
    pub use_count: u32,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillVariantInspection {
    pub record: SkillVariantRecord,
    pub lifecycle_summary: String,
    pub selection_summary: String,
    pub selected_for_context: bool,
    pub fitness_score: f64,
    pub fitness_snapshot: SkillVariantFitnessSnapshot,
    pub fitness_history: Vec<SkillVariantFitnessHistoryRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillVariantPage {
    pub variants: Vec<SkillVariantRecord>,
    pub next_cursor: Option<String>,
}

impl SkillVariantRecord {
    pub fn success_rate(&self) -> f64 {
        let attempts = self.success_count + self.failure_count;
        if attempts == 0 {
            0.5
        } else {
            self.success_count as f64 / attempts as f64
        }
    }

    pub fn is_canonical(&self) -> bool {
        self.variant_name == "canonical"
    }
}

const SKILL_ARCHIVE_MIN_USES: u32 = 3;
const SKILL_ARCHIVE_MAX_IDLE_SECS: u64 = 90 * 24 * 60 * 60;
const SKILL_ARCHIVE_SUCCESS_RATE_THRESHOLD: f64 = 0.30;
const SKILL_PROMOTION_MIN_USES: u32 = 3;
const SKILL_PROMOTION_MIN_SUCCESS_COUNT: u32 = 2;
const SKILL_PROMOTION_SUCCESS_RATE_THRESHOLD: f64 = 0.80;
const SKILL_PROMOTION_MARGIN: f64 = 0.15;
const SKILL_MERGE_MIN_USES: u32 = 4;
const SKILL_MERGE_SUCCESS_RATE_THRESHOLD: f64 = 0.85;
const SKILL_MERGE_SIMILARITY_THRESHOLD: f64 = 0.40;

pub struct SkillVariantConsultationRecord<'a> {
    pub usage_id: &'a str,
    pub variant_id: &'a str,
    pub thread_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub goal_run_id: Option<&'a str>,
    pub context_tags: &'a [String],
    pub consulted_at: u64,
}

#[derive(Debug, Clone)]
pub struct PendingSkillVariantConsultation {
    pub variant_id: String,
    pub context_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryProvenanceRelationship {
    pub related_entry_id: String,
    pub relation_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryProvenanceReportEntry {
    pub id: String,
    pub target: String,
    pub mode: String,
    pub source_kind: String,
    pub content: String,
    pub fact_keys: Vec<String>,
    pub thread_id: Option<String>,
    pub task_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub created_at: u64,
    pub age_days: f64,
    pub confidence: f64,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_scheme: Option<String>,
    pub hash_valid: bool,
    pub signature_valid: bool,
    pub relationships: Vec<MemoryProvenanceRelationship>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryProvenanceReport {
    pub total_entries: usize,
    pub target_filter: Option<String>,
    pub summary_by_target: BTreeMap<String, usize>,
    pub summary_by_source: BTreeMap<String, usize>,
    pub summary_by_status: BTreeMap<String, usize>,
    pub entries: Vec<MemoryProvenanceReportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceLogEntry {
    pub sequence: u64,
    pub timestamp: u64,
    pub event_type: String,
    pub summary: String,
    pub details: serde_json::Value,
    pub prev_hash: String,
    pub entry_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_scheme: Option<String>,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causal_trace_id: Option<String>,
    pub compliance_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceReportEntry {
    pub sequence: u64,
    pub timestamp: u64,
    pub event_type: String,
    pub summary: String,
    pub signature_present: bool,
    pub signature_scheme: Option<String>,
    pub agent_id: String,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub thread_id: Option<String>,
    pub approval_id: Option<String>,
    pub causal_trace_id: Option<String>,
    pub compliance_mode: String,
    pub hash_valid: bool,
    pub signature_valid: bool,
    pub chain_valid: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceReport {
    pub total_entries: usize,
    pub signed_entries: usize,
    pub valid_hash_entries: usize,
    pub valid_signature_entries: usize,
    pub valid_chain_entries: usize,
    pub summary_by_event: BTreeMap<String, usize>,
    pub entries: Vec<ProvenanceReportEntry>,
}

#[derive(Debug, Clone)]
pub struct SubagentMetrics {
    pub task_id: String,
    pub parent_task_id: Option<String>,
    pub thread_id: Option<String>,
    pub tool_calls_total: i64,
    pub tool_calls_succeeded: i64,
    pub tool_calls_failed: i64,
    pub tokens_consumed: i64,
    pub context_budget_tokens: Option<i64>,
    pub progress_rate: f64,
    pub last_progress_at: Option<u64>,
    pub stuck_score: f64,
    pub health_state: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Default, Clone)]
pub struct AgentMessagePatch {
    pub content: Option<String>,
    pub reasoning: Option<Option<String>>,
    pub tool_calls_json: Option<Option<String>>,
    pub metadata_json: Option<Option<String>>,
    pub provider: Option<Option<String>>,
    pub model: Option<Option<String>>,
    pub input_tokens: Option<Option<i64>>,
    pub output_tokens: Option<Option<i64>>,
    pub total_tokens: Option<Option<i64>>,
    pub cost_usd: Option<Option<f64>>,
}

mod agent_wakeups;
mod audit;
mod browser_profiles;
mod cache;
mod causal_traces;
mod checkpoints;
mod cognitive_resonance;
mod command_log;
mod consolidation;
mod context_archive;
mod core;
pub(crate) use core::{run_db_sync_loop, run_wal_checkpoint_loop};
mod critique;
pub(crate) mod db;
mod debate;
mod dream_state;
mod embedding_queue;
pub use embedding_queue::{SemanticBackfillResult, SemanticIndexStatus};
mod event_log;
mod event_triggers;
mod external_runtime_profiles;
mod gateway_state;
mod goal_runs;
mod governance;
mod harness;
mod implicit_feedback;
mod integrity_helpers;
mod memory_graph;
mod metacognition;
mod offloaded_payloads;
pub(crate) mod operator_questions;
mod operator_profile;
pub(crate) mod prompt_queue;
mod protocol_candidates;
mod protocol_registry;
mod provenance;
mod routines;
mod row_mapping;
mod schema;
pub(crate) use schema::init_schema_on_connection;
mod schema_helpers;
mod schema_migrations;
mod schema_sql;
mod thread_search;
pub(crate) use thread_search::ThreadSearchCandidate;
mod schema_sql_extra;
mod semantic_documents;
pub(crate) use semantic_documents::SemanticDocumentSyncSummary;
mod skill_generation;
mod skill_reads;
pub(crate) use skill_reads::ThreadSkillRead;
mod guideline_catalog;
pub(crate) use guideline_catalog::GuidelineDocumentRecord;
mod statistics;
mod temporal_foresight;
#[cfg(feature = "lancedb-vector")]
pub(crate) mod vector_index;
pub(crate) use skill_generation::page_skill_variants;
mod skill_metadata;
pub(crate) use skill_metadata::{derive_skill_metadata, DerivedSkillMetadata};
mod skill_tagging;
mod skill_variants;
mod task_enums;
mod tasks;
mod thread_structural_memory;
mod threads;
mod workspaces;

use integrity_helpers::*;
pub use memory_graph::{MemoryEdgeRow, MemoryGraphNeighborRow, MemoryNodeRow};
pub use offloaded_payloads::OffloadedPayloadMetadataRow;
use row_mapping::*;
use skill_metadata::*;
use skill_tagging::*;
use task_enums::*;

pub(crate) fn now_ts() -> u64 {
    integrity_helpers::now_ts()
}

pub(crate) const SQLITE_SAFE_VARIABLE_LIMIT: usize = 900;

pub(crate) fn sqlite_in_chunk_size(bind_copies: usize) -> usize {
    (SQLITE_SAFE_VARIABLE_LIMIT / bind_copies.max(1)).max(1)
}

#[cfg(test)]
mod tests;
