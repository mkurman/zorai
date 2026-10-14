use crate::client::ThreadDetailChunkBuffer;
use crate::client::{ClientEvent, DaemonClient};
use crate::wire::{
    AgentConfigSnapshot, AgentTask, AgentThread, CheckpointSummary, FetchedModel, GoalRun,
    HeartbeatItem, RestoreOutcome, ThreadWorkContext,
};
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{info, warn};
use zorai_protocol::DaemonMessage;
impl DaemonClient {
    pub(crate) async fn handle_thread_workspace_daemon_messages(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
        thread_detail_chunks: &mut Option<ThreadDetailChunkBuffer>,
    ) {
        match message {
            DaemonMessage::AgentEvent { event_json } => {
                match serde_json::from_str::<Value>(&event_json) {
                    Ok(event) => Self::dispatch_agent_event(event, event_tx).await,
                    Err(err) => warn!("Failed to parse agent event: {}", err),
                }
            }
            DaemonMessage::AgentThreadList { threads_json } => {
                match serde_json::from_str::<Vec<AgentThread>>(&threads_json) {
                    Ok(threads) => {
                        let _ = event_tx.send(ClientEvent::ThreadList(threads)).await;
                    }
                    Err(err) => warn!("Failed to parse thread list: {}", err),
                }
            }
            DaemonMessage::AgentThreadDetail { thread_json } => {
                match serde_json::from_str::<Option<AgentThread>>(&thread_json) {
                    Ok(thread) => {
                        let _ = event_tx.send(ClientEvent::ThreadDetail(thread)).await;
                    }
                    Err(err) => warn!("Failed to parse thread detail: {}", err),
                }
            }
            DaemonMessage::AgentThreadHandoffResult { result } => {
                let _ = event_tx
                    .send(ClientEvent::ThreadHandoffResult(result))
                    .await;
            }
            DaemonMessage::AgentGatewayChats { chats_json } => {
                match serde_json::from_str::<Vec<crate::client::GatewayChatVm>>(&chats_json) {
                    Ok(chats) => {
                        let _ = event_tx.send(ClientEvent::GatewayChats(chats)).await;
                    }
                    Err(err) => warn!("Failed to parse gateway chats: {}", err),
                }
            }
            DaemonMessage::AgentThreadDetailChunk {
                thread_id,
                thread_json_chunk,
                done,
            } => {
                let chunk_buffer =
                    thread_detail_chunks.get_or_insert_with(ThreadDetailChunkBuffer::default);
                if chunk_buffer.thread_id.as_deref() != Some(thread_id.as_str()) {
                    chunk_buffer.thread_id = Some(thread_id);
                    chunk_buffer.bytes.clear();
                }
                chunk_buffer.bytes.extend(thread_json_chunk);
                if done {
                    let bytes = std::mem::take(&mut chunk_buffer.bytes);
                    chunk_buffer.thread_id = None;
                    *thread_detail_chunks = None;
                    match String::from_utf8(bytes) {
                        Ok(thread_json) => {
                            match serde_json::from_str::<Option<AgentThread>>(&thread_json) {
                                Ok(thread) => {
                                    let _ = event_tx.send(ClientEvent::ThreadDetail(thread)).await;
                                }
                                Err(err) => {
                                    warn!("Failed to parse streamed thread detail: {}", err)
                                }
                            }
                        }
                        Err(err) => warn!("Failed to decode streamed thread detail: {}", err),
                    }
                }
            }
            DaemonMessage::AgentTaskList { tasks_json } => {
                match serde_json::from_str::<Vec<AgentTask>>(&tasks_json) {
                    Ok(tasks) => {
                        let _ = event_tx.send(ClientEvent::TaskList(tasks)).await;
                    }
                    Err(err) => warn!("Failed to parse task list: {}", err),
                }
            }
            DaemonMessage::AgentWorkspaceSettings { settings } => {
                let _ = event_tx
                    .send(ClientEvent::WorkspaceSettings(settings))
                    .await;
            }
            DaemonMessage::AgentWorkspaceSettingsList { settings } => {
                let _ = event_tx
                    .send(ClientEvent::WorkspaceSettingsList(settings))
                    .await;
            }
            DaemonMessage::AgentWorkspaceTaskList {
                workspace_id,
                tasks,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WorkspaceTaskList {
                        workspace_id,
                        tasks,
                    })
                    .await;
            }
            DaemonMessage::AgentWorkspaceTaskUpdated { task } => {
                let _ = event_tx.send(ClientEvent::WorkspaceTaskUpdated(task)).await;
            }
            DaemonMessage::AgentWorkspaceTaskDeleted {
                task_id,
                deleted_at,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WorkspaceTaskDeleted {
                        task_id,
                        deleted_at,
                    })
                    .await;
            }
            DaemonMessage::AgentWorkspaceNoticeList {
                workspace_id,
                notices,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WorkspaceNotices {
                        workspace_id,
                        notices,
                    })
                    .await;
            }
            DaemonMessage::AgentWorkspaceError { message } => {
                let _ = event_tx.send(ClientEvent::Error(message)).await;
            }
            DaemonMessage::AgentGoalRunList { goal_runs_json } => {
                match serde_json::from_str::<Vec<GoalRun>>(&goal_runs_json) {
                    Ok(goal_runs) => {
                        let _ = event_tx.send(ClientEvent::GoalRunList(goal_runs)).await;
                    }
                    Err(err) => warn!("Failed to parse goal run list: {}", err),
                }
            }
            DaemonMessage::AgentGoalRunStarted { goal_run_json } => {
                match serde_json::from_str::<GoalRun>(&goal_run_json) {
                    Ok(goal_run) => {
                        let _ = event_tx.send(ClientEvent::GoalRunStarted(goal_run)).await;
                    }
                    Err(err) => warn!("Failed to parse goal run started payload: {}", err),
                }
            }
            DaemonMessage::AgentGoalRunDetail { goal_run_json } => {
                match serde_json::from_str::<Option<GoalRun>>(&goal_run_json) {
                    Ok(goal_run) => {
                        let _ = event_tx.send(ClientEvent::GoalRunDetail(goal_run)).await;
                    }
                    Err(err) => warn!("Failed to parse goal run detail: {}", err),
                }
            }
            DaemonMessage::AgentGoalRunDeleted {
                goal_run_id,
                deleted,
            } => {
                let _ = event_tx
                    .send(ClientEvent::GoalRunDeleted {
                        goal_run_id,
                        deleted,
                    })
                    .await;
            }
            DaemonMessage::AgentGoalRunControlled { goal_run_id, ok } => {
                let _ = event_tx
                    .send(ClientEvent::GoalRunControlled { goal_run_id, ok })
                    .await;
            }
            DaemonMessage::AgentCheckpointList {
                goal_run_id,
                checkpoints_json,
            } => match serde_json::from_str::<Vec<CheckpointSummary>>(&checkpoints_json) {
                Ok(checkpoints) => {
                    let _ = event_tx
                        .send(ClientEvent::GoalRunCheckpoints {
                            goal_run_id,
                            checkpoints,
                        })
                        .await;
                }
                Err(err) => warn!("Failed to parse checkpoint list: {}", err),
            },
            DaemonMessage::AgentCheckpointRestored { outcome_json } => {
                match serde_json::from_str::<RestoreOutcome>(&outcome_json) {
                    Ok(outcome) => {
                        let details = format!(
                            "goal {} at step {} • restored {} task(s)",
                            outcome.goal_run_id,
                            outcome.restored_step_index + 1,
                            outcome.tasks_restored
                        );
                        let _ = event_tx
                            .send(ClientEvent::WorkflowNotice {
                                thread_id: None,
                                kind: "checkpoint-restored".to_string(),
                                message: "Checkpoint restored".to_string(),
                                details: Some(details),
                            })
                            .await;
                    }
                    Err(err) => warn!("Failed to parse checkpoint restore outcome: {}", err),
                }
            }
            DaemonMessage::AgentTodoDetail {
                thread_id,
                todos_json,
            } => match serde_json::from_str::<Vec<crate::wire::TodoItem>>(&todos_json) {
                Ok(items) => {
                    let _ = event_tx
                        .send(ClientEvent::ThreadTodos {
                            thread_id,
                            goal_run_id: None,
                            step_index: None,
                            items,
                        })
                        .await;
                }
                Err(err) => warn!("Failed to parse todo detail: {}", err),
            },
            DaemonMessage::AgentWorkContextDetail {
                thread_id: _,
                context_json,
            } => match serde_json::from_str::<ThreadWorkContext>(&context_json) {
                Ok(context) => {
                    let _ = event_tx.send(ClientEvent::WorkContext(context)).await;
                }
                Err(err) => warn!("Failed to parse work context detail: {}", err),
            },
            DaemonMessage::GitDiff {
                repo_path,
                file_path,
                diff,
            } => {
                let _ = event_tx
                    .send(ClientEvent::GitDiff {
                        repo_path,
                        file_path,
                        diff,
                    })
                    .await;
            }
            DaemonMessage::FilePreview {
                path,
                content,
                truncated,
                is_text,
            } => {
                let _ = event_tx
                    .send(ClientEvent::FilePreview {
                        path,
                        content,
                        truncated,
                        is_text,
                    })
                    .await;
            }
            DaemonMessage::DataQueryResult { result_json } => {
                let (path, content) =
                    crate::client::data_query_preview::render_data_query_preview(&result_json);
                let _ = event_tx
                    .send(ClientEvent::FilePreview {
                        path,
                        content,
                        truncated: false,
                        is_text: true,
                    })
                    .await;
            }
            DaemonMessage::AgentConfigResponse { config_json } => {
                info!(
                    json_len = config_json.len(),
                    "client: received AgentConfigResponse"
                );
                match serde_json::from_str::<Value>(&config_json) {
                    Ok(raw) => {
                        match serde_json::from_value::<AgentConfigSnapshot>(raw.clone()) {
                            Ok(config) => {
                                let _ = event_tx.send(ClientEvent::AgentConfig(config)).await;
                            }
                            Err(err) => {
                                warn!(
                                    "AgentConfigSnapshot decode failed — settings UI will not populate. \
                                     This usually means the daemon added a field the TUI's snapshot \
                                     schema doesn't know about. err={}",
                                    err
                                );
                            }
                        }
                        let raw_send = event_tx.send(ClientEvent::AgentConfigRaw(raw)).await;
                        if raw_send.is_err() {
                            warn!("client: AgentConfigRaw event send failed (receiver dropped)");
                        } else {
                            info!("client: AgentConfigRaw forwarded to app event loop");
                        }
                    }
                    Err(err) => warn!("Failed to parse agent config response: {}", err),
                }
            }
            DaemonMessage::AgentMlflowTracingStatus { status_json } => {
                match serde_json::from_str::<Value>(&status_json) {
                    Ok(status) => {
                        let _ = event_tx
                            .send(ClientEvent::MlflowTracingStatus(status))
                            .await;
                    }
                    Err(err) => warn!("Failed to parse MLflow tracing status: {}", err),
                }
            }
            DaemonMessage::AgentMlflowTracingTestResult { result_json } => {
                match serde_json::from_str::<Value>(&result_json) {
                    Ok(result) => {
                        let _ = event_tx
                            .send(ClientEvent::MlflowTracingTestResult(result))
                            .await;
                    }
                    Err(err) => warn!("Failed to parse MLflow tracing test result: {}", err),
                }
            }
            DaemonMessage::AgentMlflowTracingHeaders { .. } => {}
            DaemonMessage::AgentExternalRuntimeMigrationResult { result_json } => {
                match serde_json::from_str::<Value>(&result_json) {
                    Ok(raw) => {
                        let _ = event_tx
                            .send(ClientEvent::ExternalRuntimeMigrationResult(raw))
                            .await;
                    }
                    Err(err) => warn!("Failed to parse external runtime migration result: {}", err),
                }
            }
            DaemonMessage::AgentThreadDeleted { thread_id, deleted } => {
                let _ = event_tx
                    .send(ClientEvent::ThreadDeleted { thread_id, deleted })
                    .await;
            }
            DaemonMessage::AgentThreadExported {
                thread_id,
                file_path,
            } => {
                let _ = event_tx
                    .send(ClientEvent::ThreadExported {
                        thread_id,
                        file_path,
                    })
                    .await;
            }
            DaemonMessage::AgentModelsResponse {
                operation_id: _,
                models_json,
            } => match serde_json::from_str::<Vec<FetchedModel>>(&models_json) {
                Ok(models) => {
                    let _ = event_tx.send(ClientEvent::ModelsFetched(models)).await;
                }
                Err(err) => warn!("Failed to parse models response: {}", err),
            },
            DaemonMessage::AgentHeartbeatItems { items_json } => {
                match serde_json::from_str::<Vec<HeartbeatItem>>(&items_json) {
                    Ok(items) => {
                        let _ = event_tx.send(ClientEvent::HeartbeatItems(items)).await;
                    }
                    Err(err) => warn!("Failed to parse heartbeat items: {}", err),
                }
            }
            DaemonMessage::AgentEventRows { events_json } => {
                match serde_json::from_str::<Vec<zorai_protocol::AgentEventRow>>(&events_json) {
                    Ok(rows) => {
                        let notifications = rows
                            .into_iter()
                            .filter_map(|row| {
                                if row.category != "notification" {
                                    return None;
                                }
                                serde_json::from_str::<zorai_protocol::InboxNotification>(
                                    &row.payload_json,
                                )
                                .ok()
                            })
                            .collect::<Vec<_>>();
                        let _ = event_tx
                            .send(ClientEvent::NotificationSnapshot(notifications))
                            .await;
                    }
                    Err(err) => warn!("Failed to parse agent event rows: {}", err),
                }
            }
            DaemonMessage::AgentDbMessageAck { .. } => {}
            DaemonMessage::SessionSpawned { id } => {
                let _ = event_tx
                    .send(ClientEvent::SessionSpawned {
                        session_id: id.to_string(),
                    })
                    .await;
            }
            DaemonMessage::ApprovalRequired { approval, .. } => {
                let _ = event_tx
                    .send(ClientEvent::ApprovalRequired {
                        approval_id: approval.approval_id,
                        command: approval.command,
                        rationale: Some(approval.rationale),
                        reasons: approval.reasons,
                        risk_level: approval.risk_level,
                        blast_radius: approval.blast_radius,
                    })
                    .await;
            }
            DaemonMessage::ApprovalResolved {
                approval_id,
                decision,
                ..
            } => {
                let _ = event_tx
                    .send(ClientEvent::ApprovalResolved {
                        approval_id,
                        decision: format!("{decision:?}").to_lowercase(),
                    })
                    .await;
            }
            DaemonMessage::AgentTaskApprovalRules { rules } => {
                let _ = event_tx.send(ClientEvent::TaskApprovalRules(rules)).await;
            }
            DaemonMessage::AgentPromptQueue { thread_id, prompts } => {
                let _ = event_tx
                    .send(ClientEvent::PromptQueue { thread_id, prompts })
                    .await;
            }
            _ => unreachable!("thread/workspace daemon message dispatch should be exhaustive"),
        }
    }
}
