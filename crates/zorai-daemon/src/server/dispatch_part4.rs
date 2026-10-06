use super::*;
use crate::agent::AgentEngine;
use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;
use zorai_protocol::{ClientMessage, DaemonMessage};

pub(crate) async fn dispatch_part4(
    msg: &ClientMessage,
    agent: &Arc<AgentEngine>,
    framed: &mut ConnectionWriter,
    background_daemon_queues: &mut BackgroundSubsystemQueues,
    background_daemon_pending: &mut BackgroundPendingCounts,
    client_agent_threads: &mut HashSet<String>,
    agent_event_rx: &mut Option<tokio::sync::broadcast::Receiver<crate::agent::types::AgentEvent>>,
) -> Result<bool> {
    if !matches!(
        msg,
        ClientMessage::AgentRecordAttention { .. }
            | ClientMessage::AgentListThreads { .. }
            | ClientMessage::AgentGetThread { .. }
            | ClientMessage::AgentDeleteThread { .. }
            | ClientMessage::AgentPinThreadMessageForCompaction { .. }
            | ClientMessage::AgentUnpinThreadMessageForCompaction { .. }
            | ClientMessage::AgentMessageFeedback { .. }
            | ClientMessage::AgentAddTask { .. }
            | ClientMessage::AgentCancelTask { .. }
            | ClientMessage::AgentListTasks
            | ClientMessage::AgentListRuns
            | ClientMessage::AgentListRunsForParentThread { .. }
            | ClientMessage::AgentGetRun { .. }
            | ClientMessage::AgentStartGoalRun { .. }
            | ClientMessage::AgentListGoalRuns { .. }
            | ClientMessage::AgentGetGoalRun { .. }
            | ClientMessage::AgentGetGoalRunPage { .. }
            | ClientMessage::AgentControlGoalRun { .. }
            | ClientMessage::AgentDeleteGoalRun { .. }
            | ClientMessage::AgentListTodos
            | ClientMessage::AgentGetTodos { .. }
            | ClientMessage::AgentGetWorkContext { .. }
            | ClientMessage::AgentGetFileOperationSnapshot { .. }
            | ClientMessage::AgentRevertFileOperation { .. }
            | ClientMessage::AgentGetThreadWorkspaceContext { .. }
            | ClientMessage::AgentSetThreadWorkspaceContext { .. }
            | ClientMessage::AgentSpawnSubagent { .. }
            | ClientMessage::AgentListTools { .. }
            | ClientMessage::AgentSearchTools { .. }
            | ClientMessage::AgentGetConfig
            | ClientMessage::AgentGetMlflowTracingStatus
            | ClientMessage::AgentTestMlflowTracingConnection
            | ClientMessage::AgentSendMlflowTracingTestTrace
            | ClientMessage::AgentListMlflowTracingHeaders
            | ClientMessage::AgentSetMlflowTracingHeader { .. }
            | ClientMessage::AgentDeleteMlflowTracingHeader { .. }
            | ClientMessage::AgentExternalRuntimeMigrationStatus
            | ClientMessage::AgentExternalRuntimeMigrationPreview { .. }
            | ClientMessage::AgentExternalRuntimeMigrationApply { .. }
            | ClientMessage::AgentExternalRuntimeMigrationReport { .. }
            | ClientMessage::AgentExternalRuntimeMigrationShadowRun { .. }
            | ClientMessage::AgentGetGatewayConfig
            | ClientMessage::AgentGetEffectiveConfigState
            | ClientMessage::AgentListTaskApprovalRules
            | ClientMessage::AgentCreateTaskApprovalRule { .. }
            | ClientMessage::AgentRevokeTaskApprovalRule { .. }
            | ClientMessage::AgentSetConfigItem { .. }
            | ClientMessage::AgentSetProviderModel { .. }
            | ClientMessage::AgentSetTargetAgentProviderModel { .. }
            | ClientMessage::AgentSetTargetAgentReasoningEffort { .. }
            | ClientMessage::AgentSetTargetAgentContextWindow { .. }
            | ClientMessage::AgentGetThreadExecutionProfile { .. }
            | ClientMessage::AgentSetThreadExecutionProfile { .. }
            | ClientMessage::AgentFetchModels { .. }
            | ClientMessage::AgentHeartbeatGetItems
            | ClientMessage::AgentHeartbeatSetItems { .. }
            | ClientMessage::AgentResolveTaskApproval { .. }
            | ClientMessage::AgentSubscribe
            | ClientMessage::AgentUnsubscribe
            | ClientMessage::AgentDeclareAsyncCommandCapability { .. }
            | ClientMessage::AgentGetOperationStatus { .. }
            | ClientMessage::AgentHandoffThread { .. }
            | ClientMessage::AgentGetSubagentMetrics { .. }
            | ClientMessage::AgentGetSubsystemMetrics
            | ClientMessage::AgentListCheckpoints { .. }
            | ClientMessage::AgentRestoreCheckpoint { .. }
            | ClientMessage::AgentGetHealthStatus
            | ClientMessage::AgentListWorkspaceSettings
            | ClientMessage::AgentGetWorkspaceSettings { .. }
            | ClientMessage::AgentSetWorkspaceOperator { .. }
            | ClientMessage::AgentCreateWorkspaceTask { .. }
            | ClientMessage::AgentListWorkspaceTasks { .. }
            | ClientMessage::AgentGetWorkspaceTask { .. }
            | ClientMessage::AgentUpdateWorkspaceTask { .. }
            | ClientMessage::AgentMoveWorkspaceTask { .. }
            | ClientMessage::AgentRunWorkspaceTask { .. }
            | ClientMessage::AgentPauseWorkspaceTask { .. }
            | ClientMessage::AgentStopWorkspaceTask { .. }
            | ClientMessage::AgentDeleteWorkspaceTask { .. }
            | ClientMessage::AgentSubmitWorkspaceReview { .. }
            | ClientMessage::AgentListWorkspaceNotices { .. }
    ) {
        return Ok(false);
    }
    let msg = msg.clone();

    match msg {
        ClientMessage::AgentRecordAttention {
            surface,
            thread_id,
            goal_run_id,
        } => {
            if let Err(e) = agent
                .record_operator_attention(&surface, thread_id.as_deref(), goal_run_id.as_deref())
                .await
            {
                framed
                    .send(DaemonMessage::AgentError {
                        message: format!("failed to record attention surface: {e}"),
                    })
                    .await
                    .ok();
            }
        }

        ClientMessage::AgentListThreads {
            limit,
            offset,
            include_internal,
            agent_filter,
        } => {
            let threads = if agent_filter.is_some() {
                agent
                    .list_threads_filtered(&crate::agent::thread_crud::ThreadListFilter {
                        agent_name: agent_filter,
                        include_internal,
                        limit: limit.map(|value| value.min(500)),
                        offset: 0,
                        ..crate::agent::thread_crud::ThreadListFilter::default()
                    })
                    .await
            } else {
                agent
                    .list_threads_paginated(limit, offset.unwrap_or(0), include_internal)
                    .await
            };
            let (threads, truncated) = cap_agent_thread_list_for_ipc(threads);
            if truncated {
                tracing::warn!("truncated agent thread list to fit IPC frame limit");
            }
            let json = serde_json::to_string(&threads).unwrap_or_default();
            tracing::debug!(
                thread_count = threads.len(),
                payload_bytes = json.len(),
                "sending agent thread list to client"
            );
            framed
                .send(DaemonMessage::AgentThreadList { threads_json: json })
                .await?;
        }

        ClientMessage::AgentGetThread {
            thread_id,
            message_limit,
            message_offset,
            collapse_tool_calls,
        } => {
            let started = std::time::Instant::now();
            tracing::info!(
                thread_id = %thread_id,
                message_limit,
                message_offset,
                "daemon received thread message request"
            );
            client_agent_threads.insert(thread_id.clone());
            let json = agent
                .agent_thread_detail_json(
                    &thread_id,
                    message_limit,
                    message_offset,
                    collapse_tool_calls,
                )
                .await;
            tracing::info!(
                thread_id = %thread_id,
                build_ms = started.elapsed().as_millis() as u64,
                json_len = json.len(),
                "dispatch: AgentGetThread detail built"
            );
            if thread_detail_fits_single_ipc_frame(&json) {
                framed
                    .send(DaemonMessage::AgentThreadDetail { thread_json: json })
                    .await?;
            } else {
                let chunks = thread_detail_chunks_for_ipc(&json).collect::<Vec<_>>();
                let total_chunks = chunks.len();
                for (index, chunk) in chunks.into_iter().enumerate() {
                    framed
                        .send(DaemonMessage::AgentThreadDetailChunk {
                            thread_id: thread_id.clone(),
                            thread_json_chunk: chunk.to_vec(),
                            done: index + 1 == total_chunks,
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentDeleteThread { thread_id } => {
            client_agent_threads.remove(&thread_id);
            let deleted = agent.delete_thread(&thread_id).await;
            framed
                .send(DaemonMessage::AgentThreadDeleted { thread_id, deleted })
                .await?;
        }

        ClientMessage::AgentPinThreadMessageForCompaction {
            thread_id,
            message_id,
        } => {
            let result = agent
                .pin_thread_message_for_compaction(&thread_id, &message_id)
                .await;
            let result_json = serde_json::to_string(&result).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentThreadMessagePinResult { result_json })
                .await?;
        }

        ClientMessage::AgentUnpinThreadMessageForCompaction {
            thread_id,
            message_id,
        } => {
            let result = agent
                .unpin_thread_message_for_compaction(&thread_id, &message_id)
                .await;
            let result_json = serde_json::to_string(&result).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentThreadMessagePinResult { result_json })
                .await?;
        }

        ClientMessage::AgentMessageFeedback {
            thread_id,
            message_id,
            absolute_message_index,
            reaction,
        } => {
            match agent
                .apply_message_feedback(&thread_id, &message_id, absolute_message_index, reaction)
                .await
            {
                Ok(_resolved) => {
                    // Resolved state is fanned out via AgentEvent::MessageFeedbackUpdated
                    // so all subscribed clients (including the caller) converge on the
                    // daemon-resolved value rather than the optimistic desired one.
                }
                Err(reason) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: format!("message_feedback:{reason}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentAddTask {
            title,
            description,
            priority,
            command,
            session_id,
            scheduled_at,
            dependencies,
        } => {
            let task = agent
                .enqueue_task(
                    title,
                    description,
                    &priority,
                    command,
                    session_id,
                    dependencies,
                    scheduled_at,
                    "user",
                    None,
                    None,
                    None,
                    None,
                )
                .await;
            tracing::info!(task_id = %task.id, "agent task added");
            let json = serde_json::to_string(&task).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentTaskEnqueued { task_json: json })
                .await?;
        }

        ClientMessage::AgentCancelTask { task_id } => {
            let cancelled = agent.cancel_task(&task_id).await;
            framed
                .send(DaemonMessage::AgentTaskCancelled { task_id, cancelled })
                .await?;
        }

        ClientMessage::AgentListTasks => {
            let started = std::time::Instant::now();
            let (tasks, truncated) = agent.list_tasks_metadata_only_capped_for_ipc().await;
            let read_ms = started.elapsed().as_millis() as u64;
            if truncated {
                tracing::warn!("truncated task list to fit IPC frame limit");
            }
            let json = serde_json::to_string(&tasks).unwrap_or_default();
            let json_len = json.len();
            framed
                .send(DaemonMessage::AgentTaskList { tasks_json: json })
                .await?;
            tracing::info!(
                read_ms,
                send_ms = started.elapsed().as_millis() as u64,
                json_len,
                "dispatch: AgentListTasks done"
            );
        }

        ClientMessage::AgentListRuns => {
            let runs = agent.list_runs().await;
            let json = serde_json::to_string(&runs).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentRunList { runs_json: json })
                .await?;
        }

        ClientMessage::AgentListRunsForParentThread { parent_thread_id } => {
            let runs = agent
                .list_runs_for_parent_thread(parent_thread_id.trim())
                .await;
            let json = serde_json::to_string(&runs).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentRunList { runs_json: json })
                .await?;
        }

        ClientMessage::AgentGetRun { run_id } => {
            let run = agent.get_run(&run_id).await;
            let json = serde_json::to_string(&run).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentRunDetail { run_json: json })
                .await?;
        }

        ClientMessage::AgentStartGoalRun {
            goal,
            title,
            thread_id,
            session_id,
            priority,
            client_request_id,
            launch_assignments,
            autonomy_level,
            client_surface,
            target_agent_id,
            requires_approval,
        } => {
            if let (Some(thread_id), Some(target_agent_id)) =
                (thread_id.as_deref(), target_agent_id.as_deref())
            {
                let _ = agent
                    .get_or_create_thread_with_target(
                        Some(thread_id),
                        title.as_deref().unwrap_or(&goal),
                        Some(target_agent_id),
                    )
                    .await;
            }
            let goal_run = agent
                .start_goal_run_with_surface_and_approval_policy(
                    goal,
                    title,
                    thread_id,
                    session_id,
                    priority.as_deref(),
                    client_request_id,
                    autonomy_level,
                    client_surface,
                    requires_approval,
                    if launch_assignments.is_empty() {
                        None
                    } else {
                        Some(
                            launch_assignments
                                .into_iter()
                                .map(|assignment| crate::agent::types::GoalAgentAssignment {
                                    role_id: assignment.role_id,
                                    enabled: assignment.enabled,
                                    provider: assignment.provider,
                                    model: assignment.model,
                                    reasoning_effort: assignment.reasoning_effort,
                                    inherit_from_main: assignment.inherit_from_main,
                                })
                                .collect(),
                        )
                    },
                )
                .await;
            if let Some(thread_id) = goal_run.thread_id.clone() {
                client_agent_threads.insert(thread_id);
            }
            let json = serde_json::to_string(&goal_run).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentGoalRunStarted {
                    goal_run_json: json,
                })
                .await?;
        }

        ClientMessage::AgentListGoalRuns { limit, offset } => {
            let started = std::time::Instant::now();
            let (goal_runs, truncated) = agent
                .list_goal_runs_paginated_capped_for_ipc(limit, offset)
                .await;
            let read_ms = started.elapsed().as_millis() as u64;
            if truncated {
                tracing::warn!("truncated goal run list to fit IPC frame limit");
            }
            let json = serde_json::to_string(&goal_runs).unwrap_or_default();
            let json_len = json.len();
            framed
                .send(DaemonMessage::AgentGoalRunList {
                    goal_runs_json: json,
                })
                .await?;
            tracing::info!(
                read_ms,
                send_ms = started.elapsed().as_millis() as u64,
                json_len,
                "dispatch: AgentListGoalRuns done"
            );
        }

        ClientMessage::AgentGetGoalRun { goal_run_id } => {
            let detail = agent.get_goal_run_capped_for_ipc(&goal_run_id).await;
            if detail.as_ref().is_some_and(|(_, truncated)| *truncated) {
                tracing::warn!(
                    goal_run_id = %goal_run_id,
                    "truncated goal run detail to fit IPC frame limit"
                );
            }
            let json = detail
                .map(|(goal_run_json, _)| goal_run_json)
                .unwrap_or_else(|| {
                    serde_json::json!({
                        "id": goal_run_id.clone(),
                    })
                    .to_string()
                });
            framed
                .send(DaemonMessage::AgentGoalRunDetail {
                    goal_run_json: json,
                })
                .await?;
        }

        ClientMessage::AgentGetGoalRunPage {
            goal_run_id,
            step_offset,
            step_limit,
            event_offset,
            event_limit,
        } => {
            let detail = agent
                .get_goal_run_page_capped_for_ipc(
                    &goal_run_id,
                    step_offset,
                    step_limit,
                    event_offset,
                    event_limit,
                )
                .await;
            if detail.as_ref().is_some_and(|(_, truncated)| *truncated) {
                tracing::warn!(
                    goal_run_id = %goal_run_id,
                    "truncated goal run detail page to fit IPC frame limit"
                );
            }
            let json = detail
                .map(|(goal_run_json, _)| goal_run_json)
                .unwrap_or_else(|| {
                    serde_json::json!({
                        "id": goal_run_id.clone(),
                    })
                    .to_string()
                });
            framed
                .send(DaemonMessage::AgentGoalRunDetail {
                    goal_run_json: json,
                })
                .await?;
        }

        ClientMessage::AgentControlGoalRun {
            goal_run_id,
            action,
            step_index,
            payload_json,
        } => {
            let ok = agent
                .control_goal_run(&goal_run_id, &action, step_index, payload_json.as_deref())
                .await;
            framed
                .send(DaemonMessage::AgentGoalRunControlled { goal_run_id, ok })
                .await?;
        }

        ClientMessage::AgentDeleteGoalRun { goal_run_id } => {
            let deleted = agent.delete_goal_run(&goal_run_id).await;
            framed
                .send(DaemonMessage::AgentGoalRunDeleted {
                    goal_run_id,
                    deleted,
                })
                .await?;
        }

        ClientMessage::AgentListTodos => {
            let (todos, truncated) = agent.list_todos_capped_for_ipc().await;
            if truncated {
                tracing::warn!("truncated todo list to fit IPC frame limit");
            }
            let json = serde_json::to_string(&todos).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentTodoList { todos_json: json })
                .await?;
        }

        ClientMessage::AgentGetTodos { thread_id } => {
            client_agent_threads.insert(thread_id.clone());
            let (todos, truncated) = agent.get_todos_capped_for_ipc(&thread_id).await;
            if truncated {
                tracing::warn!(
                    thread_id = %thread_id,
                    "truncated todo detail to fit IPC frame limit"
                );
            }
            let json = serde_json::to_string(&todos).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentTodoDetail {
                    thread_id,
                    todos_json: json,
                })
                .await?;
        }

        ClientMessage::AgentGetWorkContext { thread_id } => {
            client_agent_threads.insert(thread_id.clone());
            let (context, truncated) = agent.get_work_context_capped_for_ipc(&thread_id).await;
            if truncated {
                tracing::warn!(
                    thread_id = %thread_id,
                    "truncated work context detail to fit IPC frame limit"
                );
            }
            let json = serde_json::to_string(&context).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentWorkContextDetail {
                    thread_id,
                    context_json: json,
                })
                .await?;
        }

        ClientMessage::AgentGetFileOperationSnapshot { operation_id } => {
            let status = agent.file_operation_snapshot_status(&operation_id).await;
            framed
                .send(DaemonMessage::AgentFileOperationSnapshot {
                    operation_id,
                    status_json: serde_json::to_string(&status).unwrap_or_default(),
                })
                .await?;
        }

        ClientMessage::AgentRevertFileOperation { operation_id } => {
            match agent.revert_file_operation(&operation_id).await {
                Ok(result) => {
                    framed
                        .send(DaemonMessage::AgentFileOperationReverted {
                            operation_id,
                            result_json: serde_json::to_string(&result).unwrap_or_default(),
                        })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentGetThreadWorkspaceContext { thread_id } => {
            client_agent_threads.insert(thread_id.clone());
            let context = agent.get_thread_workspace_context(&thread_id).await;
            framed
                .send(DaemonMessage::AgentThreadWorkspaceContext {
                    thread_id,
                    context_json: serde_json::to_string(&context)
                        .unwrap_or_else(|_| "null".to_string()),
                    updated: false,
                })
                .await?;
        }

        ClientMessage::AgentSetThreadWorkspaceContext {
            thread_id,
            context_json,
        } => {
            client_agent_threads.insert(thread_id.clone());
            let parsed =
                serde_json::from_str::<crate::agent::types::ThreadWorkspaceContext>(&context_json);
            match parsed {
                Ok(context) => {
                    let updated = agent
                        .set_thread_workspace_context(&thread_id, Some(context.clone()))
                        .await;
                    framed
                        .send(DaemonMessage::AgentThreadWorkspaceContext {
                            thread_id,
                            context_json: serde_json::to_string(&Some(context))
                                .unwrap_or_else(|_| "null".to_string()),
                            updated,
                        })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("invalid thread workspace context: {error}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentSpawnSubagent {
            thread_id,
            args_json,
        } => {
            client_agent_threads.insert(thread_id.clone());
            let args = serde_json::from_str::<serde_json::Value>(&args_json)
                .map_err(|error| anyhow::anyhow!("invalid spawn subagent arguments: {error}"))?;
            match crate::agent::tool_executor::execute_spawn_subagent(
                &args,
                agent,
                &thread_id,
                None,
                &agent.session_manager,
                None,
                &agent.event_tx,
            )
            .await
            {
                Ok(content) => {
                    framed
                        .send(DaemonMessage::AgentSubagentSpawned {
                            thread_id,
                            result_json: serde_json::json!({ "ok": true, "content": content })
                                .to_string(),
                        })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentSubagentSpawned {
                            thread_id,
                            result_json:
                                serde_json::json!({ "ok": false, "error": error.to_string() })
                                    .to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentListTools { limit, offset } => {
            let config = agent.config.read().await;
            let has_workspace_topology = agent.session_manager.read_workspace_topology().is_some();
            let result = crate::agent::tool_executor::list_available_tools_public(
                &config,
                &agent.data_dir,
                has_workspace_topology,
                None,
                limit.unwrap_or(50),
                offset.unwrap_or(0),
            );
            framed.send(DaemonMessage::AgentToolList { result }).await?;
        }

        ClientMessage::AgentSearchTools {
            query,
            limit,
            offset,
        } => {
            let config = agent.config.read().await;
            let has_workspace_topology = agent.session_manager.read_workspace_topology().is_some();
            let result = crate::agent::tool_executor::search_available_tools_public(
                &config,
                &agent.data_dir,
                has_workspace_topology,
                None,
                &query,
                limit.unwrap_or(20),
                offset.unwrap_or(0),
            );
            framed
                .send(DaemonMessage::AgentToolSearchResult { result })
                .await?;
        }

        ClientMessage::AgentGetConfig => {
            let started = std::time::Instant::now();
            tracing::info!("dispatch: AgentGetConfig received");
            let config = agent.get_config().await;
            let read_ms = started.elapsed().as_millis() as u64;
            let json = serde_json::to_string(&config).unwrap_or_default();
            let ser_ms = started.elapsed().as_millis() as u64;
            let json_len = json.len();
            framed
                .send(DaemonMessage::AgentConfigResponse { config_json: json })
                .await?;
            tracing::info!(
                read_ms,
                ser_ms,
                send_ms = started.elapsed().as_millis() as u64,
                json_len,
                "dispatch: AgentConfigResponse sent"
            );
        }

        ClientMessage::AgentGetMlflowTracingStatus => {
            let status_json = serde_json::to_string(&agent.mlflow_tracing.status())?;
            framed
                .send(DaemonMessage::AgentMlflowTracingStatus { status_json })
                .await?;
        }

        ClientMessage::AgentTestMlflowTracingConnection => {
            let result = agent.mlflow_tracing.test_connection().await;
            let result_json = serde_json::to_string(&match result {
                Ok(info) => serde_json::json!({ "ok": true, "connection": info }),
                Err(error) => serde_json::json!({ "ok": false, "error": error }),
            })?;
            framed
                .send(DaemonMessage::AgentMlflowTracingTestResult { result_json })
                .await?;
        }

        ClientMessage::AgentSendMlflowTracingTestTrace => {
            let result = agent.mlflow_tracing.send_diagnostic_trace().await;
            let result_json = serde_json::to_string(&match result {
                Ok(info) => serde_json::json!({ "ok": true, "connection": info }),
                Err(error) => serde_json::json!({ "ok": false, "error": error }),
            })?;
            framed
                .send(DaemonMessage::AgentMlflowTracingTestResult { result_json })
                .await?;
        }

        ClientMessage::AgentListMlflowTracingHeaders => {
            let names = agent.mlflow_tracing.header_store().list_names()?;
            framed
                .send(DaemonMessage::AgentMlflowTracingHeaders { names })
                .await?;
        }

        ClientMessage::AgentSetMlflowTracingHeader { name, value } => {
            agent.mlflow_tracing.header_store().set(&name, &value)?;
            agent.mlflow_tracing.refresh_headers().await;
            let names = agent.mlflow_tracing.header_store().list_names()?;
            framed
                .send(DaemonMessage::AgentMlflowTracingHeaders { names })
                .await?;
        }

        ClientMessage::AgentDeleteMlflowTracingHeader { name } => {
            agent.mlflow_tracing.header_store().delete(&name)?;
            agent.mlflow_tracing.refresh_headers().await;
            let names = agent.mlflow_tracing.header_store().list_names()?;
            framed
                .send(DaemonMessage::AgentMlflowTracingHeaders { names })
                .await?;
        }

        ClientMessage::AgentExternalRuntimeMigrationStatus => {
            let payload = agent.external_runtime_migration_status_json().await;
            framed
                .send(DaemonMessage::AgentExternalRuntimeMigrationResult {
                    result_json: serde_json::to_string(&payload)
                        .unwrap_or_else(|_| "{}".to_string()),
                })
                .await?;
        }

        ClientMessage::AgentExternalRuntimeMigrationPreview {
            runtime,
            config_path,
        } => {
            match agent
                .import_external_runtime_json(
                    &runtime,
                    config_path.as_deref(),
                    true,
                    crate::agent::types::ExternalRuntimeConflictPolicy::StageForReview,
                )
                .await
            {
                Ok(payload) => {
                    framed
                        .send(DaemonMessage::AgentExternalRuntimeMigrationResult {
                            result_json: serde_json::to_string(&payload)
                                .unwrap_or_else(|_| "{}".to_string()),
                        })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentExternalRuntimeMigrationApply {
            runtime,
            config_path,
            conflict_policy,
        } => {
            let policy = conflict_policy
                .parse::<crate::agent::types::ExternalRuntimeConflictPolicy>()
                .unwrap_or_default();
            match agent
                .import_external_runtime_json(&runtime, config_path.as_deref(), false, policy)
                .await
            {
                Ok(payload) => {
                    framed
                        .send(DaemonMessage::AgentExternalRuntimeMigrationResult {
                            result_json: serde_json::to_string(&payload)
                                .unwrap_or_else(|_| "{}".to_string()),
                        })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentExternalRuntimeMigrationReport { runtime, limit } => {
            match agent
                .show_import_report_json(runtime.as_deref(), limit.unwrap_or(20))
                .await
            {
                Ok(payload) => {
                    framed
                        .send(DaemonMessage::AgentExternalRuntimeMigrationResult {
                            result_json: serde_json::to_string(&payload)
                                .unwrap_or_else(|_| "{}".to_string()),
                        })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentExternalRuntimeMigrationShadowRun { runtime } => {
            match agent.preview_shadow_run_json(&runtime).await {
                Ok(payload) => {
                    framed
                        .send(DaemonMessage::AgentExternalRuntimeMigrationResult {
                            result_json: serde_json::to_string(&payload)
                                .unwrap_or_else(|_| "{}".to_string()),
                        })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentGetGatewayConfig => {
            let gateway = agent.get_config().await.gateway;
            let json = serde_json::to_string(&gateway).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentGatewayConfig { config_json: json })
                .await?;
        }

        ClientMessage::AgentGetEffectiveConfigState => {
            let state = agent.current_effective_config_runtime_state().await;
            let json = serde_json::to_string(&state).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentEffectiveConfigState { state_json: json })
                .await?;
        }

        ClientMessage::AgentGetSubsystemMetrics => {
            let metrics_json = subsystem_metrics().all_snapshots_json();
            framed
                .send(DaemonMessage::AgentSubsystemMetrics { metrics_json })
                .await?;
        }

        ClientMessage::AgentSetConfigItem {
            key_path,
            value_json,
        } => match agent.prepare_config_item_json(&key_path, &value_json).await {
            Ok((merged, value)) => {
                if !background_daemon_pending.has_capacity(BackgroundSubsystem::ConfigReconcile) {
                    background_daemon_pending.note_rejection(BackgroundSubsystem::ConfigReconcile);
                    framed
                        .send(DaemonMessage::Error {
                            message: "config_reconcile background queue is full".to_string(),
                        })
                        .await?;
                    return Ok(true);
                }

                if let Err(e) = agent
                    .persist_prepared_config_item_json(&key_path, &value, merged)
                    .await
                {
                    tracing::warn!(error = %e, key_path, "server: AgentSetConfigItem persist failed");
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("Invalid config item: {e}"),
                        })
                        .await?;
                    return Ok(true);
                }

                if crate::agent::AgentEngine::config_item_affects_swarog_execution_profile(
                    &key_path,
                ) {
                    agent
                        .sync_thread_execution_profiles_for_agent(
                            crate::agent::agent_identity::MAIN_AGENT_ID,
                        )
                        .await;
                }

                let operation = operation_registry().accept_operation(
                    OPERATION_KIND_CONFIG_SET_ITEM,
                    Some(config_set_item_dedup_key(&agent, &key_path, &value_json)),
                );

                framed
                    .send(DaemonMessage::OperationAccepted {
                        operation_id: operation.operation_id.clone(),
                        kind: operation.kind.clone(),
                        dedup: operation.dedup.clone(),
                        revision: operation.revision,
                    })
                    .await?;

                let agent = agent.clone();
                let background_daemon_tx =
                    background_daemon_queues.sender(BackgroundSubsystem::ConfigReconcile);
                spawn_background_side_effect(
                    BackgroundSubsystem::ConfigReconcile,
                    Some(operation.operation_id.clone()),
                    background_daemon_tx,
                    background_daemon_pending,
                    async move {
                        match agent.reconcile_config_runtime_after_commit().await {
                            Ok(()) => BackgroundSideEffectOutcome::Completed,
                            Err(_) => BackgroundSideEffectOutcome::Failed,
                        }
                    },
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, key_path, "server: AgentSetConfigItem rejected");
                framed
                    .send(DaemonMessage::Error {
                        message: format!("Invalid config item: {e}"),
                    })
                    .await?;
            }
        },

        ClientMessage::AgentSetProviderModel { provider_id, model } => {
            match agent
                .prepare_provider_model_json(&provider_id, &model)
                .await
            {
                Ok(merged) => {
                    if !background_daemon_pending.has_capacity(BackgroundSubsystem::ConfigReconcile)
                    {
                        background_daemon_pending
                            .note_rejection(BackgroundSubsystem::ConfigReconcile);
                        framed
                            .send(DaemonMessage::Error {
                                message: "config_reconcile background queue is full".to_string(),
                            })
                            .await?;
                        return Ok(true);
                    }

                    agent.persist_prepared_provider_model_json(merged).await;
                    agent
                        .sync_thread_execution_profiles_for_agent(
                            crate::agent::agent_identity::MAIN_AGENT_ID,
                        )
                        .await;

                    let operation = operation_registry().accept_operation(
                        OPERATION_KIND_SET_PROVIDER_MODEL,
                        Some(set_provider_model_dedup_key(&agent, &provider_id, &model)),
                    );

                    framed
                        .send(DaemonMessage::OperationAccepted {
                            operation_id: operation.operation_id.clone(),
                            kind: operation.kind.clone(),
                            dedup: operation.dedup.clone(),
                            revision: operation.revision,
                        })
                        .await?;

                    let agent = agent.clone();
                    let background_daemon_tx =
                        background_daemon_queues.sender(BackgroundSubsystem::ConfigReconcile);
                    spawn_background_side_effect(
                        BackgroundSubsystem::ConfigReconcile,
                        Some(operation.operation_id.clone()),
                        background_daemon_tx,
                        background_daemon_pending,
                        async move {
                            match agent.reconcile_config_runtime_after_commit().await {
                                Ok(()) => BackgroundSideEffectOutcome::Completed,
                                Err(_) => BackgroundSideEffectOutcome::Failed,
                            }
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(error = %e, provider_id, model, "server: AgentSetProviderModel rejected");
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("Invalid provider/model selection: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentSetTargetAgentProviderModel {
            target_agent_id,
            provider_id,
            model,
        } => {
            match agent
                .prepare_agent_provider_model_json(&target_agent_id, &provider_id, &model)
                .await
            {
                Ok(merged) => {
                    if !background_daemon_pending.has_capacity(BackgroundSubsystem::ConfigReconcile)
                    {
                        background_daemon_pending
                            .note_rejection(BackgroundSubsystem::ConfigReconcile);
                        framed
                            .send(DaemonMessage::Error {
                                message: "config_reconcile background queue is full".to_string(),
                            })
                            .await?;
                        return Ok(true);
                    }

                    agent.persist_prepared_provider_model_json(merged).await;
                    agent
                        .sync_thread_execution_profiles_for_agent(&target_agent_id)
                        .await;

                    let operation = operation_registry().accept_operation(
                        OPERATION_KIND_SET_PROVIDER_MODEL,
                        Some(set_target_agent_provider_model_dedup_key(
                            &agent,
                            &target_agent_id,
                            &provider_id,
                            &model,
                        )),
                    );

                    framed
                        .send(DaemonMessage::OperationAccepted {
                            operation_id: operation.operation_id.clone(),
                            kind: operation.kind.clone(),
                            dedup: operation.dedup.clone(),
                            revision: operation.revision,
                        })
                        .await?;

                    let agent = agent.clone();
                    let background_daemon_tx =
                        background_daemon_queues.sender(BackgroundSubsystem::ConfigReconcile);
                    spawn_background_side_effect(
                        BackgroundSubsystem::ConfigReconcile,
                        Some(operation.operation_id.clone()),
                        background_daemon_tx,
                        background_daemon_pending,
                        async move {
                            match agent.reconcile_config_runtime_after_commit().await {
                                Ok(()) => BackgroundSideEffectOutcome::Completed,
                                Err(_) => BackgroundSideEffectOutcome::Failed,
                            }
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        target_agent_id,
                        provider_id,
                        model,
                        "server: AgentSetTargetAgentProviderModel rejected"
                    );
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("Invalid target agent provider/model selection: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentSetTargetAgentReasoningEffort {
            target_agent_id,
            reasoning_effort,
        } => {
            match agent
                .prepare_agent_reasoning_effort_json(&target_agent_id, &reasoning_effort)
                .await
            {
                Ok(merged) => {
                    if !background_daemon_pending.has_capacity(BackgroundSubsystem::ConfigReconcile)
                    {
                        background_daemon_pending
                            .note_rejection(BackgroundSubsystem::ConfigReconcile);
                        framed
                            .send(DaemonMessage::Error {
                                message: "config_reconcile background queue is full".to_string(),
                            })
                            .await?;
                        return Ok(true);
                    }

                    agent.persist_prepared_provider_model_json(merged).await;
                    agent
                        .sync_thread_execution_profiles_for_agent(&target_agent_id)
                        .await;

                    let operation = operation_registry().accept_operation(
                        OPERATION_KIND_SET_PROVIDER_MODEL,
                        Some(set_target_agent_reasoning_effort_dedup_key(
                            &agent,
                            &target_agent_id,
                            &reasoning_effort,
                        )),
                    );

                    framed
                        .send(DaemonMessage::OperationAccepted {
                            operation_id: operation.operation_id.clone(),
                            kind: operation.kind.clone(),
                            dedup: operation.dedup.clone(),
                            revision: operation.revision,
                        })
                        .await?;

                    let agent = agent.clone();
                    let background_daemon_tx =
                        background_daemon_queues.sender(BackgroundSubsystem::ConfigReconcile);
                    spawn_background_side_effect(
                        BackgroundSubsystem::ConfigReconcile,
                        Some(operation.operation_id.clone()),
                        background_daemon_tx,
                        background_daemon_pending,
                        async move {
                            match agent.reconcile_config_runtime_after_commit().await {
                                Ok(()) => BackgroundSideEffectOutcome::Completed,
                                Err(_) => BackgroundSideEffectOutcome::Failed,
                            }
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        target_agent_id,
                        reasoning_effort,
                        "server: AgentSetTargetAgentReasoningEffort rejected"
                    );
                    framed
                        .send(DaemonMessage::Error {
                            message: format!(
                                "Invalid target agent reasoning effort selection: {e}"
                            ),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentSetTargetAgentContextWindow {
            target_agent_id,
            context_window_tokens,
        } => {
            match agent
                .prepare_agent_context_window_json(&target_agent_id, context_window_tokens)
                .await
            {
                Ok(merged) => {
                    if !background_daemon_pending.has_capacity(BackgroundSubsystem::ConfigReconcile)
                    {
                        background_daemon_pending
                            .note_rejection(BackgroundSubsystem::ConfigReconcile);
                        framed
                            .send(DaemonMessage::Error {
                                message: "config_reconcile background queue is full".to_string(),
                            })
                            .await?;
                        return Ok(true);
                    }

                    agent.persist_prepared_provider_model_json(merged).await;
                    agent
                        .set_owned_thread_execution_profile_context(
                            &target_agent_id,
                            context_window_tokens,
                        )
                        .await;
                    agent
                        .sync_thread_execution_profiles_for_agent(&target_agent_id)
                        .await;

                    let operation = operation_registry().accept_operation(
                        OPERATION_KIND_SET_PROVIDER_MODEL,
                        Some(set_target_agent_context_window_dedup_key(
                            &agent,
                            &target_agent_id,
                            context_window_tokens,
                        )),
                    );

                    framed
                        .send(DaemonMessage::OperationAccepted {
                            operation_id: operation.operation_id.clone(),
                            kind: operation.kind.clone(),
                            dedup: operation.dedup.clone(),
                            revision: operation.revision,
                        })
                        .await?;

                    let agent = agent.clone();
                    let background_daemon_tx =
                        background_daemon_queues.sender(BackgroundSubsystem::ConfigReconcile);
                    spawn_background_side_effect(
                        BackgroundSubsystem::ConfigReconcile,
                        Some(operation.operation_id.clone()),
                        background_daemon_tx,
                        background_daemon_pending,
                        async move {
                            match agent.reconcile_config_runtime_after_commit().await {
                                Ok(()) => BackgroundSideEffectOutcome::Completed,
                                Err(_) => BackgroundSideEffectOutcome::Failed,
                            }
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        target_agent_id,
                        context_window_tokens,
                        "server: AgentSetTargetAgentContextWindow rejected"
                    );
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("Invalid target agent context window selection: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentGetThreadExecutionProfile { thread_id } => {
            let profile = agent.get_thread_execution_profile(&thread_id).await;
            let profile_json =
                serde_json::to_string(&profile).unwrap_or_else(|_| "null".to_string());
            framed
                .send(DaemonMessage::AgentThreadExecutionProfile {
                    thread_id,
                    profile_json,
                })
                .await?;
        }

        ClientMessage::AgentSetThreadExecutionProfile {
            thread_id,
            profile_json,
        } => {
            let trimmed = profile_json.trim();
            let parsed: Option<crate::agent::types::ThreadExecutionProfile> = if trimmed.is_empty()
                || trimmed == "null"
            {
                None
            } else {
                match serde_json::from_str::<crate::agent::types::ThreadExecutionProfile>(trimmed) {
                    Ok(profile) => Some(profile),
                    Err(error) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!("Invalid thread execution profile: {error}"),
                            })
                            .await?;
                        return Ok(true);
                    }
                }
            };
            // Persist immediately, then wake any in-flight retry so the next LLM
            // attempt can rebuild against this profile instead of the snapshotted provider.
            if let Some(profile) = parsed.clone() {
                let mut map = agent.thread_execution_profiles.write().await;
                if profile.provider.is_none()
                    && profile.model.is_none()
                    && profile.reasoning_effort.is_none()
                    && profile.context_window_tokens.is_none()
                {
                    map.remove(&thread_id);
                } else {
                    map.insert(thread_id.clone(), profile);
                }
                drop(map);
                agent.notify_stream_retry_waiters(&thread_id).await;
            } else {
                agent
                    .thread_execution_profiles
                    .write()
                    .await
                    .remove(&thread_id);
            }
            agent.persist_thread_by_id(&thread_id).await;
            agent
                .persist_stored_thread_execution_profile(&thread_id)
                .await;
            let _ = agent
                .event_tx
                .send(crate::agent::types::AgentEvent::ThreadReloadRequired {
                    thread_id: thread_id.clone(),
                });
            let profile_json =
                serde_json::to_string(&parsed).unwrap_or_else(|_| "null".to_string());
            framed
                .send(DaemonMessage::AgentThreadExecutionProfile {
                    thread_id,
                    profile_json,
                })
                .await?;
        }

        ClientMessage::AgentFetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        } => {
            let _ = crate::agent::types::reload_custom_provider_catalog_from_default_path();
            let (resolved_url, resolved_key) = {
                let config = agent.config.read().await;
                let url = if base_url.is_empty() {
                    config
                        .providers
                        .get(&provider_id)
                        .map(|pc| pc.base_url.clone())
                        .filter(|value| !value.is_empty())
                        .or_else(|| {
                            crate::agent::types::custom_provider_config(&provider_id)
                                .map(|pc| pc.base_url)
                        })
                        .unwrap_or_default()
                } else {
                    base_url
                };
                let key = if api_key.is_empty() {
                    config
                        .providers
                        .get(&provider_id)
                        .map(|pc| pc.api_key.clone())
                        .filter(|value| !value.is_empty())
                        .or_else(|| {
                            crate::agent::types::custom_provider_config(&provider_id)
                                .map(|pc| pc.api_key)
                        })
                        .unwrap_or_default()
                } else {
                    api_key
                };
                (url, key)
            };
            if !background_daemon_pending.has_capacity(BackgroundSubsystem::ProviderIo) {
                background_daemon_pending.note_rejection(BackgroundSubsystem::ProviderIo);
                framed
                    .send(DaemonMessage::Error {
                        message: "provider_io background queue is full".to_string(),
                    })
                    .await?;
                return Ok(true);
            }

            let operation = operation_registry().accept_operation(
                OPERATION_KIND_FETCH_MODELS,
                Some(fetch_models_dedup_key(
                    &agent,
                    &provider_id,
                    output_modalities.as_deref(),
                )),
            );

            framed
                .send(DaemonMessage::OperationAccepted {
                    operation_id: operation.operation_id.clone(),
                    kind: operation.kind.clone(),
                    dedup: operation.dedup.clone(),
                    revision: operation.revision,
                })
                .await?;

            let operation_id = Some(operation.operation_id.clone());
            let result_operation_id = operation_id.clone();
            let background_daemon_tx =
                background_daemon_queues.sender(BackgroundSubsystem::ProviderIo);
            spawn_background_operation(
                BackgroundSubsystem::ProviderIo,
                operation_id,
                background_daemon_tx,
                background_daemon_pending,
                async move {
                    let result = crate::agent::llm_client::fetch_models(
                        &provider_id,
                        &resolved_url,
                        &resolved_key,
                        output_modalities.as_deref(),
                    )
                    .await;

                    let daemon_msg = match result {
                        Ok(models) => {
                            let json = serde_json::to_string(&models).unwrap_or_default();
                            DaemonMessage::AgentModelsResponse {
                                operation_id: result_operation_id.clone(),
                                models_json: json,
                            }
                        }
                        Err(e) => DaemonMessage::AgentError {
                            message: e.to_string(),
                        },
                    };

                    BackgroundOperationOutput::Completed(daemon_msg)
                },
            );
        }

        ClientMessage::AgentHeartbeatGetItems => {
            let items = agent.get_heartbeat_items().await;
            let json = serde_json::to_string(&items).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentHeartbeatItems { items_json: json })
                .await?;
        }

        ClientMessage::AgentHeartbeatSetItems { items_json } => {
            match serde_json::from_str(&items_json) {
                Ok(items) => agent.set_heartbeat_items(items).await,
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("Invalid heartbeat items: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentListTaskApprovalRules => {
            let rules = agent.list_task_approval_rules().await;
            tracing::debug!(
                rule_count = rules.len(),
                "sending task approval rules to client"
            );
            framed
                .send(DaemonMessage::AgentTaskApprovalRules { rules })
                .await?;
        }

        ClientMessage::AgentCreateTaskApprovalRule { approval_id } => {
            match agent
                .create_task_approval_rule_from_pending(&approval_id)
                .await
            {
                Ok(Some(_)) => {
                    let rules = agent.list_task_approval_rules().await;
                    tracing::debug!(
                        rule_count = rules.len(),
                        "sending task approval rules to client after create"
                    );
                    framed
                        .send(DaemonMessage::AgentTaskApprovalRules { rules })
                        .await?;
                }
                Ok(None) => {
                    framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "could not create approval rule: no live approval found for {approval_id}"
                                    ),
                                })
                                .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: format!(
                                "failed to create approval rule from {approval_id}: {error}"
                            ),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentRevokeTaskApprovalRule { rule_id } => {
            if !agent.revoke_task_approval_rule(&rule_id).await {
                framed
                    .send(DaemonMessage::AgentError {
                        message: format!("could not revoke approval rule: {rule_id} was not found"),
                    })
                    .await?;
            }
            let rules = agent.list_task_approval_rules().await;
            tracing::debug!(
                rule_count = rules.len(),
                "sending task approval rules to client after revoke"
            );
            framed
                .send(DaemonMessage::AgentTaskApprovalRules { rules })
                .await?;
        }

        ClientMessage::AgentResolveTaskApproval {
            approval_id,
            decision,
        } => {
            let decision = match decision.as_str() {
                "approve-session" => zorai_protocol::ApprovalDecision::ApproveSession,
                "deny" | "denied" => zorai_protocol::ApprovalDecision::Deny,
                _ => zorai_protocol::ApprovalDecision::ApproveOnce,
            };
            tracing::info!(%approval_id, ?decision, "resolving task approval");
            let _ = agent
                .record_operator_approval_resolution(&approval_id, decision)
                .await;
            agent
                .handle_task_approval_resolution(&approval_id, decision)
                .await;
            framed
                .send(DaemonMessage::ApprovalResolved {
                    id: uuid::Uuid::nil(),
                    approval_id,
                    decision,
                })
                .await?;
        }

        ClientMessage::AgentSubscribe => {
            *agent_event_rx = Some(agent.subscribe());
            tracing::info!("client subscribed to agent events");
            let subscribed_agent = agent.clone();
            tokio::spawn(async move {
                subscribed_agent
                    .mark_operator_present("client_subscribe")
                    .await;
                subscribed_agent.run_anticipatory_tick().await;
                subscribed_agent.emit_anticipatory_snapshot().await;
            });
        }

        ClientMessage::AgentUnsubscribe => {
            *agent_event_rx = None;
            tracing::info!("client unsubscribed from agent events");
        }

        ClientMessage::AgentDeclareAsyncCommandCapability { capability } => {
            framed
                .send(DaemonMessage::AgentAsyncCommandCapabilityAck { capability })
                .await?;
        }

        ClientMessage::AgentHandoffThread {
            thread_id,
            action,
            target_agent_id,
            reason,
            summary,
            requested_by,
            session_id: _,
            client_surface,
        } => {
            agent.mark_operator_present("thread_handoff").await;

            let authorization_error = if let Some(client_surface) = client_surface {
                agent
                    .get_thread_client_surface(&thread_id)
                    .await
                    .filter(|expected_surface| {
                        !client_surface_can_write_thread(*expected_surface, client_surface)
                    })
                    .map(|_| format!("unauthorized operator write for thread {thread_id}"))
            } else {
                None
            };

            let result = if let Some(error) = authorization_error {
                zorai_protocol::ThreadHandoffResult {
                    ok: false,
                    thread_id: thread_id.clone(),
                    active_agent_id: None,
                    stack_depth: None,
                    error: Some(error),
                }
            } else if requested_by.trim() != "user" {
                zorai_protocol::ThreadHandoffResult {
                    ok: false,
                    thread_id: thread_id.clone(),
                    active_agent_id: None,
                    stack_depth: None,
                    error: Some("direct thread handoff requires requested_by=user".to_string()),
                }
            } else {
                match agent
                    .apply_operator_thread_handoff(
                        &thread_id,
                        &action,
                        target_agent_id.as_deref(),
                        &reason,
                        &summary,
                    )
                    .await
                {
                    Ok(event) => {
                        let state = agent.thread_handoff_state(&thread_id).await;
                        zorai_protocol::ThreadHandoffResult {
                            ok: true,
                            thread_id: thread_id.clone(),
                            active_agent_id: Some(
                                state
                                    .as_ref()
                                    .map(|state| state.active_agent_id.clone())
                                    .unwrap_or(event.to_agent_id),
                            ),
                            stack_depth: state.map(|state| state.responder_stack.len()),
                            error: None,
                        }
                    }
                    Err(error) => zorai_protocol::ThreadHandoffResult {
                        ok: false,
                        thread_id: thread_id.clone(),
                        active_agent_id: None,
                        stack_depth: None,
                        error: Some(error.to_string()),
                    },
                }
            };

            framed
                .send(DaemonMessage::AgentThreadHandoffResult { result })
                .await?;
        }

        ClientMessage::AgentGetOperationStatus { operation_id } => {
            if let Some(status) = agent
                .session_manager
                .get_background_task_status(&operation_id)
                .await?
            {
                let snapshot = zorai_protocol::OperationStatusSnapshot {
                    operation_id: status.background_task_id,
                    kind: status.kind,
                    dedup: None,
                    state: match status.state {
                        crate::session_manager::BackgroundTaskState::Queued => {
                            zorai_protocol::OperationLifecycleState::Accepted
                        }
                        crate::session_manager::BackgroundTaskState::Running => {
                            zorai_protocol::OperationLifecycleState::Started
                        }
                        crate::session_manager::BackgroundTaskState::Completed => {
                            zorai_protocol::OperationLifecycleState::Completed
                        }
                        crate::session_manager::BackgroundTaskState::Failed => {
                            zorai_protocol::OperationLifecycleState::Failed
                        }
                    },
                    revision: 0,
                };
                framed
                    .send(DaemonMessage::OperationStatus { snapshot })
                    .await?;
            } else if let Some(snapshot) = operation_registry().snapshot(&operation_id) {
                framed
                    .send(DaemonMessage::OperationStatus { snapshot })
                    .await?;
            } else {
                framed
                    .send(DaemonMessage::AgentError {
                        message: format!("unknown operation id: {operation_id}"),
                    })
                    .await?;
            }
        }

        ClientMessage::AgentGetSubagentMetrics { task_id } => {
            let metrics_json = match agent.history.get_subagent_metrics(&task_id).await {
                Ok(Some(metrics)) => serde_json::to_string(&serde_json::json!({
                    "task_id": metrics.task_id,
                    "parent_task_id": metrics.parent_task_id,
                    "thread_id": metrics.thread_id,
                    "tool_calls_total": metrics.tool_calls_total,
                    "tool_calls_succeeded": metrics.tool_calls_succeeded,
                    "tool_calls_failed": metrics.tool_calls_failed,
                    "tokens_consumed": metrics.tokens_consumed,
                    "context_budget_tokens": metrics.context_budget_tokens,
                    "progress_rate": metrics.progress_rate,
                    "last_progress_at": metrics.last_progress_at,
                    "stuck_score": metrics.stuck_score,
                    "health_state": metrics.health_state,
                    "created_at": metrics.created_at,
                    "updated_at": metrics.updated_at,
                }))
                .unwrap_or_else(|_| "null".to_string()),
                Ok(None) => "null".to_string(),
                Err(e) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: format!("failed to fetch subagent metrics: {e}"),
                        })
                        .await
                        .ok();
                    return Ok(true);
                }
            };
            framed
                .send(DaemonMessage::AgentSubagentMetrics { metrics_json })
                .await
                .ok();
        }

        ClientMessage::AgentListCheckpoints { goal_run_id } => {
            let checkpoints_json = match agent
                .history
                .list_checkpoints_for_goal_run(&goal_run_id)
                .await
            {
                Ok(jsons) => {
                    let summaries = crate::agent::liveness::checkpoint::checkpoint_list(&jsons);
                    serde_json::to_string(&summaries).unwrap_or_else(|_| "[]".into())
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: format!("failed to list checkpoints: {e}"),
                        })
                        .await
                        .ok();
                    return Ok(true);
                }
            };
            framed
                .send(DaemonMessage::AgentCheckpointList {
                    goal_run_id,
                    checkpoints_json,
                })
                .await
                .ok();
        }

        ClientMessage::AgentRestoreCheckpoint { checkpoint_id } => {
            let outcome_json = match agent.restore_checkpoint(&checkpoint_id).await {
                Ok(outcome) => serde_json::to_string(&outcome).unwrap_or_else(|_| "null".into()),
                Err(e) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: format!("failed to restore checkpoint: {e}"),
                        })
                        .await
                        .ok();
                    return Ok(true);
                }
            };
            framed
                .send(DaemonMessage::AgentCheckpointRestored { outcome_json })
                .await
                .ok();
        }

        ClientMessage::AgentGetHealthStatus => {
            let status_json = agent.health_status_snapshot().await.to_string();
            framed
                .send(DaemonMessage::AgentHealthStatus { status_json })
                .await
                .ok();
        }

        ClientMessage::AgentListWorkspaceSettings => {
            match agent.history.list_workspace_settings().await {
                Ok(settings) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceSettingsList { settings })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentGetWorkspaceSettings { workspace_id } => {
            match agent.get_or_create_workspace_settings(&workspace_id).await {
                Ok(settings) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceSettings { settings })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentSetWorkspaceOperator {
            workspace_id,
            operator,
        } => match agent
            .set_workspace_operator_deferred_auto_start(&workspace_id, operator)
            .await
        {
            Ok(settings) => {
                framed
                    .send(DaemonMessage::AgentWorkspaceSettings { settings })
                    .await?;
            }
            Err(error) => {
                framed
                    .send(DaemonMessage::AgentWorkspaceError {
                        message: error.to_string(),
                    })
                    .await?;
            }
        },

        ClientMessage::AgentSetWorkspaceRepoMonitor {
            workspace_id,
            repo_monitor_enabled,
            repo_monitor_include_dirs,
            repo_monitor_exclude_dirs,
        } => match agent
            .set_workspace_repo_monitor(
                &workspace_id,
                repo_monitor_enabled,
                repo_monitor_include_dirs,
                repo_monitor_exclude_dirs,
            )
            .await
        {
            Ok(settings) => {
                framed
                    .send(DaemonMessage::AgentWorkspaceSettings { settings })
                    .await?;
            }
            Err(error) => {
                framed
                    .send(DaemonMessage::AgentWorkspaceError {
                        message: error.to_string(),
                    })
                    .await?;
            }
        },

        ClientMessage::AgentCreateWorkspaceTask { request } => {
            match agent
                .create_workspace_task_deferred_auto_start(
                    request,
                    zorai_protocol::WorkspaceActor::User,
                )
                .await
            {
                Ok(task) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskUpdated { task })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentListWorkspaceTasks {
            workspace_id,
            include_deleted,
        } => match agent
            .list_workspace_tasks(&workspace_id, include_deleted)
            .await
        {
            Ok(tasks) => {
                framed
                    .send(DaemonMessage::AgentWorkspaceTaskList {
                        workspace_id,
                        tasks,
                    })
                    .await?;
            }
            Err(error) => {
                framed
                    .send(DaemonMessage::AgentWorkspaceError {
                        message: error.to_string(),
                    })
                    .await?;
            }
        },

        ClientMessage::AgentGetWorkspaceTask { task_id } => {
            match agent.get_workspace_task(&task_id).await {
                Ok(task) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskDetail { task })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentUpdateWorkspaceTask { task_id, update } => {
            match agent
                .update_workspace_task_deferred_auto_start(&task_id, update)
                .await
            {
                Ok(Some(task)) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskUpdated { task })
                        .await?;
                }
                Ok(None) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskDetail { task: None })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentMoveWorkspaceTask { request } => {
            match agent.move_workspace_task_deferred_auto_start(request).await {
                Ok(Some(task)) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskUpdated { task })
                        .await?;
                }
                Ok(None) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskDetail { task: None })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentRunWorkspaceTask { task_id } => {
            match agent.run_workspace_task(&task_id).await {
                Ok(task) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskUpdated { task })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentPauseWorkspaceTask { task_id } => {
            match agent.pause_workspace_task(&task_id).await {
                Ok(Some(task)) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskUpdated { task })
                        .await?;
                }
                Ok(None) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskDetail { task: None })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentStopWorkspaceTask { task_id } => {
            match agent.stop_workspace_task(&task_id).await {
                Ok(Some(task)) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskUpdated { task })
                        .await?;
                }
                Ok(None) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskDetail { task: None })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentDeleteWorkspaceTask { task_id } => {
            match agent.delete_workspace_task(&task_id).await {
                Ok(Some(task)) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskDeleted {
                            task_id: task.id,
                            deleted_at: task.deleted_at,
                        })
                        .await?;
                }
                Ok(None) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskDetail { task: None })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentSubmitWorkspaceReview { review } => {
            match agent.submit_workspace_review(review).await {
                Ok(Some(task)) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskUpdated { task })
                        .await?;
                }
                Ok(None) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceTaskDetail { task: None })
                        .await?;
                }
                Err(error) => {
                    framed
                        .send(DaemonMessage::AgentWorkspaceError {
                            message: error.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentListWorkspaceNotices {
            workspace_id,
            task_id,
        } => match agent
            .list_workspace_notices(&workspace_id, task_id.as_deref())
            .await
        {
            Ok(notices) => {
                framed
                    .send(DaemonMessage::AgentWorkspaceNoticeList {
                        workspace_id,
                        notices,
                    })
                    .await?;
            }
            Err(error) => {
                framed
                    .send(DaemonMessage::AgentWorkspaceError {
                        message: error.to_string(),
                    })
                    .await?;
            }
        },

        _ => unreachable!("message chunk should be exhaustive"),
    }
    Ok(true)
}
