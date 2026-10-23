use crate::client::ThreadDetailChunkBuffer;
use crate::client::{ClientEvent, DaemonClient};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use zorai_protocol::DaemonMessage;
impl DaemonClient {
    pub(crate) async fn handle_daemon_message(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
        thread_detail_chunks: &mut Option<ThreadDetailChunkBuffer>,
    ) -> bool {
        match message {
            message @ (DaemonMessage::AgentEvent { .. }
            | DaemonMessage::AgentThreadList { .. }
            | DaemonMessage::AgentThreadDetail { .. }
            | DaemonMessage::AgentThreadHandoffResult { .. }
            | DaemonMessage::AgentGatewayChats { .. }
            | DaemonMessage::AgentThreadSearchResults { .. }
            | DaemonMessage::AgentThreadDetailChunk { .. }
            | DaemonMessage::AgentTaskList { .. }
            | DaemonMessage::AgentGoalRunList { .. }
            | DaemonMessage::AgentGoalRunStarted { .. }
            | DaemonMessage::AgentGoalRunDetail { .. }
            | DaemonMessage::AgentGoalRunControlled { .. }
            | DaemonMessage::AgentCheckpointList { .. }
            | DaemonMessage::AgentCheckpointRestored { .. }
            | DaemonMessage::AgentTodoDetail { .. }
            | DaemonMessage::AgentWorkContextDetail { .. }
            | DaemonMessage::GitDiff { .. }
            | DaemonMessage::FilePreview { .. }
            | DaemonMessage::DataQueryResult { .. }
            | DaemonMessage::AgentConfigResponse { .. }
            | DaemonMessage::AgentExternalRuntimeMigrationResult { .. }
            | DaemonMessage::AgentModelsResponse { .. }
            | DaemonMessage::AgentHeartbeatItems { .. }
            | DaemonMessage::AgentEventRows { .. }
            | DaemonMessage::AgentDbMessageAck { .. }
            | DaemonMessage::AgentThreadExported { .. }
            | DaemonMessage::SessionSpawned { .. }
            | DaemonMessage::ApprovalRequired { .. }
            | DaemonMessage::AgentTaskApprovalRules { .. }
            | DaemonMessage::ApprovalResolved { .. }
            | DaemonMessage::AgentWorkspaceSettings { .. }
            | DaemonMessage::AgentWorkspaceSettingsList { .. }
            | DaemonMessage::AgentWorkspaceTaskList { .. }
            | DaemonMessage::AgentWorkspaceTaskUpdated { .. }
            | DaemonMessage::AgentWorkspaceTaskDeleted { .. }
            | DaemonMessage::AgentWorkspaceNoticeList { .. }
            | DaemonMessage::AgentWorkspaceError { .. }
            | DaemonMessage::AgentPromptQueue { .. }) => {
                Self::handle_thread_workspace_daemon_messages(
                    message,
                    event_tx,
                    thread_detail_chunks,
                )
                .await
            }
            message @ (DaemonMessage::AgentProviderAuthStates { .. }
            | DaemonMessage::AgentProviderCatalog { .. }
            | DaemonMessage::AgentOpenAICodexAuthStatus { .. }
            | DaemonMessage::AgentOpenAICodexAuthLoginResult { .. }
            | DaemonMessage::AgentOpenAICodexAuthLogoutResult { .. }
            | DaemonMessage::AgentProviderValidation { .. }
            | DaemonMessage::AgentSubAgentList { .. }
            | DaemonMessage::AgentSubAgentUpdated { .. }
            | DaemonMessage::AgentSubAgentRemoved { .. }
            | DaemonMessage::AgentConciergeConfig { .. }
            | DaemonMessage::PluginListResult { .. }
            | DaemonMessage::PluginGetResult { .. }
            | DaemonMessage::PluginSettingsResult { .. }
            | DaemonMessage::PluginTestConnectionResult { .. }
            | DaemonMessage::PluginActionResult { .. }
            | DaemonMessage::PluginCommandsResult { .. }
            | DaemonMessage::PluginOAuthUrl { .. }
            | DaemonMessage::PluginOAuthComplete { .. }) => {
                Self::handle_provider_plugin_daemon_messages(message, event_tx).await
            }
            message @ (DaemonMessage::AgentWhatsAppLinkStatus { .. }
            | DaemonMessage::AgentThreadMessagePinResult { .. }
            | DaemonMessage::AgentWhatsAppLinkQr { .. }
            | DaemonMessage::AgentWhatsAppLinked { .. }
            | DaemonMessage::AgentWhatsAppLinkError { .. }
            | DaemonMessage::AgentWhatsAppLinkDisconnected { .. }
            | DaemonMessage::AgentExplanation { .. }
            | DaemonMessage::AgentDivergentSessionStarted { .. }
            | DaemonMessage::AgentDivergentSession { .. }
            | DaemonMessage::AgentStatusResponse { .. }
            | DaemonMessage::AgentStatisticsResponse { .. }
            | DaemonMessage::AgentPromptInspection { .. }
            | DaemonMessage::AgentOperatorProfileSessionStarted { .. }
            | DaemonMessage::AgentOperatorProfileQuestion { .. }
            | DaemonMessage::AgentOperatorProfileProgress { .. }
            | DaemonMessage::AgentOperatorProfileSummary { .. }
            | DaemonMessage::AgentOperatorModel { .. }
            | DaemonMessage::AgentOperatorModelReset { .. }
            | DaemonMessage::AgentCollaborationSessions { .. }
            | DaemonMessage::AgentCollaborationVoteResult { .. }
            | DaemonMessage::AgentGeneratedTools { .. }
            | DaemonMessage::AgentSpeechToTextResult { .. }
            | DaemonMessage::AgentTextToSpeechResult { .. }
            | DaemonMessage::AgentGenerateImageResult { .. }
            | DaemonMessage::AgentOperatorProfileSessionCompleted { .. }
            | DaemonMessage::AgentError { .. }
            | DaemonMessage::AgentTierChanged { .. }
            | DaemonMessage::SemanticIndexRepairResult { .. }
            | DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. }
            | DaemonMessage::Error { .. }) => {
                Self::handle_activity_profile_gateway_daemon_messages(message, event_tx).await
            }
            DaemonMessage::AgentQuestionAnswered {
                question_id,
                answer,
            } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorQuestionResolved {
                        question_id,
                        answer,
                    })
                    .await;
            }
            DaemonMessage::DatabaseSyncResult { ok, message } => {
                let _ = event_tx
                    .send(ClientEvent::DatabaseSyncResult { ok, message })
                    .await;
            }
            DaemonMessage::DatabaseBackendState {
                backend,
                sync_url,
                has_token,
                seeded_at,
            } => {
                let _ = event_tx
                    .send(ClientEvent::DatabaseBackendState {
                        backend,
                        sync_url,
                        has_token,
                        seeded_at,
                    })
                    .await;
            }
            DaemonMessage::Pong => {}
            DaemonMessage::AgentConciergeWelcomeDismissed => {}
            DaemonMessage::AgentAsyncCommandCapabilityAck { capability } => {
                debug!(
                    target: "zorai_tui::client",
                    ?capability,
                    "received AgentAsyncCommandCapabilityAck"
                );
            }
            DaemonMessage::OperationAccepted {
                operation_id,
                kind,
                dedup,
                revision,
            } => {
                debug!(
                    target: "zorai_tui::client",
                    operation_id = %operation_id,
                    kind = %kind,
                    dedup = ?dedup,
                    revision,
                    "received OperationAccepted"
                );
            }
            other => {
                #[cfg(debug_assertions)]
                {
                    warn!(
                        target: "zorai_tui::client",
                        message = ?other,
                        "unhandled DaemonMessage variant — dispatcher needs an explicit arm",
                    );
                }
                #[cfg(not(debug_assertions))]
                {
                    debug!(
                        target: "zorai_tui::client",
                        message = ?other,
                        "ignoring unhandled daemon message",
                    );
                }
            }
        }

        true
    }
}
