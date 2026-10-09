use anyhow::Result;
use futures::SinkExt;
use tokio_util::codec::Framed;
use zorai_protocol::{ClientMessage, ZoraiCodec};

use super::emit_agent_event;
use crate::client::agent_protocol::AgentBridgeCommand;

pub(super) async fn handle_line<T>(framed: &mut Framed<T, ZoraiCodec>, line: &str) -> Result<bool>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let command: AgentBridgeCommand = match serde_json::from_str(line) {
        Ok(cmd) => cmd,
        Err(error) => {
            let err_json =
                serde_json::json!({"type":"error","message":format!("invalid command: {error}")});
            emit_agent_event(&err_json.to_string())?;
            return Ok(true);
        }
    };

    match command {
        AgentBridgeCommand::SendMessage {
            thread_id,
            content,
            session_id,
            context_messages,
            content_blocks_json,
            target_agent_id,
            workspace_context,
        } => {
            let context_messages_json =
                context_messages.and_then(|msgs| serde_json::to_string(&msgs).ok());
            let workspace_context_json =
                workspace_context.and_then(|context| serde_json::to_string(&context).ok());
            framed
                .send(ClientMessage::AgentSendMessage {
                    thread_id,
                    content,
                    session_id,
                    context_messages_json,
                    content_blocks_json,
                    client_surface: Some(zorai_protocol::ClientSurface::Electron),
                    target_agent_id,
                    workspace_context_json,
                })
                .await?;
        }
        AgentBridgeCommand::InternalDelegate {
            thread_id,
            target_agent_id,
            content,
            session_id,
        } => {
            framed
                .send(ClientMessage::AgentInternalDelegate {
                    thread_id,
                    target_agent_id,
                    content,
                    session_id,
                    client_surface: Some(zorai_protocol::ClientSurface::Electron),
                })
                .await?;
        }
        AgentBridgeCommand::ThreadParticipantCommand {
            thread_id,
            target_agent_id,
            action,
            instruction,
            session_id,
        } => {
            framed
                .send(ClientMessage::AgentThreadParticipantCommand {
                    thread_id,
                    target_agent_id,
                    action,
                    instruction,
                    session_id,
                    client_surface: Some(zorai_protocol::ClientSurface::Electron),
                })
                .await?;
        }
        AgentBridgeCommand::StopStream { thread_id } => {
            framed
                .send(ClientMessage::AgentStopStream { thread_id })
                .await?;
        }
        AgentBridgeCommand::RetryStreamNow { thread_id } => {
            framed
                .send(ClientMessage::AgentRetryStreamNow { thread_id })
                .await?;
        }
        AgentBridgeCommand::ListThreads {
            agent_filter,
            include_internal,
        } => {
            framed
                .send(ClientMessage::AgentListThreads {
                    limit: None,
                    offset: None,
                    include_internal,
                    agent_filter,
                })
                .await?;
        }
        AgentBridgeCommand::GetThread {
            thread_id,
            message_limit,
            message_offset,
            collapse_tool_calls,
        } => {
            framed
                .send(ClientMessage::AgentGetThread {
                    thread_id,
                    message_limit,
                    message_offset,
                    collapse_tool_calls,
                })
                .await?;
        }
        AgentBridgeCommand::DeleteThread { thread_id } => {
            framed
                .send(ClientMessage::AgentDeleteThread { thread_id })
                .await?;
        }
        AgentBridgeCommand::PinThreadMessageForCompaction {
            thread_id,
            message_id,
        } => {
            framed
                .send(ClientMessage::AgentPinThreadMessageForCompaction {
                    thread_id,
                    message_id,
                })
                .await?;
        }
        AgentBridgeCommand::UnpinThreadMessageForCompaction {
            thread_id,
            message_id,
        } => {
            framed
                .send(ClientMessage::AgentUnpinThreadMessageForCompaction {
                    thread_id,
                    message_id,
                })
                .await?;
        }
        AgentBridgeCommand::MessageFeedback {
            thread_id,
            message_id,
            reaction,
        } => {
            let reaction = reaction.as_deref().and_then(|value| match value {
                "up" => Some(zorai_protocol::Reaction::Up),
                "down" => Some(zorai_protocol::Reaction::Down),
                _ => None,
            });
            framed
                .send(ClientMessage::AgentMessageFeedback {
                    thread_id,
                    message_id,
                    absolute_message_index: None,
                    reaction,
                })
                .await?;
        }
        AgentBridgeCommand::AddTask {
            title,
            description,
            priority,
            command,
            session_id,
            scheduled_at,
            dependencies,
        } => {
            framed
                .send(ClientMessage::AgentAddTask {
                    title,
                    description,
                    priority: priority.unwrap_or_else(|| "normal".into()),
                    command,
                    session_id,
                    scheduled_at,
                    dependencies,
                })
                .await?;
        }
        AgentBridgeCommand::CancelTask { task_id } => {
            framed
                .send(ClientMessage::AgentCancelTask { task_id })
                .await?;
        }
        AgentBridgeCommand::GetOperationStatus { operation_id } => {
            framed
                .send(ClientMessage::AgentGetOperationStatus { operation_id })
                .await?;
        }
        AgentBridgeCommand::ListTasks => {
            framed.send(ClientMessage::AgentListTasks).await?;
        }
        AgentBridgeCommand::ListRuns { parent_thread_id } => {
            let message = match parent_thread_id {
                Some(parent_thread_id) if !parent_thread_id.trim().is_empty() => {
                    ClientMessage::AgentListRunsForParentThread {
                        parent_thread_id: parent_thread_id.trim().to_string(),
                    }
                }
                _ => ClientMessage::AgentListRuns,
            };
            framed.send(message).await?;
        }
        AgentBridgeCommand::GetRun { run_id } => {
            framed.send(ClientMessage::AgentGetRun { run_id }).await?;
        }
        AgentBridgeCommand::HandoffThread {
            thread_id,
            action,
            target_agent_id,
            reason,
            summary,
            requested_by,
            session_id,
        } => {
            framed
                .send(ClientMessage::AgentHandoffThread {
                    thread_id,
                    action,
                    target_agent_id,
                    reason,
                    summary,
                    requested_by,
                    session_id,
                    client_surface: Some(zorai_protocol::ClientSurface::Electron),
                })
                .await?;
        }
        AgentBridgeCommand::StartGoalRun {
            goal,
            title,
            thread_id,
            session_id,
            priority,
            client_request_id,
            autonomy_level,
            requires_approval,
        } => {
            framed
                .send(ClientMessage::AgentStartGoalRun {
                    goal,
                    title,
                    thread_id,
                    session_id,
                    priority,
                    client_request_id,
                    launch_assignments: Vec::new(),
                    autonomy_level,
                    client_surface: Some(zorai_protocol::ClientSurface::Electron),
                    target_agent_id: None,
                    requires_approval,
                })
                .await?;
        }
        AgentBridgeCommand::ListGoalRuns => {
            framed
                .send(ClientMessage::AgentListGoalRuns {
                    limit: None,
                    offset: None,
                })
                .await?;
        }
        AgentBridgeCommand::GetGoalRun { goal_run_id } => {
            framed
                .send(ClientMessage::AgentGetGoalRun { goal_run_id })
                .await?;
        }
        AgentBridgeCommand::ControlGoalRun {
            goal_run_id,
            action,
            step_index,
            payload_json,
        } => {
            framed
                .send(ClientMessage::AgentControlGoalRun {
                    goal_run_id,
                    action,
                    step_index,
                    payload_json,
                })
                .await?;
        }
        AgentBridgeCommand::ListWorkspaceSettings => {
            framed
                .send(ClientMessage::AgentListWorkspaceSettings)
                .await?;
        }
        AgentBridgeCommand::GetWorkspaceSettings { workspace_id } => {
            framed
                .send(ClientMessage::AgentGetWorkspaceSettings { workspace_id })
                .await?;
        }
        AgentBridgeCommand::SetWorkspaceOperator {
            workspace_id,
            operator,
        } => {
            framed
                .send(ClientMessage::AgentSetWorkspaceOperator {
                    workspace_id,
                    operator,
                })
                .await?;
        }
        AgentBridgeCommand::SetWorkspaceRepoMonitor {
            workspace_id,
            repo_monitor_enabled,
            repo_monitor_include_dirs,
            repo_monitor_exclude_dirs,
        } => {
            framed
                .send(ClientMessage::AgentSetWorkspaceRepoMonitor {
                    workspace_id,
                    repo_monitor_enabled,
                    repo_monitor_include_dirs,
                    repo_monitor_exclude_dirs,
                })
                .await?;
        }
        AgentBridgeCommand::CreateWorkspaceTask { request } => {
            framed
                .send(ClientMessage::AgentCreateWorkspaceTask { request })
                .await?;
        }
        AgentBridgeCommand::ListWorkspaceTasks {
            workspace_id,
            include_deleted,
        } => {
            framed
                .send(ClientMessage::AgentListWorkspaceTasks {
                    workspace_id,
                    include_deleted,
                })
                .await?;
        }
        AgentBridgeCommand::GetWorkspaceTask { task_id } => {
            framed
                .send(ClientMessage::AgentGetWorkspaceTask { task_id })
                .await?;
        }
        AgentBridgeCommand::UpdateWorkspaceTask { task_id, update } => {
            framed
                .send(ClientMessage::AgentUpdateWorkspaceTask { task_id, update })
                .await?;
        }
        AgentBridgeCommand::MoveWorkspaceTask { request } => {
            framed
                .send(ClientMessage::AgentMoveWorkspaceTask { request })
                .await?;
        }
        AgentBridgeCommand::RunWorkspaceTask { task_id } => {
            framed
                .send(ClientMessage::AgentRunWorkspaceTask { task_id })
                .await?;
        }
        AgentBridgeCommand::PauseWorkspaceTask { task_id } => {
            framed
                .send(ClientMessage::AgentPauseWorkspaceTask { task_id })
                .await?;
        }
        AgentBridgeCommand::StopWorkspaceTask { task_id } => {
            framed
                .send(ClientMessage::AgentStopWorkspaceTask { task_id })
                .await?;
        }
        AgentBridgeCommand::DeleteWorkspaceTask { task_id } => {
            framed
                .send(ClientMessage::AgentDeleteWorkspaceTask { task_id })
                .await?;
        }
        AgentBridgeCommand::SubmitWorkspaceReview { review } => {
            framed
                .send(ClientMessage::AgentSubmitWorkspaceReview { review })
                .await?;
        }
        AgentBridgeCommand::ListWorkspaceNotices {
            workspace_id,
            task_id,
        } => {
            framed
                .send(ClientMessage::AgentListWorkspaceNotices {
                    workspace_id,
                    task_id,
                })
                .await?;
        }
        AgentBridgeCommand::ListTodos => {
            framed.send(ClientMessage::AgentListTodos).await?;
        }
        AgentBridgeCommand::GetTodos { thread_id } => {
            framed
                .send(ClientMessage::AgentGetTodos { thread_id })
                .await?;
        }
        AgentBridgeCommand::GetWorkContext { thread_id } => {
            framed
                .send(ClientMessage::AgentGetWorkContext { thread_id })
                .await?;
        }
        AgentBridgeCommand::GetFileOperationSnapshot { operation_id } => {
            framed
                .send(ClientMessage::AgentGetFileOperationSnapshot { operation_id })
                .await?;
        }
        AgentBridgeCommand::RevertFileOperation { operation_id } => {
            framed
                .send(ClientMessage::AgentRevertFileOperation { operation_id })
                .await?;
        }
        AgentBridgeCommand::GetThreadWorkspaceContext { thread_id } => {
            framed
                .send(ClientMessage::AgentGetThreadWorkspaceContext { thread_id })
                .await?;
        }
        AgentBridgeCommand::SetThreadWorkspaceContext { thread_id, context } => {
            framed
                .send(ClientMessage::AgentSetThreadWorkspaceContext {
                    thread_id,
                    context_json: serde_json::to_string(&context)?,
                })
                .await?;
        }
        AgentBridgeCommand::SpawnSubagent { thread_id, args } => {
            framed
                .send(ClientMessage::AgentSpawnSubagent {
                    thread_id,
                    args_json: serde_json::to_string(&args)?,
                })
                .await?;
        }
        AgentBridgeCommand::GetGitDiff {
            repo_path,
            file_path,
        } => {
            framed
                .send(ClientMessage::GetGitDiff {
                    repo_path,
                    file_path,
                })
                .await?;
        }
        AgentBridgeCommand::GetFilePreview { path, max_bytes } => {
            framed
                .send(ClientMessage::GetFilePreview { path, max_bytes })
                .await?;
        }
        AgentBridgeCommand::GetConfig => {
            framed.send(ClientMessage::AgentGetConfig).await?;
        }
        AgentBridgeCommand::GetMlflowTracingStatus => {
            framed
                .send(ClientMessage::AgentGetMlflowTracingStatus)
                .await?;
        }
        AgentBridgeCommand::TestMlflowTracingConnection => {
            framed
                .send(ClientMessage::AgentTestMlflowTracingConnection)
                .await?;
        }
        AgentBridgeCommand::SendMlflowTracingTestTrace => {
            framed
                .send(ClientMessage::AgentSendMlflowTracingTestTrace)
                .await?;
        }
        AgentBridgeCommand::ListMlflowTracingHeaders => {
            framed
                .send(ClientMessage::AgentListMlflowTracingHeaders)
                .await?;
        }
        AgentBridgeCommand::SetMlflowTracingHeader { name, value } => {
            framed
                .send(ClientMessage::AgentSetMlflowTracingHeader { name, value })
                .await?;
        }
        AgentBridgeCommand::DeleteMlflowTracingHeader { name } => {
            framed
                .send(ClientMessage::AgentDeleteMlflowTracingHeader { name })
                .await?;
        }
        AgentBridgeCommand::ExternalRuntimeMigrationStatus => {
            framed
                .send(ClientMessage::AgentExternalRuntimeMigrationStatus)
                .await?;
        }
        AgentBridgeCommand::ExternalRuntimeMigrationPreview {
            runtime,
            config_path,
        } => {
            framed
                .send(ClientMessage::AgentExternalRuntimeMigrationPreview {
                    runtime,
                    config_path,
                })
                .await?;
        }
        AgentBridgeCommand::ExternalRuntimeMigrationApply {
            runtime,
            config_path,
            conflict_policy,
        } => {
            framed
                .send(ClientMessage::AgentExternalRuntimeMigrationApply {
                    runtime,
                    config_path,
                    conflict_policy,
                })
                .await?;
        }
        AgentBridgeCommand::ExternalRuntimeMigrationReport { runtime, limit } => {
            framed
                .send(ClientMessage::AgentExternalRuntimeMigrationReport { runtime, limit })
                .await?;
        }
        AgentBridgeCommand::ExternalRuntimeMigrationShadowRun { runtime } => {
            framed
                .send(ClientMessage::AgentExternalRuntimeMigrationShadowRun { runtime })
                .await?;
        }
        AgentBridgeCommand::GetGatewayConfig => {
            framed.send(ClientMessage::AgentGetGatewayConfig).await?;
        }
        AgentBridgeCommand::SetConfigItem {
            key_path,
            value_json,
        } => {
            framed
                .send(ClientMessage::AgentSetConfigItem {
                    key_path,
                    value_json,
                })
                .await?;
        }
        AgentBridgeCommand::SetProviderModel { provider_id, model } => {
            framed
                .send(ClientMessage::AgentSetProviderModel { provider_id, model })
                .await?;
        }
        AgentBridgeCommand::FetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        } => {
            framed
                .send(ClientMessage::AgentFetchModels {
                    provider_id,
                    base_url,
                    api_key,
                    output_modalities,
                })
                .await?;
        }
        AgentBridgeCommand::SetTargetAgentProviderModel {
            target_agent_id,
            provider_id,
            model,
        } => {
            framed
                .send(ClientMessage::AgentSetTargetAgentProviderModel {
                    target_agent_id,
                    provider_id,
                    model,
                })
                .await?;
        }
        AgentBridgeCommand::SetTargetAgentReasoningEffort {
            target_agent_id,
            reasoning_effort,
        } => {
            framed
                .send(ClientMessage::AgentSetTargetAgentReasoningEffort {
                    target_agent_id,
                    reasoning_effort,
                })
                .await?;
        }
        AgentBridgeCommand::SetTargetAgentContextWindow {
            target_agent_id,
            context_window_tokens,
        } => {
            framed
                .send(ClientMessage::AgentSetTargetAgentContextWindow {
                    target_agent_id,
                    context_window_tokens,
                })
                .await?;
        }
        AgentBridgeCommand::HeartbeatGetItems => {
            framed.send(ClientMessage::AgentHeartbeatGetItems).await?;
        }
        AgentBridgeCommand::HeartbeatSetItems { items_json } => {
            framed
                .send(ClientMessage::AgentHeartbeatSetItems { items_json })
                .await?;
        }
        AgentBridgeCommand::ResolveTaskApproval {
            approval_id,
            decision,
        } => {
            framed
                .send(ClientMessage::AgentResolveTaskApproval {
                    approval_id,
                    decision,
                })
                .await?;
        }
        AgentBridgeCommand::ValidateProvider {
            provider_id,
            base_url,
            api_key,
            auth_source,
        } => {
            framed
                .send(ClientMessage::AgentValidateProvider {
                    provider_id,
                    base_url,
                    api_key,
                    auth_source,
                })
                .await?;
        }
        AgentBridgeCommand::LoginProvider {
            provider_id,
            api_key,
            base_url,
        } => {
            framed
                .send(ClientMessage::AgentLoginProvider {
                    provider_id,
                    api_key,
                    base_url,
                })
                .await?;
        }
        AgentBridgeCommand::LogoutProvider { provider_id } => {
            framed
                .send(ClientMessage::AgentLogoutProvider { provider_id })
                .await?;
        }
        AgentBridgeCommand::GetProviderAuthStates => {
            framed
                .send(ClientMessage::AgentGetProviderAuthStates)
                .await?;
        }
        AgentBridgeCommand::GetProviderCatalog => {
            framed.send(ClientMessage::AgentGetProviderCatalog).await?;
        }
        AgentBridgeCommand::GetOpenAICodexAuthStatus => {
            framed
                .send(ClientMessage::AgentGetOpenAICodexAuthStatus)
                .await?;
        }
        AgentBridgeCommand::LoginOpenAICodex => {
            framed.send(ClientMessage::AgentLoginOpenAICodex).await?;
        }
        AgentBridgeCommand::LogoutOpenAICodex => {
            framed.send(ClientMessage::AgentLogoutOpenAICodex).await?;
        }
        AgentBridgeCommand::SetSubAgent { sub_agent_json } => {
            framed
                .send(ClientMessage::AgentSetSubAgent { sub_agent_json })
                .await?;
        }
        AgentBridgeCommand::RemoveSubAgent { sub_agent_id } => {
            framed
                .send(ClientMessage::AgentRemoveSubAgent { sub_agent_id })
                .await?;
        }
        AgentBridgeCommand::ListSubAgents => {
            framed.send(ClientMessage::AgentListSubAgents).await?;
        }
        AgentBridgeCommand::GetConciergeConfig => {
            framed.send(ClientMessage::AgentGetConciergeConfig).await?;
        }
        AgentBridgeCommand::SetConciergeConfig { config_json } => {
            framed
                .send(ClientMessage::AgentSetConciergeConfig { config_json })
                .await?;
        }
        AgentBridgeCommand::DismissConciergeWelcome => {
            framed
                .send(ClientMessage::AgentDismissConciergeWelcome)
                .await?;
        }
        AgentBridgeCommand::RequestConciergeWelcome => {
            framed
                .send(ClientMessage::AgentRequestConciergeWelcome)
                .await?;
        }
        AgentBridgeCommand::AuditDismiss { entry_id } => {
            framed
                .send(ClientMessage::AuditDismiss { entry_id })
                .await?;
        }
        AgentBridgeCommand::QueryAudits {
            action_types,
            since,
            limit,
        } => {
            framed
                .send(ClientMessage::AuditQuery {
                    action_types,
                    since,
                    limit,
                })
                .await?;
        }
        AgentBridgeCommand::GetProvenanceReport { limit } => {
            framed
                .send(ClientMessage::AgentGetProvenanceReport { limit })
                .await?;
        }
        AgentBridgeCommand::GetMemoryProvenanceReport { target, limit } => {
            framed
                .send(ClientMessage::AgentGetMemoryProvenanceReport { target, limit })
                .await?;
        }
        AgentBridgeCommand::ConfirmMemoryProvenanceEntry { entry_id } => {
            framed
                .send(ClientMessage::AgentConfirmMemoryProvenanceEntry { entry_id })
                .await?;
        }
        AgentBridgeCommand::RetractMemoryProvenanceEntry { entry_id } => {
            framed
                .send(ClientMessage::AgentRetractMemoryProvenanceEntry { entry_id })
                .await?;
        }
        AgentBridgeCommand::GetCollaborationSessions { parent_task_id } => {
            framed
                .send(ClientMessage::AgentGetCollaborationSessions { parent_task_id })
                .await?;
        }
        AgentBridgeCommand::SendParticipantSuggestion {
            thread_id,
            suggestion_id,
            session_id,
            force_send,
        } => {
            framed
                .send(ClientMessage::AgentSendParticipantSuggestion {
                    thread_id,
                    suggestion_id,
                    session_id,
                    client_surface: Some(zorai_protocol::ClientSurface::Electron),
                    force_send,
                })
                .await?;
        }
        AgentBridgeCommand::DismissParticipantSuggestion {
            thread_id,
            suggestion_id,
            session_id,
        } => {
            framed
                .send(ClientMessage::AgentDismissParticipantSuggestion {
                    thread_id,
                    suggestion_id,
                    session_id,
                    client_surface: Some(zorai_protocol::ClientSurface::Electron),
                })
                .await?;
        }
        AgentBridgeCommand::ListGeneratedTools => {
            framed.send(ClientMessage::AgentListGeneratedTools).await?;
        }
        AgentBridgeCommand::RunGeneratedTool {
            tool_name,
            args_json,
        } => {
            framed
                .send(ClientMessage::AgentRunGeneratedTool {
                    tool_name,
                    args_json,
                })
                .await?;
        }
        AgentBridgeCommand::SpeechToText { args_json } => {
            framed
                .send(ClientMessage::AgentSpeechToText { args_json })
                .await?;
        }
        AgentBridgeCommand::TextToSpeech { args_json } => {
            framed
                .send(ClientMessage::AgentTextToSpeech { args_json })
                .await?;
        }
        AgentBridgeCommand::PromoteGeneratedTool { tool_name } => {
            framed
                .send(ClientMessage::AgentPromoteGeneratedTool { tool_name })
                .await?;
        }
        AgentBridgeCommand::ActivateGeneratedTool { tool_name } => {
            framed
                .send(ClientMessage::AgentActivateGeneratedTool { tool_name })
                .await?;
        }
        AgentBridgeCommand::RetireGeneratedTool { tool_name } => {
            framed
                .send(ClientMessage::AgentRetireGeneratedTool { tool_name })
                .await?;
        }
        AgentBridgeCommand::VoteOnCollaborationDisagreement {
            parent_task_id,
            disagreement_id,
            task_id,
            position,
            confidence,
        } => {
            framed
                .send(ClientMessage::AgentVoteOnCollaborationDisagreement {
                    parent_task_id,
                    disagreement_id,
                    task_id,
                    position,
                    confidence,
                })
                .await?;
        }
        AgentBridgeCommand::GetStatistics {
            window,
            session_limit,
            session_offset,
            sessions_only,
        } => {
            framed
                .send(ClientMessage::AgentStatisticsQuery {
                    window,
                    session_limit,
                    session_offset,
                    sessions_only,
                })
                .await?;
        }
        AgentBridgeCommand::GetStatus => {
            framed.send(ClientMessage::AgentStatusQuery).await?;
        }
        AgentBridgeCommand::InspectPrompt { agent_id } => {
            framed
                .send(ClientMessage::AgentInspectPrompt {
                    agent_id,
                    client_surface: Some(zorai_protocol::ClientSurface::Tui),
                })
                .await?;
        }
        AgentBridgeCommand::SetTierOverride { tier } => {
            framed
                .send(ClientMessage::AgentSetTierOverride { tier })
                .await?;
        }
        AgentBridgeCommand::PluginList => {
            framed.send(ClientMessage::PluginList {}).await?;
        }
        AgentBridgeCommand::PluginGetDetail { name } => {
            framed.send(ClientMessage::PluginGet { name }).await?;
        }
        AgentBridgeCommand::PluginEnableCmd { name } => {
            framed.send(ClientMessage::PluginEnable { name }).await?;
        }
        AgentBridgeCommand::PluginDisableCmd { name } => {
            framed.send(ClientMessage::PluginDisable { name }).await?;
        }
        AgentBridgeCommand::PluginInstallCmd {
            dir_name,
            install_source,
        } => {
            framed
                .send(ClientMessage::PluginInstall {
                    dir_name,
                    install_source,
                })
                .await?;
        }
        AgentBridgeCommand::PluginUninstallCmd { name } => {
            framed.send(ClientMessage::PluginUninstall { name }).await?;
        }
        AgentBridgeCommand::PluginGetSettings { name } => {
            framed
                .send(ClientMessage::PluginGetSettings { name })
                .await?;
        }
        AgentBridgeCommand::PluginUpdateSettings {
            plugin_name,
            key,
            value,
            is_secret,
        } => {
            framed
                .send(ClientMessage::PluginUpdateSettings {
                    plugin_name,
                    key,
                    value,
                    is_secret,
                })
                .await?;
        }
        AgentBridgeCommand::PluginTestConnection { name } => {
            framed
                .send(ClientMessage::PluginTestConnection { name })
                .await?;
        }
        AgentBridgeCommand::PluginOAuthStart { name } => {
            framed
                .send(ClientMessage::PluginOAuthStart { name })
                .await?;
        }
        AgentBridgeCommand::WhatsAppLinkStart => {
            framed.send(ClientMessage::AgentWhatsAppLinkStart).await?;
        }
        AgentBridgeCommand::WhatsAppLinkStop => {
            framed.send(ClientMessage::AgentWhatsAppLinkStop).await?;
        }
        AgentBridgeCommand::WhatsAppLinkStatus => {
            framed.send(ClientMessage::AgentWhatsAppLinkStatus).await?;
        }
        AgentBridgeCommand::WhatsAppLinkSubscribe => {
            framed
                .send(ClientMessage::AgentWhatsAppLinkSubscribe)
                .await?;
        }
        AgentBridgeCommand::WhatsAppLinkUnsubscribe => {
            framed
                .send(ClientMessage::AgentWhatsAppLinkUnsubscribe)
                .await?;
        }
        AgentBridgeCommand::StartOperatorProfileSession { kind } => {
            framed
                .send(ClientMessage::AgentStartOperatorProfileSession { kind })
                .await?;
        }
        AgentBridgeCommand::NextOperatorProfileQuestion { session_id } => {
            framed
                .send(ClientMessage::AgentNextOperatorProfileQuestion { session_id })
                .await?;
        }
        AgentBridgeCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            framed
                .send(ClientMessage::AgentSubmitOperatorProfileAnswer {
                    session_id,
                    question_id,
                    answer_json,
                })
                .await?;
        }
        AgentBridgeCommand::SkipOperatorProfileQuestion {
            session_id,
            question_id,
            reason,
        } => {
            framed
                .send(ClientMessage::AgentSkipOperatorProfileQuestion {
                    session_id,
                    question_id,
                    reason,
                })
                .await?;
        }
        AgentBridgeCommand::DeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        } => {
            framed
                .send(ClientMessage::AgentDeferOperatorProfileQuestion {
                    session_id,
                    question_id,
                    defer_until_unix_ms,
                })
                .await?;
        }
        AgentBridgeCommand::AnswerQuestion {
            question_id,
            answer,
        } => {
            framed
                .send(ClientMessage::AgentAnswerQuestion {
                    question_id,
                    answer,
                })
                .await?;
        }
        AgentBridgeCommand::GetOperatorProfileSummary => {
            framed
                .send(ClientMessage::AgentGetOperatorProfileSummary)
                .await?;
        }
        AgentBridgeCommand::SetOperatorProfileConsent {
            consent_key,
            granted,
        } => {
            framed
                .send(ClientMessage::AgentSetOperatorProfileConsent {
                    consent_key,
                    granted,
                })
                .await?;
        }
        AgentBridgeCommand::ExplainAction {
            action_id,
            step_index,
        } => {
            framed
                .send(ClientMessage::AgentExplainAction {
                    action_id,
                    step_index,
                })
                .await?;
        }
        AgentBridgeCommand::StartDivergentSession {
            problem_statement,
            thread_id,
            goal_run_id,
            custom_framings_json,
        } => {
            framed
                .send(ClientMessage::AgentStartDivergentSession {
                    problem_statement,
                    thread_id,
                    goal_run_id,
                    custom_framings_json,
                })
                .await?;
        }
        AgentBridgeCommand::GetDivergentSession { session_id } => {
            framed
                .send(ClientMessage::AgentGetDivergentSession { session_id })
                .await?;
        }
        AgentBridgeCommand::EnqueuePrompt {
            thread_id,
            content,
            content_blocks_json,
            prompt_id,
        } => {
            framed
                .send(ClientMessage::AgentEnqueuePrompt {
                    thread_id,
                    content,
                    content_blocks_json,
                    prompt_id,
                })
                .await?;
        }
        AgentBridgeCommand::ListPromptQueue { thread_id } => {
            framed
                .send(ClientMessage::AgentListPromptQueue { thread_id })
                .await?;
        }
        AgentBridgeCommand::UpdateQueuedPrompt {
            thread_id,
            prompt_id,
            content,
            content_blocks_json,
        } => {
            framed
                .send(ClientMessage::AgentUpdateQueuedPrompt {
                    thread_id,
                    prompt_id,
                    content,
                    content_blocks_json,
                })
                .await?;
        }
        AgentBridgeCommand::CancelQueuedPrompt {
            thread_id,
            prompt_id,
        } => {
            framed
                .send(ClientMessage::AgentCancelQueuedPrompt {
                    thread_id,
                    prompt_id,
                })
                .await?;
        }
        AgentBridgeCommand::SendQueuedPromptNow {
            thread_id,
            prompt_id,
        } => {
            framed
                .send(ClientMessage::AgentSendQueuedPromptNow {
                    thread_id,
                    prompt_id,
                })
                .await?;
        }
        AgentBridgeCommand::GetThreadExecutionProfile { thread_id } => {
            framed
                .send(ClientMessage::AgentGetThreadExecutionProfile { thread_id })
                .await?;
        }
        AgentBridgeCommand::SetThreadExecutionProfile {
            thread_id,
            profile_json,
        } => {
            framed
                .send(ClientMessage::AgentSetThreadExecutionProfile {
                    thread_id,
                    profile_json,
                })
                .await?;
        }
        AgentBridgeCommand::ForceCompact { thread_id } => {
            framed
                .send(ClientMessage::AgentForceCompact { thread_id })
                .await?;
        }
        AgentBridgeCommand::Shutdown => {
            framed.send(ClientMessage::AgentUnsubscribe).await?;
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::handle_line;
    use crate::client::agent_protocol::AgentBridgeCommand;
    use bytes::BytesMut;
    use futures::{SinkExt, StreamExt};
    use tokio_util::codec::Framed;
    use tokio_util::codec::{Decoder, Encoder};
    use zorai_protocol::{ClientMessage, DaemonCodec, ZoraiCodec};

    async fn emitted_client_message(line: &str) -> ClientMessage {
        let (client_side, server_side) = tokio::io::duplex(1024);
        let mut bridge = Framed::new(client_side, ZoraiCodec);
        let mut daemon = Framed::new(server_side, DaemonCodec);

        let (handle_result, message_result) =
            tokio::join!(handle_line(&mut bridge, line), daemon.next());

        assert!(handle_result.expect("bridge command should be handled"));

        message_result
            .expect("expected outbound client message")
            .expect("codec should decode client message")
    }

    async fn assert_emitted_client_message(line: &str, expected: ClientMessage) {
        let message = emitted_client_message(line).await;
        assert_eq!(
            std::mem::discriminant(&message),
            std::mem::discriminant(&expected)
        );
    }

    #[tokio::test]
    async fn handoff_thread_command_emits_handoff_frame() {
        let message = emitted_client_message(
            r#"{"type":"handoff-thread","thread_id":"thread-1","action":"push_handoff","target_agent_id":"rarog","reason":"need concierge","summary":"take over","requested_by":"user","session_id":"session-1"}"#,
        )
        .await;

        match message {
            ClientMessage::AgentHandoffThread {
                thread_id,
                action,
                target_agent_id,
                reason,
                summary,
                requested_by,
                session_id,
                client_surface,
            } => {
                assert_eq!(thread_id, "thread-1");
                assert_eq!(action, "push_handoff");
                assert_eq!(target_agent_id.as_deref(), Some("rarog"));
                assert_eq!(reason, "need concierge");
                assert_eq!(summary, "take over");
                assert_eq!(requested_by, "user");
                assert_eq!(session_id.as_deref(), Some("session-1"));
                assert_eq!(
                    client_surface,
                    Some(zorai_protocol::ClientSurface::Electron)
                );
            }
            other => panic!("expected AgentHandoffThread, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_operation_status_command_emits_status_query_frame() {
        let message =
            emitted_client_message(r#"{"type":"get-operation-status","operation_id":"op-1"}"#)
                .await;

        match message {
            ClientMessage::AgentGetOperationStatus { operation_id } => {
                assert_eq!(operation_id, "op-1");
            }
            other => panic!("expected AgentGetOperationStatus, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_participant_suggestion_command_emits_force_send_frame() {
        let message = emitted_client_message(
            r#"{"type":"send-participant-suggestion","thread_id":"thread-1","suggestion_id":"sugg-1","session_id":"session-1","force_send":true}"#,
        )
        .await;

        match message {
            ClientMessage::AgentSendParticipantSuggestion {
                thread_id,
                suggestion_id,
                session_id,
                client_surface,
                force_send,
            } => {
                assert_eq!(thread_id, "thread-1");
                assert_eq!(suggestion_id, "sugg-1");
                assert_eq!(session_id.as_deref(), Some("session-1"));
                assert_eq!(
                    client_surface,
                    Some(zorai_protocol::ClientSurface::Electron)
                );
                assert!(force_send);
            }
            other => panic!("expected AgentSendParticipantSuggestion, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_participant_suggestion_command_defaults_force_send_false() {
        let message = emitted_client_message(
            r#"{"type":"send-participant-suggestion","thread_id":"thread-1","suggestion_id":"sugg-1"}"#,
        )
        .await;

        match message {
            ClientMessage::AgentSendParticipantSuggestion {
                thread_id,
                suggestion_id,
                force_send,
                ..
            } => {
                assert_eq!(thread_id, "thread-1");
                assert_eq!(suggestion_id, "sugg-1");
                assert!(!force_send);
            }
            other => panic!("expected AgentSendParticipantSuggestion, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dismiss_participant_suggestion_command_emits_dismiss_frame() {
        let message = emitted_client_message(
            r#"{"type":"dismiss-participant-suggestion","thread_id":"thread-1","suggestion_id":"sugg-1","session_id":"session-1"}"#,
        )
        .await;

        match message {
            ClientMessage::AgentDismissParticipantSuggestion {
                thread_id,
                suggestion_id,
                session_id,
                client_surface,
            } => {
                assert_eq!(thread_id, "thread-1");
                assert_eq!(suggestion_id, "sugg-1");
                assert_eq!(session_id.as_deref(), Some("session-1"));
                assert_eq!(
                    client_surface,
                    Some(zorai_protocol::ClientSurface::Electron)
                );
            }
            other => panic!("expected AgentDismissParticipantSuggestion, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cancel_task_command_remains_operation_cancellation_route() {
        let message = emitted_client_message(r#"{"type":"cancel-task","task_id":"op-1"}"#).await;

        match message {
            ClientMessage::AgentCancelTask { task_id } => {
                assert_eq!(task_id, "op-1");
            }
            other => panic!("expected AgentCancelTask, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_target_agent_reasoning_effort_command_maps_to_client_message() {
        let message = emitted_client_message(
            r#"{"type":"set-target-agent-reasoning-effort","target_agent_id":"rarog","reasoning_effort":"high"}"#,
        )
        .await;
        match message {
            ClientMessage::AgentSetTargetAgentReasoningEffort {
                target_agent_id,
                reasoning_effort,
            } => {
                assert_eq!(target_agent_id, "rarog");
                assert_eq!(reasoning_effort, "high");
            }
            other => panic!("expected AgentSetTargetAgentReasoningEffort, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_target_agent_context_window_command_maps_to_client_message() {
        let message = emitted_client_message(
            r#"{"type":"set-target-agent-context-window","target_agent_id":"weles","context_window_tokens":180000}"#,
        )
        .await;
        match message {
            ClientMessage::AgentSetTargetAgentContextWindow {
                target_agent_id,
                context_window_tokens,
            } => {
                assert_eq!(target_agent_id, "weles");
                assert_eq!(context_window_tokens, 180_000);
            }
            other => panic!("expected AgentSetTargetAgentContextWindow, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn openai_codex_auth_status_command_maps_to_client_message() {
        assert_emitted_client_message(
            r#"{"type":"openai-codex-auth-status"}"#,
            ClientMessage::AgentGetOpenAICodexAuthStatus,
        )
        .await;
    }

    #[tokio::test]
    async fn openai_codex_auth_login_command_maps_to_client_message() {
        assert_emitted_client_message(
            r#"{"type":"openai-codex-auth-login"}"#,
            ClientMessage::AgentLoginOpenAICodex,
        )
        .await;
    }

    #[tokio::test]
    async fn openai_codex_auth_logout_command_maps_to_client_message() {
        assert_emitted_client_message(
            r#"{"type":"openai-codex-auth-logout"}"#,
            ClientMessage::AgentLogoutOpenAICodex,
        )
        .await;
    }

    #[tokio::test]
    async fn list_threads_forwards_agent_filter_and_include_internal() {
        let message = emitted_client_message(
            r#"{"type":"list-threads","agent_filter":"dazhbog","include_internal":true}"#,
        )
        .await;
        match message {
            ClientMessage::AgentListThreads {
                agent_filter,
                include_internal,
                limit,
                offset,
            } => {
                assert_eq!(agent_filter.as_deref(), Some("dazhbog"));
                assert!(include_internal);
                assert_eq!(limit, None);
                assert_eq!(offset, None);
            }
            other => panic!("expected AgentListThreads, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_threads_defaults_to_public_unfiltered_list() {
        let message = emitted_client_message(r#"{"type":"list-threads"}"#).await;
        match message {
            ClientMessage::AgentListThreads {
                agent_filter,
                include_internal,
                ..
            } => {
                assert_eq!(agent_filter, None);
                assert!(!include_internal);
            }
            other => panic!("expected AgentListThreads, got {other:?}"),
        }
    }

    #[test]
    fn send_message_command_deserializes() {
        let command: AgentBridgeCommand = serde_json::from_str(
            r#"{"type":"send-message","thread_id":"thread-1","content":"hello","session_id":null,"context_messages":null,"target_agent_id":"weles"}"#,
        )
        .expect("send-message command should deserialize");

        match command {
            AgentBridgeCommand::SendMessage {
                thread_id,
                content,
                session_id,
                context_messages,
                content_blocks_json,
                target_agent_id,
                workspace_context,
            } => {
                assert!(workspace_context.is_none());
                assert_eq!(thread_id.as_deref(), Some("thread-1"));
                assert_eq!(content, "hello");
                assert!(session_id.is_none());
                assert!(context_messages.is_none());
                assert!(content_blocks_json.is_none());
                assert_eq!(target_agent_id.as_deref(), Some("weles"));
            }
            other => panic!("expected SendMessage command, got {other:?}"),
        }
    }

    #[test]
    fn internal_delegate_command_deserializes() {
        let command: AgentBridgeCommand = serde_json::from_str(
            r#"{"type":"internal-delegate","thread_id":"thread-1","target_agent_id":"weles","content":"verify this","session_id":null}"#,
        )
        .expect("internal-delegate command should deserialize");

        match command {
            AgentBridgeCommand::InternalDelegate {
                thread_id,
                target_agent_id,
                content,
                session_id,
            } => {
                assert_eq!(thread_id.as_deref(), Some("thread-1"));
                assert_eq!(target_agent_id, "weles");
                assert_eq!(content, "verify this");
                assert!(session_id.is_none());
            }
            other => panic!("expected InternalDelegate command, got {other:?}"),
        }
    }

    #[test]
    fn thread_participant_command_deserializes() {
        let command: AgentBridgeCommand = serde_json::from_str(
            r#"{"type":"thread-participant-command","thread_id":"thread-1","target_agent_id":"weles","action":"upsert","instruction":"verify claims","session_id":null}"#,
        )
        .expect("thread-participant-command should deserialize");

        match command {
            AgentBridgeCommand::ThreadParticipantCommand {
                thread_id,
                target_agent_id,
                action,
                instruction,
                session_id,
            } => {
                assert_eq!(thread_id, "thread-1");
                assert_eq!(target_agent_id, "weles");
                assert_eq!(action, "upsert");
                assert_eq!(instruction.as_deref(), Some("verify claims"));
                assert!(session_id.is_none());
            }
            other => panic!("expected ThreadParticipantCommand, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_message_command_emits_agent_send_message_frame() {
        let message = emitted_client_message(
            r#"{"type":"send-message","thread_id":"thread-1","content":"hello","session_id":null,"context_messages":null,"target_agent_id":"weles"}"#,
        )
        .await;

        match message {
            ClientMessage::AgentSendMessage {
                thread_id,
                content,
                session_id,
                context_messages_json,
                content_blocks_json,
                client_surface,
                target_agent_id,
                workspace_context_json,
            } => {
                assert!(workspace_context_json.is_none());
                assert_eq!(thread_id.as_deref(), Some("thread-1"));
                assert_eq!(content, "hello");
                assert!(session_id.is_none());
                assert!(context_messages_json.is_none());
                assert!(content_blocks_json.is_none());
                assert_eq!(
                    client_surface,
                    Some(zorai_protocol::ClientSurface::Electron)
                );
                assert_eq!(target_agent_id.as_deref(), Some("weles"));
            }
            other => panic!("expected AgentSendMessage, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_message_command_preserves_image_content_blocks() {
        let blocks = r#"[{\"type\":\"image\",\"data_url\":\"data:image/png;base64,iVBORw0KGgo=\",\"mime_type\":\"image/png\"}]"#;
        let line = serde_json::json!({
            "type": "send-message",
            "thread_id": "thread-image",
            "content": "what is in this picture?",
            "session_id": null,
            "context_messages": null,
            "content_blocks_json": blocks,
            "target_agent_id": null,
        })
        .to_string();
        let message = emitted_client_message(&line).await;

        match message {
            ClientMessage::AgentSendMessage {
                thread_id,
                content,
                content_blocks_json,
                client_surface,
                ..
            } => {
                assert_eq!(thread_id.as_deref(), Some("thread-image"));
                assert_eq!(content, "what is in this picture?");
                assert_eq!(content_blocks_json.as_deref(), Some(blocks));
                assert_eq!(
                    client_surface,
                    Some(zorai_protocol::ClientSurface::Electron)
                );
            }
            other => panic!("expected AgentSendMessage, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn internal_delegate_command_emits_internal_delegate_frame() {
        let message = emitted_client_message(
            r#"{"type":"internal-delegate","thread_id":"thread-1","target_agent_id":"weles","content":"verify this","session_id":null}"#,
        )
        .await;

        match message {
            ClientMessage::AgentInternalDelegate {
                thread_id,
                target_agent_id,
                content,
                session_id,
                client_surface,
            } => {
                assert_eq!(thread_id.as_deref(), Some("thread-1"));
                assert_eq!(target_agent_id, "weles");
                assert_eq!(content, "verify this");
                assert!(session_id.is_none());
                assert_eq!(
                    client_surface,
                    Some(zorai_protocol::ClientSurface::Electron)
                );
            }
            other => panic!("expected AgentInternalDelegate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn participant_command_emits_thread_participant_frame() {
        let message = emitted_client_message(
            r#"{"type":"thread-participant-command","thread_id":"thread-1","target_agent_id":"weles","action":"upsert","instruction":"verify claims","session_id":null}"#,
        )
        .await;

        match message {
            ClientMessage::AgentThreadParticipantCommand {
                thread_id,
                target_agent_id,
                action,
                instruction,
                session_id,
                client_surface,
            } => {
                assert_eq!(thread_id, "thread-1");
                assert_eq!(target_agent_id, "weles");
                assert_eq!(action, "upsert");
                assert_eq!(instruction.as_deref(), Some("verify claims"));
                assert!(session_id.is_none());
                assert_eq!(
                    client_surface,
                    Some(zorai_protocol::ClientSurface::Electron)
                );
            }
            other => panic!("expected AgentThreadParticipantCommand, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_thread_command_preserves_message_window() {
        let message = emitted_client_message(
            r#"{"type":"get-thread","thread_id":"thread-1","message_limit":50,"message_offset":100}"#,
        )
        .await;

        match message {
            ClientMessage::AgentGetThread {
                thread_id,
                message_limit,
                message_offset,
                collapse_tool_calls,
            } => {
                assert_eq!(thread_id, "thread-1");
                assert_eq!(message_limit, Some(50));
                assert_eq!(message_offset, Some(100));
                assert!(!collapse_tool_calls);
            }
            other => panic!("expected AgentGetThread, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn direct_agent_send_message_frame_decodes_cleanly() {
        let message = ClientMessage::AgentSendMessage {
            thread_id: Some("thread-1".to_string()),
            content: "hello".to_string(),
            session_id: None,
            context_messages_json: None,
            content_blocks_json: None,
            client_surface: Some(zorai_protocol::ClientSurface::Electron),
            target_agent_id: None,
            workspace_context_json: None,
        };

        let mut encoded = BytesMut::new();
        ZoraiCodec
            .encode(message.clone(), &mut encoded)
            .expect("codec should encode AgentSendMessage in memory");
        let decoded = DaemonCodec
            .decode(&mut encoded)
            .expect("codec decode should not error in memory")
            .expect("codec should decode AgentSendMessage from memory buffer");
        assert!(matches!(decoded, ClientMessage::AgentSendMessage { .. }));

        let (client_side, server_side) = tokio::io::duplex(4096);
        let mut bridge = Framed::new(client_side, ZoraiCodec);
        let mut daemon = Framed::new(server_side, DaemonCodec);

        bridge
            .send(message)
            .await
            .expect("direct send should succeed");

        match daemon.next().await {
            Some(Ok(ClientMessage::AgentSendMessage { content, .. })) => {
                assert_eq!(content, "hello");
            }
            other => panic!("expected direct AgentSendMessage decode, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn direct_agent_get_thread_frame_decodes_cleanly() {
        let (client_side, server_side) = tokio::io::duplex(4096);
        let mut bridge = Framed::new(client_side, ZoraiCodec);
        let mut daemon = Framed::new(server_side, DaemonCodec);

        bridge
            .send(ClientMessage::AgentGetThread {
                thread_id: "thread-1".to_string(),
                message_limit: None,
                message_offset: None,
                collapse_tool_calls: false,
            })
            .await
            .expect("direct send should succeed");

        match daemon.next().await {
            Some(Ok(ClientMessage::AgentGetThread {
                thread_id,
                message_limit,
                message_offset,
                ..
            })) => {
                assert_eq!(thread_id, "thread-1");
                assert!(message_limit.is_none());
                assert!(message_offset.is_none());
            }
            other => panic!("expected direct AgentGetThread decode, got {other:?}"),
        }
    }
}
