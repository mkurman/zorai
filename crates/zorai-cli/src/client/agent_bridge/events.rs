use anyhow::Result;
use futures::StreamExt;
use tokio_util::codec::Framed;
use zorai_protocol::{DaemonMessage, OperationLifecycleState, ZoraiCodec};

use super::emit_agent_event;

#[derive(Debug, Default)]
pub(super) struct ThreadDetailChunkBuffer {
    thread_id: Option<String>,
    bytes: Vec<u8>,
}

fn extend_thread_detail_chunk(
    buffer: &mut Option<ThreadDetailChunkBuffer>,
    thread_id: String,
    thread_json_chunk: Vec<u8>,
    done: bool,
) -> Result<Option<serde_json::Value>> {
    let chunk_buffer = buffer.get_or_insert_with(ThreadDetailChunkBuffer::default);
    if chunk_buffer.thread_id.as_deref() != Some(thread_id.as_str()) {
        chunk_buffer.thread_id = Some(thread_id);
        chunk_buffer.bytes.clear();
    }
    chunk_buffer.bytes.extend(thread_json_chunk);
    if !done {
        return Ok(None);
    }

    let bytes = std::mem::take(&mut chunk_buffer.bytes);
    chunk_buffer.thread_id = None;
    *buffer = None;

    let thread_json = String::from_utf8(bytes)?;
    Ok(Some(serde_json::json!({
        "type": "thread-detail",
        "data": serde_json::from_str::<serde_json::Value>(&thread_json).unwrap_or_default(),
    })))
}

fn operation_lifecycle_state_name(state: OperationLifecycleState) -> &'static str {
    match state {
        OperationLifecycleState::Accepted => "accepted",
        OperationLifecycleState::Started => "started",
        OperationLifecycleState::Completed => "completed",
        OperationLifecycleState::Failed => "failed",
    }
}

