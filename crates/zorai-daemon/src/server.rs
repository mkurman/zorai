#![allow(dead_code)]

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::SinkExt;
use futures::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc};
use tokio_util::codec::Framed;
use zorai_protocol::{
    ClientMessage, DaemonMessage, GatewayBootstrapPayload, GatewayConnectionStatus,
    GatewayContinuityState, GatewayCursorState, GatewayHealthState, GatewayIncomingEvent,
    GatewayProviderBootstrap, GatewayRegistration, GatewayRouteMode, GatewayRouteModeState,
    GatewayThreadBindingState, SessionInfo, GATEWAY_IPC_PROTOCOL_VERSION,
};

use crate::agent::skill_community::{
    export_skill, import_community_skill, prepare_publish, unpack_skill, ImportResult,
};
use crate::agent::skill_registry::{to_community_entry, RegistryClient};
use crate::agent::AgentEngine;
use crate::session_manager::SessionManager;

pub(crate) const AGENT_DB_THREAD_DETAIL_MESSAGE_WINDOW: usize = 64;

/// Outcome of a dispatch helper that may also terminate the connection loop.
pub(crate) enum DispatchOutcome {
    NotMatched,
    Continue,
    Terminate,
}

pub(crate) const CONNECTION_WRITE_BUFFER: usize = 4096;

#[derive(Clone)]
pub(crate) struct ConnectionWriter {
    tx: mpsc::Sender<DaemonMessage>,
}

impl ConnectionWriter {
    pub(crate) fn new(tx: mpsc::Sender<DaemonMessage>) -> Self {
        Self { tx }
    }

    /// Must be non-blocking: the read loop calls this on the same task that
    /// reads client messages. A blocking `await` here would deadlock the
    /// connection if the socket is slow.
    pub(crate) async fn send(&mut self, msg: DaemonMessage) -> std::io::Result<()> {
        use mpsc::error::TrySendError;
        match self.tx.try_send(msg) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                tracing::warn!(
                    buffer = CONNECTION_WRITE_BUFFER,
                    "daemon writer queue full — TUI not draining; closing connection"
                );
                Err(std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "daemon writer queue full",
                ))
            }
            Err(TrySendError::Closed(_)) => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "daemon writer task closed",
            )),
        }
    }
}

/// Outcome of the pre-dispatch loop step that drains async events before
/// reading the next ClientMessage.
pub(crate) enum PreDispatchOutcome {
    Terminate,
    Msg(Option<ClientMessage>),
}
pub(crate) const AGENT_DB_THREAD_LIST_WINDOW: usize = 128;
pub(crate) const AGENT_DB_INDEX_LIST_WINDOW: usize = 128;

#[path = "server/helpers_part1.rs"]
mod helpers_part1;
#[path = "server/helpers_part2.rs"]
mod helpers_part2;
#[path = "server/operation_retention.rs"]
mod operation_retention;
#[path = "server/operations.rs"]
mod operations;
#[path = "server/operations_registry.rs"]
mod operations_registry;
#[path = "server/subsystem_metrics.rs"]
mod subsystem_metrics;
#[path = "server/subsystem_queue.rs"]
mod subsystem_queue;
pub(crate) use helpers_part1::*;
pub(crate) use helpers_part2::*;
pub(crate) use operation_retention::*;
pub(crate) use operations::*;
pub(crate) use operations_registry::*;
pub(crate) use subsystem_metrics::*;
pub(crate) use subsystem_queue::*;

fn is_unknown_operator_profile_session_error(error: &anyhow::Error) -> bool {
    error
        .to_string()
        .contains("unknown operator profile session:")
}

#[cfg(test)]
mod tests;

#[path = "server/post_tests.rs"]
mod post_tests;
pub(crate) use post_tests::*;

#[path = "server/connection_pre_dispatch.rs"]
mod connection_pre_dispatch;
#[path = "server/dispatch_part1.rs"]
mod dispatch_part1;
#[path = "server/dispatch_part10.rs"]
mod dispatch_part10;
#[path = "server/dispatch_part11.rs"]
mod dispatch_part11;
#[path = "server/dispatch_part2.rs"]
mod dispatch_part2;
#[path = "server/dispatch_part3.rs"]
mod dispatch_part3;
#[path = "server/dispatch_part4.rs"]
mod dispatch_part4;
#[path = "server/dispatch_part5.rs"]
mod dispatch_part5;
#[path = "server/dispatch_part6.rs"]
mod dispatch_part6;
#[path = "server/dispatch_part7.rs"]
mod dispatch_part7;
#[path = "server/dispatch_part8.rs"]
mod dispatch_part8;
#[path = "server/dispatch_part9.rs"]
mod dispatch_part9;

