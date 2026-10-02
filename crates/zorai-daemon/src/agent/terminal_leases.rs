//! Auto-reclaim of terminals allocated for agents and subagents.
//!
//! Agent-owned lanes are cloned for parallel/isolated work and attached in the
//! GUI. Without a lease they live for the whole daemon process. This registry
//! closes them when the owning task finishes, the owning thread is deleted, or
//! the lane sits idle with no active command.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::MissedTickBehavior;
use zorai_protocol::SessionId;

use super::internal_event::InternalAgentEvent;
use super::task_prompt::now_millis;
use super::types::AgentEvent;
use super::AgentEngine;

pub(crate) const CLOSE_AGENT_TERMINAL_COMMAND: &str = "close_agent_terminal";
pub(crate) const AGENT_TERMINAL_IDLE_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const AGENT_TERMINAL_SWEEP_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub(crate) struct AgentTerminalLease {
    pub session_id: SessionId,
    pub workspace_id: Option<String>,
    pub owner_task_id: Option<String>,
    pub owner_thread_id: Option<String>,
    pub last_idle_at: u64,
    pub busy_since: Option<u64>,
}

impl AgentTerminalLease {
    pub(crate) fn new(
        session_id: SessionId,
        workspace_id: Option<String>,
        owner_task_id: Option<String>,
        owner_thread_id: Option<String>,
        now_ms: u64,
    ) -> Self {
        Self {
            session_id,
            workspace_id,
            owner_task_id: owner_task_id.filter(|value| !value.is_empty()),
            owner_thread_id: owner_thread_id.filter(|value| !value.is_empty()),
            last_idle_at: now_ms,
            busy_since: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentTerminalCloseReason {
    OwnerTaskFinished,
    Idle,
    SessionGone,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AgentTerminalSweepView {
    pub now_ms: u64,
    pub idle_timeout_ms: u64,
    pub owner_task_is_terminal: bool,
    pub session_alive: bool,
    pub has_active_command: bool,
}

pub(crate) const AGENT_WORKSPACE_NAME_PREFIX: &str = "Agent - ";

pub(crate) fn is_reclaimable_agent_workspace_name(name: &str) -> bool {
    name.starts_with(AGENT_WORKSPACE_NAME_PREFIX)
}

pub(crate) fn note_reclaimable_agent_workspaces(
    known: &mut HashSet<String>,
    topology: Option<&zorai_protocol::WorkspaceTopology>,
) {
    let Some(topology) = topology else {
        return;
    };
    for workspace in &topology.workspaces {
        if workspace.agent_owned || is_reclaimable_agent_workspace_name(&workspace.workspace_name) {
            known.insert(workspace.workspace_id.clone());
        }
    }
}

pub(crate) fn should_reclaim_unleased_session(
    has_workspace: bool,
    agent_owned_workspace: bool,
    tracked_by_ui: bool,
    busy: bool,
    leased: bool,
    last_activity_at_ms: u64,
    now_ms: u64,
    idle_timeout_ms: u64,
) -> bool {
    if !has_workspace || busy || leased {
        return false;
    }
    if !agent_owned_workspace && tracked_by_ui {
        return false;
    }
    now_ms.saturating_sub(last_activity_at_ms) >= idle_timeout_ms
}

pub(crate) fn close_reason_for_lease(
    lease: &AgentTerminalLease,
    view: &AgentTerminalSweepView,
) -> Option<AgentTerminalCloseReason> {
    if !view.session_alive {
        return Some(AgentTerminalCloseReason::SessionGone);
    }
    if view.owner_task_is_terminal {
        return Some(AgentTerminalCloseReason::OwnerTaskFinished);
    }
    if view.has_active_command {
        return None;
    }
    if view.now_ms.saturating_sub(lease.last_idle_at) >= view.idle_timeout_ms {
        return Some(AgentTerminalCloseReason::Idle);
    }
    None
}

fn reason_label(reason: AgentTerminalCloseReason) -> &'static str {
    match reason {
        AgentTerminalCloseReason::OwnerTaskFinished => "owner_finished",
        AgentTerminalCloseReason::Idle => "idle",
        AgentTerminalCloseReason::SessionGone => "session_gone",
    }
}

impl AgentEngine {
    pub(super) fn spawn_agent_terminal_lease_worker(engine: Arc<Self>) {
        tokio::spawn(async move {
            let mut events = engine.internal_event_tx.subscribe();
            let mut interval = tokio::time::interval(AGENT_TERMINAL_SWEEP_INTERVAL);
            let mut known_agent_workspaces = HashSet::new();
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
            interval.tick().await;
            loop {
                tokio::select! {
                    event = events.recv() => {
                        match event {
                            Ok(InternalAgentEvent::TaskTerminal { task_id, .. }) => {
                                engine.release_agent_terminals_for_task(&task_id).await;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    _ = interval.tick() => {
                        engine.sweep_agent_terminal_leases().await;
                        note_reclaimable_agent_workspaces(
                            &mut known_agent_workspaces,
                            engine.session_manager.read_workspace_topology().as_ref(),
                        );
                        engine
                            .reclaim_idle_agent_sessions(&known_agent_workspaces, now_millis())
                            .await;
                    }
                }
            }
        });
    }

    pub(crate) async fn register_agent_terminal_lease(&self, lease: AgentTerminalLease) {
        let session_id = lease.session_id;
        self.agent_terminal_leases
            .lock()
            .await
            .insert(session_id, lease);
        tracing::info!(%session_id, "registered agent-owned terminal lease");
    }

    pub(crate) async fn release_agent_terminals_for_task(&self, task_id: &str) {
        let session_ids = {
            let leases = self.agent_terminal_leases.lock().await;
            leases
                .values()
                .filter(|lease| lease.owner_task_id.as_deref() == Some(task_id))
                .map(|lease| lease.session_id)
                .collect::<Vec<_>>()
        };
        self.release_agent_terminal_sessions(
            &session_ids,
            AgentTerminalCloseReason::OwnerTaskFinished,
        )
        .await;
    }

    pub(crate) async fn release_agent_terminals_for_thread(&self, thread_id: &str) {
        let session_ids = {
            let leases = self.agent_terminal_leases.lock().await;
            leases
                .values()
                .filter(|lease| lease.owner_thread_id.as_deref() == Some(thread_id))
                .map(|lease| lease.session_id)
                .collect::<Vec<_>>()
        };
        self.release_agent_terminal_sessions(
            &session_ids,
            AgentTerminalCloseReason::OwnerTaskFinished,
        )
        .await;
    }

    pub(crate) async fn reclaim_idle_agent_sessions(
        &self,
        agent_workspace_ids: &HashSet<String>,
        now_ms: u64,
    ) {
        if agent_workspace_ids.is_empty() {
            return;
        }
        let leased = {
            let leases = self.agent_terminal_leases.lock().await;
            leases.keys().copied().collect::<HashSet<_>>()
        };
        let topology = self.session_manager.read_workspace_topology();
        let snapshots = self.session_manager.session_activity_snapshots().await;
        let mut due = Vec::new();
        for snapshot in snapshots {
            let Some(workspace_id) = snapshot.workspace_id.as_deref() else {
                continue;
            };
            let agent_owned = agent_workspace_ids.contains(workspace_id);
            let tracked_by_ui = match topology.as_ref() {
                Some(topology) => topology
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == workspace_id),
                None => true,
            };
            if should_reclaim_unleased_session(
                true,
                agent_owned,
                tracked_by_ui,
                snapshot.busy,
                leased.contains(&snapshot.id),
                snapshot.last_activity_at_ms,
                now_ms,
                AGENT_TERMINAL_IDLE_TIMEOUT_MS,
            ) {
                due.push(snapshot.id);
            }
        }
        if !due.is_empty() {
            self.release_agent_terminal_sessions(&due, AgentTerminalCloseReason::Idle)
                .await;
        }
    }

    pub(crate) async fn sweep_agent_terminal_leases(&self) {
        let now_ms = now_millis();
        let snapshots = self.session_manager.session_activity_snapshots().await;
        let sessions: HashMap<SessionId, (bool, u64)> = snapshots
            .iter()
            .map(|session| {
                (
                    session.id,
                    (session.has_active_command, session.last_activity_at_ms),
                )
            })
            .collect();

        let task_terminal_by_id = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .map(|task| (task.id.clone(), task.status.is_terminal()))
                .collect::<HashMap<_, _>>()
        };

        let mut due = Vec::new();
        {
            let mut leases = self.agent_terminal_leases.lock().await;
            for lease in leases.values_mut() {
                let session_state = sessions.get(&lease.session_id).copied();
                let session_alive = session_state.is_some();
                let has_active_command = session_state.map(|(busy, _)| busy).unwrap_or(false);
                let last_activity_at_ms = session_state.map(|(_, activity)| activity).unwrap_or(0);
                if has_active_command {
                    if lease.busy_since.is_none() {
                        lease.busy_since = Some(now_ms);
                    }
                } else if lease.busy_since.take().is_some() {
                    lease.last_idle_at = now_ms;
                } else if last_activity_at_ms > lease.last_idle_at {
                    lease.last_idle_at = last_activity_at_ms;
                }

                let owner_task_is_terminal = match lease.owner_task_id.as_deref() {
                    Some(task_id) => task_terminal_by_id.get(task_id).copied().unwrap_or(true),
                    None => false,
                };
                let view = AgentTerminalSweepView {
                    now_ms,
                    idle_timeout_ms: AGENT_TERMINAL_IDLE_TIMEOUT_MS,
                    owner_task_is_terminal,
                    session_alive,
                    has_active_command,
                };
                if let Some(reason) = close_reason_for_lease(lease, &view) {
                    due.push((lease.session_id, reason));
                }
            }
        }

        for (session_id, reason) in due {
            self.release_agent_terminal_sessions(&[session_id], reason)
                .await;
        }
    }

    async fn release_agent_terminal_sessions(
        &self,
        session_ids: &[SessionId],
        reason: AgentTerminalCloseReason,
    ) {
        for session_id in session_ids {
            let lease = self.agent_terminal_leases.lock().await.remove(session_id);
            let workspace_id = lease.and_then(|lease| lease.workspace_id);
            if let Err(error) = self.session_manager.kill(*session_id).await {
                tracing::debug!(
                    %session_id,
                    %error,
                    "agent terminal kill skipped or failed during reclaim"
                );
            }
            let _ = self.event_tx.send(AgentEvent::WorkspaceCommand {
                command: CLOSE_AGENT_TERMINAL_COMMAND.to_string(),
                args: serde_json::json!({
                    "session_id": session_id.to_string(),
                    "workspace_id": workspace_id,
                    "reason": reason_label(reason),
                }),
            });
            tracing::info!(
                %session_id,
                reason = reason_label(reason),
                "reclaimed agent-owned terminal"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::super::types::{AgentConfig, TaskStatus};
    use super::*;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;
    use tokio::time::{timeout, Duration};

    fn lease_at(last_idle_at: u64) -> AgentTerminalLease {
        AgentTerminalLease {
            session_id: SessionId::nil(),
            workspace_id: Some("ws".to_string()),
            owner_task_id: Some("task-1".to_string()),
            owner_thread_id: Some("thread-1".to_string()),
            last_idle_at,
            busy_since: None,
        }
    }

    fn view(
        now_ms: u64,
        owner_task_is_terminal: bool,
        session_alive: bool,
        has_active_command: bool,
    ) -> AgentTerminalSweepView {
        AgentTerminalSweepView {
            now_ms,
            idle_timeout_ms: AGENT_TERMINAL_IDLE_TIMEOUT_MS,
            owner_task_is_terminal,
            session_alive,
            has_active_command,
        }
    }

    #[test]
    fn finished_owner_closes_even_when_command_is_still_running() {
        let lease = lease_at(1);
        assert_eq!(
            close_reason_for_lease(&lease, &view(2, true, true, true)),
            Some(AgentTerminalCloseReason::OwnerTaskFinished)
        );
    }

    #[test]
    fn in_progress_owner_keeps_a_busy_terminal() {
        let lease = lease_at(1);
        assert_eq!(
            close_reason_for_lease(
                &lease,
                &view(AGENT_TERMINAL_IDLE_TIMEOUT_MS * 4, false, true, true)
            ),
            None
        );
    }

    #[test]
    fn unused_lane_closes_after_idle_timeout() {
        let lease = lease_at(1);
        assert_eq!(
            close_reason_for_lease(
                &lease,
                &view(1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS, false, true, false)
            ),
            Some(AgentTerminalCloseReason::Idle)
        );
    }

    #[test]
    fn unused_lane_stays_before_idle_timeout() {
        let lease = lease_at(1);
        assert_eq!(
            close_reason_for_lease(
                &lease,
                &view(1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS - 1, false, true, false)
            ),
            None
        );
    }

    #[test]
    fn unleased_idle_agent_session_is_reclaimed() {
        assert!(should_reclaim_unleased_session(
            true,
            true,
            true,
            false,
            false,
            1,
            1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS,
            AGENT_TERMINAL_IDLE_TIMEOUT_MS,
        ));
    }

    #[test]
    fn detached_idle_session_is_reclaimed_when_the_ui_no_longer_tracks_it() {
        assert!(should_reclaim_unleased_session(
            true,
            false,
            false,
            false,
            false,
            1,
            1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS,
            AGENT_TERMINAL_IDLE_TIMEOUT_MS,
        ));
    }

    #[test]
    fn unleased_agent_session_stays_while_busy_or_recent_or_operator_owned() {
        assert!(!should_reclaim_unleased_session(
            true,
            true,
            true,
            true,
            false,
            1,
            1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS,
            AGENT_TERMINAL_IDLE_TIMEOUT_MS,
        ));
        assert!(!should_reclaim_unleased_session(
            true,
            true,
            true,
            false,
            false,
            1,
            1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS - 1,
            AGENT_TERMINAL_IDLE_TIMEOUT_MS,
        ));
        assert!(!should_reclaim_unleased_session(
            true,
            false,
            true,
            false,
            false,
            1,
            1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS,
            AGENT_TERMINAL_IDLE_TIMEOUT_MS,
        ));
        assert!(!should_reclaim_unleased_session(
            true,
            true,
            true,
            false,
            true,
            1,
            1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS,
            AGENT_TERMINAL_IDLE_TIMEOUT_MS,
        ));
        assert!(!should_reclaim_unleased_session(
            false,
            false,
            false,
            false,
            false,
            1,
            1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS,
            AGENT_TERMINAL_IDLE_TIMEOUT_MS,
        ));
    }

    #[test]
    fn topology_remembers_agent_workspaces_by_flag_or_name() {
        let topology = zorai_protocol::WorkspaceTopology {
            workspaces: vec![
                zorai_protocol::WorkspaceTopologyEntry {
                    workspace_id: "ws-flag".into(),
                    workspace_name: "Renamed".into(),
                    agent_owned: true,
                    last_activity_at: 0,
                    surfaces: Vec::new(),
                },
                zorai_protocol::WorkspaceTopologyEntry {
                    workspace_id: "ws-name".into(),
                    workspace_name: "Agent - training".into(),
                    agent_owned: false,
                    last_activity_at: 0,
                    surfaces: Vec::new(),
                },
                zorai_protocol::WorkspaceTopologyEntry {
                    workspace_id: "ws-operator".into(),
                    workspace_name: "Default".into(),
                    agent_owned: false,
                    last_activity_at: 0,
                    surfaces: Vec::new(),
                },
            ],
        };
        let mut known = HashSet::new();
        note_reclaimable_agent_workspaces(&mut known, Some(&topology));
        assert!(known.contains("ws-flag"));
        assert!(known.contains("ws-name"));
        assert!(!known.contains("ws-operator"));
        note_reclaimable_agent_workspaces(&mut known, None);
        assert!(known.contains("ws-flag"));
    }

    #[test]
    fn dead_session_is_reclaimed() {
        let lease = lease_at(1);
        assert_eq!(
            close_reason_for_lease(&lease, &view(2, false, false, false)),
            Some(AgentTerminalCloseReason::SessionGone)
        );
    }

    #[test]
    fn close_reason_does_not_depend_on_task_status_enum_layout() {
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());
        assert!(!TaskStatus::InProgress.is_terminal());
    }

    async fn wait_for_close_command(
        events: &mut tokio::sync::broadcast::Receiver<AgentEvent>,
        session_id: SessionId,
    ) -> serde_json::Value {
        timeout(Duration::from_secs(2), async {
            loop {
                match events.recv().await.expect("close event") {
                    AgentEvent::WorkspaceCommand { command, args }
                        if command == CLOSE_AGENT_TERMINAL_COMMAND
                            && args.get("session_id").and_then(|value| value.as_str())
                                == Some(&session_id.to_string()) =>
                    {
                        return args;
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("timed out waiting for close_agent_terminal")
    }

    #[tokio::test]
    async fn releasing_owner_task_emits_close_and_drops_the_lease() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine =
            super::super::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let session_id = SessionId::from_u128(42);
        let mut events = engine.subscribe();
        engine
            .register_agent_terminal_lease(AgentTerminalLease::new(
                session_id,
                Some("ws-agent".to_string()),
                Some("task-owner".to_string()),
                Some("thread-owner".to_string()),
                10,
            ))
            .await;
        engine.release_agent_terminals_for_task("task-owner").await;
        let args = wait_for_close_command(&mut events, session_id).await;
        assert_eq!(
            args.get("reason").and_then(|value| value.as_str()),
            Some("owner_finished")
        );
        assert!(engine.agent_terminal_leases.lock().await.is_empty());
    }

    #[tokio::test]
    async fn sweep_reclaims_leases_whose_session_is_already_gone() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine =
            super::super::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let session_id = SessionId::from_u128(43);
        let mut events = engine.subscribe();
        engine
            .register_agent_terminal_lease(AgentTerminalLease::new(
                session_id,
                Some("ws-agent".to_string()),
                None,
                Some("thread-chat".to_string()),
                10,
            ))
            .await;
        engine.sweep_agent_terminal_leases().await;
        let args = wait_for_close_command(&mut events, session_id).await;
        assert_eq!(
            args.get("reason").and_then(|value| value.as_str()),
            Some("session_gone")
        );
        assert!(engine.agent_terminal_leases.lock().await.is_empty());
    }

    #[tokio::test]
    async fn sweep_reclaims_idle_unleased_session_in_an_agent_workspace() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine =
            super::super::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let (session_id, _rx) = engine
            .session_manager
            .spawn(
                Some("/bin/cat".to_string()),
                None,
                Some("ws-agent".to_string()),
                None,
                80,
                24,
            )
            .await
            .expect("spawn idle session");
        assert!(
            engine
                .session_manager
                .set_session_last_activity_for_test(session_id, 1)
                .await
        );
        let mut events = engine.subscribe();
        let known = HashSet::from(["ws-agent".to_string()]);
        engine
            .reclaim_idle_agent_sessions(&known, 1 + AGENT_TERMINAL_IDLE_TIMEOUT_MS)
            .await;
        let args = wait_for_close_command(&mut events, session_id).await;
        assert_eq!(
            args.get("reason").and_then(|value| value.as_str()),
            Some("idle")
        );
        assert!(engine
            .session_manager
            .list()
            .await
            .iter()
            .all(|session| session.id != session_id));
    }
}