fn bridge_event_from_daemon_message(message: &DaemonMessage) -> Option<serde_json::Value> {
    match message {
        DaemonMessage::AgentOperatorModel { model_json } => Some(serde_json::json!({
            "type": "operator-model",
            "data": serde_json::from_str::<serde_json::Value>(model_json).unwrap_or_default(),
        })),
        DaemonMessage::AgentOperatorModelReset { ok } => Some(serde_json::json!({
            "type": "operator-model-reset",
            "data": {
                "ok": ok,
            }
        })),
        DaemonMessage::AuditList { entries_json } => Some(serde_json::json!({
            "type": "audit-list",
            "data": serde_json::from_str::<serde_json::Value>(entries_json).unwrap_or_default(),
        })),
        DaemonMessage::AgentProvenanceReport { report_json } => Some(serde_json::json!({
            "type": "provenance-report",
            "data": serde_json::from_str::<serde_json::Value>(report_json).unwrap_or_default(),
        })),
        DaemonMessage::AgentMemoryProvenanceReport { report_json } => Some(serde_json::json!({
            "type": "memory-provenance-report",
            "data": serde_json::from_str::<serde_json::Value>(report_json).unwrap_or_default(),
        })),
        DaemonMessage::AgentMemoryProvenanceConfirmed {
            entry_id,
            confirmed_at,
        } => Some(serde_json::json!({
            "type": "memory-provenance-confirmed",
            "data": {
                "entry_id": entry_id,
                "confirmed_at": confirmed_at,
            }
        })),
        DaemonMessage::AgentMemoryProvenanceRetracted {
            entry_id,
            retracted_at,
        } => Some(serde_json::json!({
            "type": "memory-provenance-retracted",
            "data": {
                "entry_id": entry_id,
                "retracted_at": retracted_at,
            }
        })),
        DaemonMessage::AgentCollaborationSessions { sessions_json } => Some(serde_json::json!({
            "type": "collaboration-sessions",
            "data": serde_json::from_str::<serde_json::Value>(sessions_json).unwrap_or_default(),
        })),
        DaemonMessage::AgentCollaborationVoteResult { report_json } => Some(serde_json::json!({
            "type": "collaboration-vote-result",
            "data": serde_json::from_str::<serde_json::Value>(report_json).unwrap_or_default(),
        })),
        DaemonMessage::AgentThreadHandoffResult { result } => Some(serde_json::json!({
            "type": "thread-handoff-result",
            "data": result,
        })),
        DaemonMessage::OperationStatus { snapshot } => Some(serde_json::json!({
            "type": "operation-status",
            "data": {
                "operation_id": snapshot.operation_id,
                "kind": snapshot.kind,
                "dedup": snapshot.dedup,
                "state": operation_lifecycle_state_name(snapshot.state),
                "revision": snapshot.revision,
            }
        })),
        DaemonMessage::AgentGeneratedTools { tools_json } => Some(serde_json::json!({
            "type": "generated-tools",
            "data": serde_json::from_str::<serde_json::Value>(tools_json).unwrap_or_default(),
        })),
        DaemonMessage::AgentGeneratedToolResult {
            operation_id,
            tool_name,
            result_json,
        } => Some(serde_json::json!({
            "type": "generated-tool-result",
            "data": {
                "operation_id": operation_id,
                "tool_name": tool_name,
                "result": serde_json::from_str::<serde_json::Value>(result_json).unwrap_or_default(),
            }
        })),
        DaemonMessage::AgentSpeechToTextResult { content } => Some(serde_json::json!({
            "type": "speech-to-text-result",
            "data": serde_json::from_str::<serde_json::Value>(content)
                .unwrap_or_else(|_| serde_json::Value::String(content.clone())),
        })),
        DaemonMessage::AgentTextToSpeechResult { content } => Some(serde_json::json!({
            "type": "text-to-speech-result",
            "data": serde_json::from_str::<serde_json::Value>(content)
                .unwrap_or_else(|_| serde_json::Value::String(content.clone())),
        })),
        DaemonMessage::AgentOpenAICodexAuthStatus { status_json } => Some(serde_json::json!({
            "type": "openai-codex-auth-status",
            "data": serde_json::from_str::<serde_json::Value>(status_json).unwrap_or_default(),
        })),
        DaemonMessage::AgentOpenAICodexAuthLoginResult { result_json } => Some(serde_json::json!({
            "type": "openai-codex-auth-login-result",
            "data": serde_json::from_str::<serde_json::Value>(result_json).unwrap_or_default(),
        })),
        DaemonMessage::AgentOpenAICodexAuthLogoutResult { ok, error } => Some(serde_json::json!({
            "type": "openai-codex-auth-logout-result",
            "data": {
                "ok": ok,
                "error": error,
            }
        })),
        DaemonMessage::AgentModelsResponse {
            operation_id,
            models_json,
        } => Some(serde_json::json!({
            "type": "provider-models",
            "data": {
                "operation_id": operation_id,
                "models": serde_json::from_str::<serde_json::Value>(models_json).unwrap_or_default(),
            }
        })),
        _ => None,
    }
}