#[derive(Clone)]
pub(crate) struct StartupReadiness {
    ready: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
}

impl StartupReadiness {
    fn new(initial_ready: bool) -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(initial_ready)),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    fn mark_ready(&self) {
        self.ready.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    async fn wait_until_ready(&self) {
        if self.ready.load(Ordering::Acquire) {
            return;
        }

        loop {
            let notified = self.notify.notified();
            if self.ready.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}

fn client_message_requires_startup_readiness(msg: &ClientMessage) -> bool {
    !matches!(
        msg,
        ClientMessage::Ping
            | ClientMessage::AgentGetConfig
            | ClientMessage::AgentGetMlflowTracingStatus
            | ClientMessage::AgentGetGatewayConfig
            | ClientMessage::AgentGetEffectiveConfigState
            | ClientMessage::AgentGetProviderCatalog
            | ClientMessage::GatewayRegister { .. }
            | ClientMessage::GatewayAck { .. }
            | ClientMessage::GatewayIncomingEvent { .. }
            | ClientMessage::GatewayCursorUpdate { .. }
            | ClientMessage::GatewayThreadBindingUpdate { .. }
            | ClientMessage::GatewayRouteModeUpdate { .. }
            | ClientMessage::GatewaySendResult { .. }
            | ClientMessage::GatewayHealthUpdate { .. }
            | ClientMessage::SpawnSession { .. }
            | ClientMessage::CloneSession { .. }
            | ClientMessage::AttachSession { .. }
            | ClientMessage::DetachSession { .. }
            | ClientMessage::AgentSubscribe
            | ClientMessage::AgentUnsubscribe
            | ClientMessage::AgentDeclareAsyncCommandCapability { .. }
            | ClientMessage::AgentGetOperationStatus { .. }
            | ClientMessage::AgentHandoffThread { .. }
            | ClientMessage::AgentGetHealthStatus
    )
}

/// Returns a short, log-safe label for a `ClientMessage` variant. Used in
/// dispatch tracing so we can see *which* message arrived without dumping the
/// full payload into the log line.
fn client_message_variant_name(msg: &ClientMessage) -> &'static str {
    use ClientMessage::*;
    match msg {
        Ping => "Ping",
        AgentSubscribe => "AgentSubscribe",
        AgentUnsubscribe => "AgentUnsubscribe",
        AgentGetConfig => "AgentGetConfig",
        AgentGetMlflowTracingStatus => "AgentGetMlflowTracingStatus",
        AgentTestMlflowTracingConnection => "AgentTestMlflowTracingConnection",
        AgentSendMlflowTracingTestTrace => "AgentSendMlflowTracingTestTrace",
        AgentListMlflowTracingHeaders => "AgentListMlflowTracingHeaders",
        AgentSetMlflowTracingHeader { .. } => "AgentSetMlflowTracingHeader",
        AgentDeleteMlflowTracingHeader { .. } => "AgentDeleteMlflowTracingHeader",
        AgentEnqueuePrompt { .. } => "AgentEnqueuePrompt",
        AgentListPromptQueue { .. } => "AgentListPromptQueue",
        AgentUpdateQueuedPrompt { .. } => "AgentUpdateQueuedPrompt",
        AgentCancelQueuedPrompt { .. } => "AgentCancelQueuedPrompt",
        AgentSendQueuedPromptNow { .. } => "AgentSendQueuedPromptNow",
        AgentGetGatewayConfig => "AgentGetGatewayConfig",
        AgentGetEffectiveConfigState => "AgentGetEffectiveConfigState",
        AgentGetProviderCatalog => "AgentGetProviderCatalog",
        AgentGetProviderAuthStates => "AgentGetProviderAuthStates",
        AgentListSubAgents => "AgentListSubAgents",
        AgentListThreads { .. } => "AgentListThreads",
        AgentListTasks => "AgentListTasks",
        AgentListGoalRuns { .. } => "AgentListGoalRuns",
        AgentGetThread { .. } => "AgentGetThread",
        AgentForceCompact { .. } => "AgentForceCompact",
        AgentRequestConciergeWelcome => "AgentRequestConciergeWelcome",
        AgentDismissConciergeWelcome => "AgentDismissConciergeWelcome",
        AgentGetConciergeConfig => "AgentGetConciergeConfig",
        AgentSetConciergeConfig { .. } => "AgentSetConciergeConfig",
        AgentDeclareAsyncCommandCapability { .. } => "AgentDeclareAsyncCommandCapability",
        AgentGetOperationStatus { .. } => "AgentGetOperationStatus",
        AgentHandoffThread { .. } => "AgentHandoffThread",
        AgentHeartbeatGetItems => "AgentHeartbeatGetItems",
        ListAgentEvents { .. } => "ListAgentEvents",
        UpsertAgentEvent { .. } => "UpsertAgentEvent",
        MarkAllNotificationsRead => "MarkAllNotificationsRead",
        ArchiveReadNotifications => "ArchiveReadNotifications",
        PluginList { .. } => "PluginList",
        PluginListCommands { .. } => "PluginListCommands",
        _ => "<other>",
    }
}

fn client_surface_can_write_thread(
    expected_surface: zorai_protocol::ClientSurface,
    actual_surface: zorai_protocol::ClientSurface,
) -> bool {
    expected_surface == actual_surface
        || matches!(
            (expected_surface, actual_surface),
            (
                zorai_protocol::ClientSurface::Tui,
                zorai_protocol::ClientSurface::Electron
            ) | (
                zorai_protocol::ClientSurface::Electron,
                zorai_protocol::ClientSurface::Tui
            )
        )
}

#[cfg(test)]
mod client_surface_authorization_tests {
    use super::*;

    #[test]
    fn local_operator_surfaces_can_write_each_others_threads() {
        assert!(client_surface_can_write_thread(
            zorai_protocol::ClientSurface::Tui,
            zorai_protocol::ClientSurface::Electron
        ));
        assert!(client_surface_can_write_thread(
            zorai_protocol::ClientSurface::Electron,
            zorai_protocol::ClientSurface::Tui
        ));
    }
}

#[cfg(test)]
mod startup_readiness_tests {
    use super::*;

    #[test]
    fn config_requests_are_not_blocked_on_startup_readiness() {
        assert!(!client_message_requires_startup_readiness(
            &ClientMessage::AgentGetConfig
        ));
        assert!(!client_message_requires_startup_readiness(
            &ClientMessage::AgentGetEffectiveConfigState
        ));
        assert!(!client_message_requires_startup_readiness(
            &ClientMessage::AgentGetGatewayConfig
        ));
    }

    #[tokio::test]
    async fn cloned_startup_readiness_observes_mark_ready() {
        let readiness = StartupReadiness::new(false);
        let cloned = readiness.clone();

        readiness.mark_ready();

        tokio::time::timeout(
            std::time::Duration::from_millis(50),
            cloned.wait_until_ready(),
        )
        .await
        .expect("cloned startup readiness should observe mark_ready");
    }
}

pub(crate) async fn handle_connection<S>(
    stream: S,
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
    startup_readiness: StartupReadiness,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    use zorai_protocol::DaemonCodec;
    let framed = Framed::new(stream, DaemonCodec);
    let (mut sink, mut stream) = framed.split();

    let (write_tx, mut write_rx) = mpsc::channel::<DaemonMessage>(CONNECTION_WRITE_BUFFER);
    let _writer_task = tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            let started = std::time::Instant::now();
            if let Err(error) = sink.send(msg).await {
                tracing::warn!(
                    error = %error,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "daemon connection writer task aborting on send error"
                );
                return;
            }
            let elapsed_ms = started.elapsed().as_millis() as u64;
            if elapsed_ms > 1000 {
                tracing::warn!(
                    elapsed_ms,
                    "daemon writer: sink.send blocked >1s — likely TUI not draining socket"
                );
            }
        }
    });
    let mut framed = ConnectionWriter::new(write_tx);

    let mut attached_rxs: Vec<(
        zorai_protocol::SessionId,
        broadcast::Receiver<DaemonMessage>,
    )> = Vec::new();
    let mut client_agent_threads: HashSet<String> = HashSet::new();
    let mut last_concierge_welcome_fingerprint: Option<String> = None;
    let mut agent_event_rx: Option<broadcast::Receiver<crate::agent::types::AgentEvent>> = None;
    let mut background_daemon_queues = BackgroundSubsystemQueues::new();
    let mut background_daemon_pending = BackgroundPendingCounts::default();
    let mut whatsapp_link_rx: Option<
        broadcast::Receiver<crate::agent::types::WhatsAppLinkRuntimeEvent>,
    > = None;
    let mut gateway_ipc_rx: Option<mpsc::UnboundedReceiver<DaemonMessage>> = None;
    let mut whatsapp_link_subscriber_guard = WhatsAppLinkSubscriberGuard::new(agent.clone());
    let mut whatsapp_link_snapshot_replayed = false;
    let mut gateway_connection_state = GatewayConnectionState::Unregistered;

    loop {
        let msg = match connection_pre_dispatch::pre_dispatch(
            &agent,
            &mut stream,
            &mut framed,
            &plugin_manager,
            &mut attached_rxs,
            &mut client_agent_threads,
            &mut last_concierge_welcome_fingerprint,
            &mut agent_event_rx,
            &mut background_daemon_queues,
            &mut background_daemon_pending,
            &mut whatsapp_link_rx,
            &mut whatsapp_link_snapshot_replayed,
            &mut gateway_ipc_rx,
            &gateway_connection_state,
        )
        .await?
        {
            PreDispatchOutcome::Terminate => return Ok(()),
            PreDispatchOutcome::Msg(m) => m,
        };

        if let Some(msg) = msg {
            tracing::info!(
                variant = client_message_variant_name(&msg),
                "dispatch: incoming client message"
            );
            if client_message_requires_startup_readiness(&msg) {
                startup_readiness.wait_until_ready().await;
            }
            match dispatch_part1::dispatch_part1(
                &msg,
                &agent,
                &mut framed,
                &manager,
                &mut attached_rxs,
                &mut gateway_connection_state,
                &mut gateway_ipc_rx,
            )
            .await?
            {
                DispatchOutcome::NotMatched => {}
                DispatchOutcome::Continue => continue,
                DispatchOutcome::Terminate => return Ok(()),
            }
            match dispatch_part2::dispatch_part2(
                &msg,
                &agent,
                &mut framed,
                &manager,
                &mut attached_rxs,
            )
            .await?
            {
                DispatchOutcome::NotMatched => {}
                DispatchOutcome::Continue => continue,
                DispatchOutcome::Terminate => return Ok(()),
            }
            if dispatch_part3::dispatch_part3(
                &msg,
                &agent,
                &mut framed,
                &manager,
                &mut client_agent_threads,
            )
            .await?
            {
                continue;
            }
            if dispatch_part4::dispatch_part4(
                &msg,
                &agent,
                &mut framed,
                &mut background_daemon_queues,
                &mut background_daemon_pending,
                &mut client_agent_threads,
                &mut agent_event_rx,
            )
            .await?
            {
                continue;
            }
            if dispatch_part5::dispatch_part5(&msg, &agent, &mut framed).await? {
                continue;
            }
            if dispatch_part6::dispatch_part6(
                &msg,
                &agent,
                &mut framed,
                &mut background_daemon_queues,
                &mut background_daemon_pending,
            )
            .await?
            {
                continue;
            }
            if dispatch_part7::dispatch_part7(
                &msg,
                &agent,
                &mut framed,
                &mut background_daemon_queues,
                &mut background_daemon_pending,
                &mut last_concierge_welcome_fingerprint,
                &mut agent_event_rx,
            )
            .await?
            {
                continue;
            }
            if dispatch_part8::dispatch_part8(
                &msg,
                &agent,
                &mut framed,
                &mut background_daemon_queues,
                &mut background_daemon_pending,
            )
            .await?
            {
                continue;
            }
            if dispatch_part9::dispatch_part9(
                &msg,
                &agent,
                &mut framed,
                &mut background_daemon_queues,
                &mut background_daemon_pending,
                &plugin_manager,
            )
            .await?
            {
                continue;
            }
            if dispatch_part10::dispatch_whatsapp_link(
                &msg,
                &agent,
                &mut framed,
                &mut whatsapp_link_subscriber_guard,
                &mut whatsapp_link_rx,
                &mut whatsapp_link_snapshot_replayed,
            )
            .await?
            {
                continue;
            }
            if dispatch_part11::dispatch_part11(&msg, &agent, &mut framed).await? {
                continue;
            }
        }
    }
}