pub(super) async fn handle_message<T>(
    framed: &mut Framed<T, ZoraiCodec>,
    thread_detail_chunks: &mut Option<ThreadDetailChunkBuffer>,
) -> Result<bool>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    match framed.next().await {
        Some(Ok(message)) if bridge_event_from_daemon_message(&message).is_some() => {
            let event = bridge_event_from_daemon_message(&message)
                .expect("bridge event must exist for matched daemon message");
            emit_agent_event(&event.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentEvent { event_json })) => {
            emit_agent_event(&event_json)?;
        }
        Some(Ok(DaemonMessage::AgentThreadList { threads_json })) => {
            let msg = serde_json::json!({"type":"thread-list","data":serde_json::from_str::<serde_json::Value>(&threads_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentThreadDetail { thread_json })) => {
            let msg = serde_json::json!({"type":"thread-detail","data":serde_json::from_str::<serde_json::Value>(&thread_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentThreadMessagePinResult { result_json })) => {
            let msg = serde_json::json!({"type":"thread-message-pin-result","data":serde_json::from_str::<serde_json::Value>(&result_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentThreadDetailChunk {
            thread_id,
            thread_json_chunk,
            done,
        })) => {
            if let Some(msg) = extend_thread_detail_chunk(
                thread_detail_chunks,
                thread_id,
                thread_json_chunk,
                done,
            )? {
                emit_agent_event(&msg.to_string())?;
            }
        }
        Some(Ok(DaemonMessage::AgentTaskList { tasks_json })) => {
            let msg = serde_json::json!({"type":"task-list","data":serde_json::from_str::<serde_json::Value>(&tasks_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentRunList { runs_json })) => {
            let msg = serde_json::json!({"type":"run-list","data":serde_json::from_str::<serde_json::Value>(&runs_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentRunDetail { run_json })) => {
            let msg = serde_json::json!({"type":"run-detail","data":serde_json::from_str::<serde_json::Value>(&run_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTaskEnqueued { task_json })) => {
            let msg = serde_json::json!({"type":"task-enqueued","data":serde_json::from_str::<serde_json::Value>(&task_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTaskCancelled { task_id, cancelled })) => {
            let msg = serde_json::json!({"type":"task-cancelled","task_id":task_id,"cancelled":cancelled});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGoalRunStarted { goal_run_json })) => {
            let msg = serde_json::json!({"type":"goal-run-started","data":serde_json::from_str::<serde_json::Value>(&goal_run_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGoalRunList { goal_runs_json })) => {
            let msg = serde_json::json!({"type":"goal-run-list","data":serde_json::from_str::<serde_json::Value>(&goal_runs_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGoalRunDetail { goal_run_json })) => {
            let msg = serde_json::json!({"type":"goal-run-detail","data":serde_json::from_str::<serde_json::Value>(&goal_run_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGoalRunControlled { goal_run_id, ok })) => {
            let msg = serde_json::json!({"type":"goal-run-controlled","data":{"goal_run_id":goal_run_id,"ok":ok}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkspaceSettingsList { settings })) => {
            let msg = serde_json::json!({"type":"workspace-settings-list","data":serde_json::to_value(settings).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkspaceSettings { settings })) => {
            let msg = serde_json::json!({"type":"workspace-settings","data":serde_json::to_value(settings).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkspaceTaskList {
            workspace_id,
            tasks,
        })) => {
            let msg = serde_json::json!({"type":"workspace-task-list","data":serde_json::to_value(tasks).unwrap_or_default(),"workspace_id":workspace_id});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkspaceTaskDetail { task })) => {
            let msg = serde_json::json!({"type":"workspace-task-detail","data":serde_json::to_value(task).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkspaceTaskUpdated { task })) => {
            let msg = serde_json::json!({"type":"workspace-task-updated","data":serde_json::to_value(task).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkspaceTaskDeleted {
            task_id,
            deleted_at,
        })) => {
            let msg = serde_json::json!({"type":"workspace-task-deleted","data":{"task_id":task_id,"deleted_at":deleted_at,"ok":true}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkspaceNotice { notice })) => {
            let msg = serde_json::json!({"type":"workspace-notice","data":serde_json::to_value(notice).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkspaceNoticeList {
            workspace_id,
            notices,
        })) => {
            let msg = serde_json::json!({"type":"workspace-notice-list","data":serde_json::to_value(notices).unwrap_or_default(),"workspace_id":workspace_id});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkspaceError { message })) => {
            let msg = serde_json::json!({"type":"error","message":message});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentThreadControlled {
            thread_id,
            action,
            ok,
        })) => {
            let msg = serde_json::json!({"type":"thread-controlled","data":{"thread_id":thread_id,"action":action,"ok":ok}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTodoList { todos_json })) => {
            let msg = serde_json::json!({"type":"todo-list","data":serde_json::from_str::<serde_json::Value>(&todos_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTodoDetail {
            thread_id,
            todos_json,
        })) => {
            let msg = serde_json::json!({"type":"todo-detail","data":{"thread_id":thread_id,"items":serde_json::from_str::<serde_json::Value>(&todos_json).unwrap_or_default()}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWorkContextDetail {
            thread_id,
            context_json,
        })) => {
            let msg = serde_json::json!({"type":"work-context-detail","data":{"thread_id":thread_id,"context":serde_json::from_str::<serde_json::Value>(&context_json).unwrap_or_default()}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentThreadWorkspaceContext {
            thread_id,
            context_json,
            updated,
        })) => {
            let msg = serde_json::json!({"type":"thread-workspace-context","data":{"thread_id":thread_id,"context":serde_json::from_str::<serde_json::Value>(&context_json).unwrap_or(serde_json::Value::Null),"updated":updated}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentSubagentSpawned {
            thread_id,
            result_json,
        })) => {
            let result =
                serde_json::from_str::<serde_json::Value>(&result_json).unwrap_or_default();
            let msg = serde_json::json!({"type":"subagent-spawned","data":{"thread_id":thread_id,"result":result}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentFileOperationSnapshot {
            operation_id,
            status_json,
        })) => {
            let msg = serde_json::json!({"type":"file-operation-snapshot","data":{"operation_id":operation_id,"status":serde_json::from_str::<serde_json::Value>(&status_json).unwrap_or_default()}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentFileOperationReverted {
            operation_id,
            result_json,
        })) => {
            let msg = serde_json::json!({"type":"file-operation-reverted","data":{"operation_id":operation_id,"result":serde_json::from_str::<serde_json::Value>(&result_json).unwrap_or_default()}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::GitDiff {
            repo_path,
            file_path,
            diff,
        })) => {
            let msg = serde_json::json!({"type":"git-diff","data":{"repo_path":repo_path,"file_path":file_path,"diff":diff}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::FilePreview {
            path,
            content,
            truncated,
            is_text,
        })) => {
            let msg = serde_json::json!({"type":"file-preview","data":{"path":path,"content":content,"truncated":truncated,"is_text":is_text}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentMlflowTracingStatus { status_json })) => {
            let msg = serde_json::json!({"type":"mlflow-tracing-status","data":serde_json::from_str::<serde_json::Value>(&status_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentMlflowTracingTestResult { result_json })) => {
            let msg = serde_json::json!({"type":"mlflow-tracing-test-result","data":serde_json::from_str::<serde_json::Value>(&result_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentMlflowTracingHeaders { names })) => {
            let msg = serde_json::json!({"type":"mlflow-tracing-headers","data":{"names":names}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentPromptQueue { thread_id, prompts })) => {
            let msg = serde_json::json!({
                "type": "prompt-queue",
                "data": {
                    "thread_id": thread_id,
                    "prompts": prompts,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentPromptQueueError {
            thread_id,
            message,
            prompts,
        })) => {
            let msg = serde_json::json!({
                "type": "prompt-queue",
                "data": {
                    "thread_id": thread_id,
                    "prompts": prompts,
                    "error": message,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentThreadExecutionProfile {
            thread_id,
            profile_json,
        })) => {
            let msg = serde_json::json!({
                "type": "thread-execution-profile",
                "data": {
                    "thread_id": thread_id,
                    "profile": serde_json::from_str::<serde_json::Value>(&profile_json)
                        .unwrap_or(serde_json::Value::Null),
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentConfigResponse { config_json })) => {
            let msg = serde_json::json!({"type":"config","data":serde_json::from_str::<serde_json::Value>(&config_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentExternalRuntimeMigrationResult { result_json })) => {
            let msg = serde_json::json!({"type":"external-runtime-migration","data":serde_json::from_str::<serde_json::Value>(&result_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGatewayConfig { config_json })) => {
            let msg = serde_json::json!({"type":"gateway-config","data":serde_json::from_str::<serde_json::Value>(&config_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentHeartbeatItems { items_json })) => {
            let msg = serde_json::json!({"type":"heartbeat-items","data":serde_json::from_str::<serde_json::Value>(&items_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentProviderValidation {
            operation_id: _,
            provider_id,
            valid,
            error,
            models_json,
        })) => {
            let msg = serde_json::json!({
                "type": "provider-validation",
                "data": {
                    "provider_id": provider_id,
                    "valid": valid,
                    "error": error,
                    "models": models_json.and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok()),
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentProviderAuthStates { states_json })) => {
            let msg = serde_json::json!({"type":"provider-auth-states","data":serde_json::from_str::<serde_json::Value>(&states_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentProviderCatalog { catalog_json })) => {
            let msg = serde_json::json!({"type":"provider-catalog","data":serde_json::from_str::<serde_json::Value>(&catalog_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentSubAgentList { sub_agents_json })) => {
            let msg = serde_json::json!({"type":"sub-agent-list","data":serde_json::from_str::<serde_json::Value>(&sub_agents_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentSubAgentUpdated { sub_agent_json })) => {
            let msg = serde_json::json!({"type":"sub-agent-updated","data":serde_json::from_str::<serde_json::Value>(&sub_agent_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentSubAgentRemoved { sub_agent_id })) => {
            let msg = serde_json::json!({"type":"sub-agent-removed","data":{"sub_agent_id":sub_agent_id}});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentFileOperationBefore { result_json })) => {
            let msg = serde_json::json!({"type":"file-operation-before","data":serde_json::from_str::<serde_json::Value>(&result_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::DataQueryResult { result_json })) => {
            let msg = serde_json::json!({"type":"data-query-result","data":serde_json::from_str::<serde_json::Value>(&result_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentGatewayChats { chats_json })) => {
            let msg = serde_json::json!({"type":"gateway-chats","data":serde_json::from_str::<serde_json::Value>(&chats_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentThreadSearchResults { results_json })) => {
            let msg = serde_json::json!({"type":"thread-search","data":serde_json::from_str::<serde_json::Value>(&results_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentConciergeConfig { config_json })) => {
            let msg = serde_json::json!({"type":"concierge-config","data":serde_json::from_str::<serde_json::Value>(&config_json).unwrap_or_default()});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentConciergeWelcomeDismissed)) => {
            let msg = serde_json::json!({"type":"concierge-welcome-dismissed"});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentTierChanged {
            previous_tier,
            new_tier,
            reason,
        })) => {
            let msg = serde_json::json!({
                "type": "tier-changed",
                "data": {
                    "previous_tier": previous_tier,
                    "new_tier": new_tier,
                    "reason": reason,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentStatusResponse {
            tier,
            feature_flags_json,
            activity,
            active_thread_id,
            active_goal_run_id,
            active_goal_run_title,
            provider_health_json,
            gateway_statuses_json,
            recent_actions_json,
            diagnostics_json,
        })) => {
            let msg = serde_json::json!({
                "type": "status-response",
                "data": {
                    "tier": tier,
                    "feature_flags": serde_json::from_str::<serde_json::Value>(&feature_flags_json).unwrap_or_default(),
                    "activity": activity,
                    "active_thread_id": active_thread_id,
                    "active_goal_run_id": active_goal_run_id,
                    "active_goal_run_title": active_goal_run_title,
                    "provider_health": serde_json::from_str::<serde_json::Value>(&provider_health_json).unwrap_or_default(),
                    "gateway_statuses": serde_json::from_str::<serde_json::Value>(&gateway_statuses_json).unwrap_or_default(),
                    "recent_actions": serde_json::from_str::<serde_json::Value>(&recent_actions_json).unwrap_or_default(),
                    "diagnostics": serde_json::from_str::<serde_json::Value>(&diagnostics_json).unwrap_or_default(),
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentStatisticsResponse { statistics_json })) => {
            let msg = serde_json::json!({
                "type": "statistics-response",
                "data": serde_json::from_str::<serde_json::Value>(&statistics_json).unwrap_or_default(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentPromptInspection { prompt_json })) => {
            let msg = serde_json::json!({
                "type": "prompt-inspection",
                "data": serde_json::from_str::<serde_json::Value>(&prompt_json).unwrap_or_default(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentExplanation {
            operation_id: _,
            explanation_json,
        })) => {
            let msg = serde_json::json!({
                "type": "agent-explanation",
                "data": serde_json::from_str::<serde_json::Value>(&explanation_json).unwrap_or_default(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentDivergentSessionStarted {
            operation_id: _,
            session_json,
        })) => {
            let msg = serde_json::json!({
                "type": "agent-divergent-session-started",
                "data": serde_json::from_str::<serde_json::Value>(&session_json).unwrap_or_default(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentDivergentSession { session_json })) => {
            let msg = serde_json::json!({
                "type": "agent-divergent-session",
                "data": serde_json::from_str::<serde_json::Value>(&session_json).unwrap_or_default(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginListResult { plugins })) => {
            let msg = serde_json::json!({
                "type": "plugin-list-result",
                "plugins": plugins.iter().map(|p| serde_json::json!({
                    "name": p.name, "version": p.version, "description": p.description,
                    "author": p.author, "enabled": p.enabled, "install_source": p.install_source,
                    "has_api": p.has_api, "has_auth": p.has_auth, "has_commands": p.has_commands,
                    "has_skills": p.has_skills, "endpoint_count": p.endpoint_count,
                    "settings_count": p.settings_count, "installed_at": p.installed_at,
                    "updated_at": p.updated_at, "auth_status": p.auth_status,
                })).collect::<Vec<_>>(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginGetResult {
            plugin,
            settings_schema,
        })) => {
            let msg = serde_json::json!({
                "type": "plugin-get-result",
                "plugin": plugin.as_ref().map(|p| serde_json::json!({
                    "name": p.name, "version": p.version, "description": p.description,
                    "author": p.author, "enabled": p.enabled, "install_source": p.install_source,
                    "has_api": p.has_api, "has_auth": p.has_auth, "has_commands": p.has_commands,
                    "has_skills": p.has_skills, "endpoint_count": p.endpoint_count,
                    "settings_count": p.settings_count, "installed_at": p.installed_at,
                    "updated_at": p.updated_at, "auth_status": p.auth_status,
                })),
                "settings_schema": settings_schema,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginActionResult { success, message })) => {
            let msg = serde_json::json!({
                "type": "plugin-action-result",
                "success": success,
                "message": message,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginSettingsResult {
            plugin_name,
            settings,
        })) => {
            let msg = serde_json::json!({
                "type": "plugin-settings",
                "plugin_name": plugin_name,
                "settings": settings.iter().map(|(k, v, s)| serde_json::json!({"key": k, "value": v, "is_secret": s})).collect::<Vec<_>>(),
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginTestConnectionResult {
            plugin_name,
            success,
            message,
        })) => {
            let msg = serde_json::json!({
                "type": "plugin-test-connection-result",
                "plugin_name": plugin_name,
                "success": success,
                "message": message,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginOAuthUrl { name, url })) => {
            let msg = serde_json::json!({
                "type": "plugin-oauth-url",
                "name": name,
                "url": url,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::PluginOAuthComplete {
            operation_id: _,
            name,
            success,
            error,
        })) => {
            let msg = serde_json::json!({
                "type": "plugin-oauth-complete",
                "name": name,
                "success": success,
                "error": error,
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinkStatus {
            state,
            phone,
            last_error,
        })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-status",
                "data": {
                    "status": state,
                    "phone": phone,
                    "last_error": last_error,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinkQr {
            ascii_qr,
            expires_at_ms,
        })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-qr",
                "data": {
                    "ascii_qr": ascii_qr,
                    "expires_at_ms": expires_at_ms,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinked { phone })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-linked",
                "data": {
                    "phone": phone,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinkError {
            message,
            recoverable,
        })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-error",
                "data": {
                    "message": message,
                    "recoverable": recoverable,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentWhatsAppLinkDisconnected { reason })) => {
            let msg = serde_json::json!({
                "type": "whatsapp-link-disconnected",
                "data": {
                    "reason": reason,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileSessionStarted { session_id, kind })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-session-started",
                "data": { "session_id": session_id, "kind": kind }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileQuestion {
            session_id,
            question_id,
            field_key,
            prompt,
            input_kind,
            optional,
        })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-question",
                "data": {
                    "session_id": session_id,
                    "question_id": question_id,
                    "field_key": field_key,
                    "prompt": prompt,
                    "input_kind": input_kind,
                    "optional": optional,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileProgress {
            session_id,
            answered,
            remaining,
            completion_ratio,
        })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-progress",
                "data": {
                    "session_id": session_id,
                    "answered": answered,
                    "remaining": remaining,
                    "completion_ratio": completion_ratio,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileSummary { summary_json })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-summary",
                "data": serde_json::from_str::<serde_json::Value>(&summary_json).unwrap_or_default()
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::AgentOperatorProfileSessionCompleted {
            session_id,
            updated_fields,
        })) => {
            let msg = serde_json::json!({
                "type": "operator-profile-session-completed",
                "data": {
                    "session_id": session_id,
                    "updated_fields": updated_fields,
                }
            });
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(DaemonMessage::Error { message }))
        | Some(Ok(DaemonMessage::AgentError { message })) => {
            let msg = serde_json::json!({"type":"error","message":message});
            emit_agent_event(&msg.to_string())?;
        }
        Some(Ok(_)) => {}
        Some(Err(error)) => return Err(error.into()),
        None => {
            let msg = serde_json::json!({"type":"error","message":"daemon connection closed"});
            emit_agent_event(&msg.to_string())?;
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use zorai_protocol::DaemonMessage;

    use super::{
        bridge_event_from_daemon_message, extend_thread_detail_chunk, ThreadDetailChunkBuffer,
    };

    #[test]
    fn translates_openai_codex_auth_status_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentOpenAICodexAuthStatus {
            status_json: r#"{"available":false,"authMode":"chatgpt_subscription"}"#.to_string(),
        })
        .expect("status event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "openai-codex-auth-status",
                "data": {
                    "available": false,
                    "authMode": "chatgpt_subscription"
                }
            })
        );
    }

    #[test]
    fn translates_openai_codex_auth_login_result_event() {
        let event =
            bridge_event_from_daemon_message(&DaemonMessage::AgentOpenAICodexAuthLoginResult {
                result_json: r#"{"status":"pending","authUrl":"https://example.test/auth"}"#
                    .to_string(),
            })
            .expect("login result event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "openai-codex-auth-login-result",
                "data": {
                    "status": "pending",
                    "authUrl": "https://example.test/auth"
                }
            })
        );
    }

    #[test]
    fn translates_openai_codex_auth_logout_result_event() {
        let event =
            bridge_event_from_daemon_message(&DaemonMessage::AgentOpenAICodexAuthLogoutResult {
                ok: true,
                error: None,
            })
            .expect("logout result event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "openai-codex-auth-logout-result",
                "data": {
                    "ok": true,
                    "error": null
                }
            })
        );
    }

    #[test]
    fn translates_operator_model_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentOperatorModel {
            model_json: r#"{"version":"1.0","session_count":4}"#.to_string(),
        })
        .expect("operator model event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "operator-model",
                "data": {
                    "version": "1.0",
                    "session_count": 4
                }
            })
        );
    }

    #[test]
    fn translates_operator_model_reset_event() {
        let event =
            bridge_event_from_daemon_message(&DaemonMessage::AgentOperatorModelReset { ok: true })
                .expect("operator model reset event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "operator-model-reset",
                "data": {
                    "ok": true
                }
            })
        );
    }

    #[test]
    fn translates_audit_list_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AuditList {
            entries_json: serde_json::json!([
                {
                    "id": 1,
                    "action_type": "tool",
                    "summary": "Executed managed command"
                }
            ])
            .to_string(),
        })
        .expect("audit list event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "audit-list",
                "data": [
                    {
                        "id": 1,
                        "action_type": "tool",
                        "summary": "Executed managed command"
                    }
                ]
            })
        );
    }

    #[test]
    fn translates_provenance_report_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentProvenanceReport {
            report_json: serde_json::json!({
                "total_entries": 3,
                "valid_hash_entries": 3,
                "valid_signature_entries": 2,
                "valid_chain_entries": 3,
                "entries": []
            })
            .to_string(),
        })
        .expect("provenance report event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "provenance-report",
                "data": {
                    "total_entries": 3,
                    "valid_hash_entries": 3,
                    "valid_signature_entries": 2,
                    "valid_chain_entries": 3,
                    "entries": []
                }
            })
        );
    }

    #[test]
    fn translates_memory_provenance_report_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentMemoryProvenanceReport {
            report_json: serde_json::json!({
                "total_entries": 4,
                "summary_by_status": {
                    "active": 3,
                    "uncertain": 1
                },
                "entries": []
            })
            .to_string(),
        })
        .expect("memory provenance report event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "memory-provenance-report",
                "data": {
                    "total_entries": 4,
                    "summary_by_status": {
                        "active": 3,
                        "uncertain": 1
                    },
                    "entries": []
                }
            })
        );
    }

    #[test]
    fn translates_collaboration_sessions_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentCollaborationSessions {
            sessions_json: serde_json::json!([
                {
                    "id": "session-1",
                    "parent_task_id": "task-1",
                    "disagreements": [
                        { "topic": "deployment strategy" }
                    ]
                }
            ])
            .to_string(),
        })
        .expect("collaboration sessions event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "collaboration-sessions",
                "data": [
                    {
                        "id": "session-1",
                        "parent_task_id": "task-1",
                        "disagreements": [
                            { "topic": "deployment strategy" }
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn translates_collaboration_vote_result_event() {
        let event =
            bridge_event_from_daemon_message(&DaemonMessage::AgentCollaborationVoteResult {
                report_json: serde_json::json!({
                    "session_id": "session-1",
                    "resolution": "resolved"
                })
                .to_string(),
            })
            .expect("collaboration vote result should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "collaboration-vote-result",
                "data": {
                    "session_id": "session-1",
                    "resolution": "resolved"
                }
            })
        );
    }

    #[test]
    fn translates_thread_handoff_result_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentThreadHandoffResult {
            result: zorai_protocol::ThreadHandoffResult {
                ok: true,
                thread_id: "thread-1".to_string(),
                active_agent_id: Some("rarog".to_string()),
                stack_depth: Some(2),
                error: None,
            },
        })
        .expect("thread handoff result should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "thread-handoff-result",
                "data": {
                    "ok": true,
                    "thread_id": "thread-1",
                    "active_agent_id": "rarog",
                    "stack_depth": 2,
                    "error": null,
                }
            })
        );
    }

    #[test]
    fn translates_operation_status_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::OperationStatus {
            snapshot: zorai_protocol::OperationStatusSnapshot {
                operation_id: "op-1".to_string(),
                kind: "handoff".to_string(),
                dedup: Some("thread:thread-1".to_string()),
                state: zorai_protocol::OperationLifecycleState::Started,
                revision: 7,
            },
        })
        .expect("operation status should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "operation-status",
                "data": {
                    "operation_id": "op-1",
                    "kind": "handoff",
                    "dedup": "thread:thread-1",
                    "state": "started",
                    "revision": 7,
                }
            })
        );
    }

    #[test]
    fn translates_generated_tools_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentGeneratedTools {
            tools_json: serde_json::json!([
                {
                    "id": "tool-1",
                    "name": "tool-1",
                    "status": "new"
                }
            ])
            .to_string(),
        })
        .expect("generated tools event should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "generated-tools",
                "data": [
                    {
                        "id": "tool-1",
                        "name": "tool-1",
                        "status": "new"
                    }
                ]
            })
        );
    }

    #[test]
    fn translates_generated_tool_result_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentGeneratedToolResult {
            operation_id: Some("op-generated-tool-1".to_string()),
            tool_name: Some("tool-1".to_string()),
            result_json: serde_json::json!({
                "status": "active"
            })
            .to_string(),
        })
        .expect("generated tool result should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "generated-tool-result",
                "data": {
                    "operation_id": "op-generated-tool-1",
                    "tool_name": "tool-1",
                    "result": {
                        "status": "active"
                    }
                }
            })
        );
    }

    #[test]
    fn translates_speech_to_text_result_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentSpeechToTextResult {
            content: serde_json::json!({
                "text": "hello"
            })
            .to_string(),
        })
        .expect("speech-to-text result should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "speech-to-text-result",
                "data": {
                    "text": "hello"
                }
            })
        );
    }

    #[test]
    fn translates_text_to_speech_result_event() {
        let event = bridge_event_from_daemon_message(&DaemonMessage::AgentTextToSpeechResult {
            content: serde_json::json!({
                "path": "/tmp/speech.mp3"
            })
            .to_string(),
        })
        .expect("text-to-speech result should translate");

        assert_eq!(
            event,
            serde_json::json!({
                "type": "text-to-speech-result",
                "data": {
                    "path": "/tmp/speech.mp3"
                }
            })
        );
    }

    #[test]
    fn reassembles_thread_detail_chunks_before_emitting_event() {
        let thread = serde_json::json!({
            "id": "thread-1",
            "messages": [
                { "id": "m1", "role": "user", "content": "hello", "timestamp": 1 }
            ]
        });
        let thread_json = thread.to_string();
        let midpoint = thread_json.len() / 2;

        let mut buffer = Some(ThreadDetailChunkBuffer::default());
        let first = extend_thread_detail_chunk(
            &mut buffer,
            "thread-1".to_string(),
            thread_json.as_bytes()[..midpoint].to_vec(),
            false,
        )
        .expect("first chunk should buffer cleanly");
        assert!(first.is_none());

        let second = extend_thread_detail_chunk(
            &mut buffer,
            "thread-1".to_string(),
            thread_json.as_bytes()[midpoint..].to_vec(),
            true,
        )
        .expect("second chunk should decode cleanly")
        .expect("final chunk should emit an event");

        assert_eq!(
            second,
            serde_json::json!({
                "type": "thread-detail",
                "data": thread,
            })
        );
        assert!(buffer.is_none());
    }
}
