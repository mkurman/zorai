use super::*;
use crate::agent::AgentEngine;
use crate::session_manager::SessionManager;
use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;
use zorai_protocol::{ClientMessage, DaemonMessage};

pub(crate) async fn dispatch_part3(
    msg: &ClientMessage,
    agent: &Arc<AgentEngine>,
    framed: &mut ConnectionWriter,
    manager: &Arc<SessionManager>,
    client_agent_threads: &mut HashSet<String>,
) -> Result<bool> {
    if !matches!(
        msg,
        ClientMessage::ListAgentMessages { .. }
            | ClientMessage::UpsertTranscriptIndex { .. }
            | ClientMessage::ListTranscriptIndex { .. }
            | ClientMessage::UpsertSnapshotIndex { .. }
            | ClientMessage::ListSnapshotIndex { .. }
            | ClientMessage::UpsertAgentEvent { .. }
            | ClientMessage::ListAgentEvents { .. }
            | ClientMessage::MarkAllNotificationsRead
            | ClientMessage::ArchiveReadNotifications
            | ClientMessage::ListDatabaseTables
            | ClientMessage::QueryDatabaseRows { .. }
            | ClientMessage::UpdateDatabaseRows { .. }
            | ClientMessage::ExecuteDatabaseSql { .. }
            | ClientMessage::QueueSemanticBackfill { .. }
            | ClientMessage::GetSemanticIndexStatus { .. }
            | ClientMessage::GenerateSkill { .. }
            | ClientMessage::FindSymbol { .. }
            | ClientMessage::ListSnapshots { .. }
            | ClientMessage::RestoreSnapshot { .. }
            | ClientMessage::ListWorkspaceSessions { .. }
            | ClientMessage::GetGitStatus { .. }
            | ClientMessage::GetGitDiff { .. }
            | ClientMessage::GetFilePreview { .. }
            | ClientMessage::SubscribeNotifications
            | ClientMessage::ScrubSensitive { .. }
            | ClientMessage::CheckpointSession { .. }
            | ClientMessage::VerifyTelemetryIntegrity
            | ClientMessage::AgentSendMessage { .. }
            | ClientMessage::AgentDirectMessage { .. }
            | ClientMessage::AgentStopStream { .. }
            | ClientMessage::AgentForceCompact { .. }
            | ClientMessage::AgentInternalDelegate { .. }
            | ClientMessage::AgentThreadParticipantCommand { .. }
            | ClientMessage::AgentSendParticipantSuggestion { .. }
            | ClientMessage::AgentDismissParticipantSuggestion { .. }
    ) {
        return Ok(false);
    }
    let msg = msg.clone();

    match msg {
        ClientMessage::ListAgentMessages {
            thread_id,
            limit,
            include_deleted,
        } => {
            let effective_limit = limit.or(Some(AGENT_DB_THREAD_DETAIL_MESSAGE_WINDOW));
            let started = std::time::Instant::now();
            let (list_result, thread_result) = if include_deleted {
                tokio::join!(
                    manager.list_agent_messages_with_deleted(&thread_id, effective_limit),
                    manager.get_agent_thread(&thread_id),
                )
            } else {
                tokio::join!(
                    manager.list_agent_messages(&thread_id, effective_limit),
                    manager.get_agent_thread(&thread_id),
                )
            };
            match list_result {
                Ok(messages) => {
                    let thread = thread_result?;
                    let db_elapsed = started.elapsed().as_millis() as u64;
                    let message_count = messages.len();
                    let thread_id_for_log = thread_id.clone();
                    let cap_started = std::time::Instant::now();
                    let ((thread_json, messages_json), truncated) =
                        tokio::task::spawn_blocking(move || {
                            cap_agent_db_thread_detail_for_ipc(thread, messages)
                        })
                        .await
                        .unwrap_or_else(|_| ((String::new(), String::new()), true));
                    tracing::debug!(
                        thread_id = %thread_id_for_log,
                        message_count,
                        db_ms = db_elapsed,
                        cap_ms = cap_started.elapsed().as_millis() as u64,
                        truncated,
                        "ListAgentMessages served"
                    );
                    if truncated {
                        tracing::warn!(
                            thread_id = %thread_id_for_log,
                            "truncated listed agent db thread detail to fit IPC frame limit"
                        );
                    }
                    framed
                        .send(DaemonMessage::AgentDbThreadDetail {
                            thread_json,
                            messages_json,
                        })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::UpsertTranscriptIndex { entry_json } => {
            match serde_json::from_str::<zorai_protocol::TranscriptIndexEntry>(&entry_json) {
                Ok(entry) => match manager.upsert_transcript_index(&entry).await {
                    Ok(()) => {
                        framed
                            .send(DaemonMessage::AgentDbMessageAck { message_id: None })
                            .await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("invalid transcript index payload: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::ListTranscriptIndex { workspace_id } => {
            match manager
                .list_transcript_index_limited(
                    workspace_id.as_deref(),
                    Some(AGENT_DB_INDEX_LIST_WINDOW),
                )
                .await
            {
                Ok(entries) => {
                    let entries_json = serde_json::to_string(&entries).unwrap_or_default();
                    framed
                        .send(DaemonMessage::TranscriptIndexEntries { entries_json })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::UpsertSnapshotIndex { entry_json } => {
            match serde_json::from_str::<zorai_protocol::SnapshotIndexEntry>(&entry_json) {
                Ok(entry) => match manager.upsert_snapshot_index(&entry).await {
                    Ok(()) => {
                        framed
                            .send(DaemonMessage::AgentDbMessageAck { message_id: None })
                            .await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("invalid snapshot index payload: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::ListSnapshotIndex { workspace_id } => {
            match manager
                .list_snapshot_index_limited(
                    workspace_id.as_deref(),
                    Some(AGENT_DB_INDEX_LIST_WINDOW),
                )
                .await
            {
                Ok(entries) => {
                    let entries_json = serde_json::to_string(&entries).unwrap_or_default();
                    framed
                        .send(DaemonMessage::SnapshotIndexEntries { entries_json })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::UpsertAgentEvent { event_json } => {
            match serde_json::from_str::<zorai_protocol::AgentEventRow>(&event_json) {
                Ok(event) => match manager.upsert_agent_event(&event).await {
                    Ok(()) => {
                        if let Some(notification) =
                            crate::notifications::parse_notification_row(&event)
                        {
                            let _ = agent.event_sender().send(
                                crate::agent::types::AgentEvent::NotificationInboxUpsert {
                                    notification,
                                },
                            );
                        }
                        framed
                            .send(DaemonMessage::AgentDbMessageAck { message_id: None })
                            .await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("invalid agent event payload: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::ListAgentEvents {
            category,
            pane_id,
            limit,
        } => {
            let events_result = if category.as_deref()
                == Some(crate::notifications::NOTIFICATION_CATEGORY)
                && pane_id.is_none()
            {
                manager
                    .list_notifications(false, limit)
                    .await
                    .map(|notifications| {
                        notifications
                            .into_iter()
                            .filter_map(|notification| {
                                crate::notifications::notification_event_row(&notification).ok()
                            })
                            .collect::<Vec<_>>()
                    })
            } else {
                manager
                    .list_agent_events(category.as_deref(), pane_id.as_deref(), limit)
                    .await
            };
            match events_result {
                Ok(events) => {
                    let (events_json, truncated) = cap_agent_event_rows_for_ipc(events);
                    if truncated {
                        tracing::warn!(
                            category = category.as_deref().unwrap_or(""),
                            pane_id = pane_id.as_deref().unwrap_or(""),
                            "truncated agent event rows to fit IPC frame limit"
                        );
                    }
                    framed
                        .send(DaemonMessage::AgentEventRows { events_json })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::MarkAllNotificationsRead | ClientMessage::ArchiveReadNotifications => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or(0);
            let update_result = match msg {
                ClientMessage::MarkAllNotificationsRead => {
                    manager.mark_all_notifications_read(now).await
                }
                _ => manager.archive_read_notifications(now).await,
            };
            match update_result {
                Ok(updated) => {
                    tracing::info!(updated, "bulk notification update applied");
                    let events_result =
                        manager
                            .list_notifications(false, None)
                            .await
                            .map(|notifications| {
                                notifications
                                    .into_iter()
                                    .filter_map(|notification| {
                                        crate::notifications::notification_event_row(&notification)
                                            .ok()
                                    })
                                    .collect::<Vec<_>>()
                            });
                    match events_result {
                        Ok(events) => {
                            let (events_json, truncated) = cap_agent_event_rows_for_ipc(events);
                            if truncated {
                                tracing::warn!("truncated agent event rows to fit IPC frame limit");
                            }
                            framed
                                .send(DaemonMessage::AgentEventRows { events_json })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::ListDatabaseTables => match manager.list_database_tables().await {
            Ok(tables) => {
                let tables_json = serde_json::to_string(&tables).unwrap_or_default();
                framed
                    .send(DaemonMessage::DatabaseTables { tables_json })
                    .await?;
            }
            Err(e) => {
                framed
                    .send(DaemonMessage::Error {
                        message: e.to_string(),
                    })
                    .await?;
            }
        },

        ClientMessage::QueryDatabaseRows {
            table_name,
            offset,
            limit,
            sort_column,
            sort_direction,
        } => {
            match manager
                .query_database_table_rows(
                    &table_name,
                    offset,
                    limit,
                    sort_column.as_deref(),
                    sort_direction.as_deref(),
                )
                .await
            {
                Ok(page) => {
                    let rows_json = serde_json::to_string(&page).unwrap_or_default();
                    framed
                        .send(DaemonMessage::DatabaseRows { rows_json })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::UpdateDatabaseRows {
            table_name,
            updates_json,
        } => match serde_json::from_str::<Vec<crate::history::DatabaseRowUpdate>>(&updates_json) {
            Ok(updates) => match manager
                .update_database_table_rows(&table_name, updates)
                .await
            {
                Ok(updated_rows) => {
                    framed
                        .send(DaemonMessage::DatabaseUpdateAck { updated_rows })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            },
            Err(e) => {
                framed
                    .send(DaemonMessage::Error {
                        message: format!("invalid database update payload: {e}"),
                    })
                    .await?;
            }
        },

        ClientMessage::ExecuteDatabaseSql { sql } => {
            match manager.execute_database_sql(&sql).await {
                Ok(result) => {
                    let result_json = serde_json::to_string(&result).unwrap_or_default();
                    framed
                        .send(DaemonMessage::DatabaseSqlResult { result_json })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::QueueSemanticBackfill { limit } => {
            match manager.queue_semantic_backfill(limit).await {
                Ok(result) => {
                    let result_json = serde_json::to_string(&result).unwrap_or_default();
                    framed
                        .send(DaemonMessage::SemanticBackfillQueued { result_json })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::GetSemanticIndexStatus {
            embedding_model,
            dimensions,
        } => {
            match manager
                .semantic_index_status(&embedding_model, dimensions)
                .await
            {
                Ok(status) => {
                    let status_json = serde_json::to_string(&status).unwrap_or_default();
                    framed
                        .send(DaemonMessage::SemanticIndexStatus { status_json })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::GenerateSkill { query, title } => {
            match manager
                .generate_skill(query.as_deref(), title.as_deref())
                .await
            {
                Ok((title, path)) => {
                    framed
                        .send(DaemonMessage::SkillGenerated { title, path })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::FindSymbol {
            workspace_root,
            symbol,
            limit,
        } => {
            let matches =
                manager.find_symbol_matches(&workspace_root, &symbol, limit.unwrap_or(16).max(1));
            framed
                .send(DaemonMessage::SymbolSearchResult { symbol, matches })
                .await?;
        }

        ClientMessage::ListSnapshots { workspace_id } => {
            match manager
                .list_snapshots_limited(workspace_id.as_deref(), AGENT_DB_INDEX_LIST_WINDOW)
                .await
            {
                Ok(snapshots) => {
                    framed
                        .send(DaemonMessage::SnapshotList { snapshots })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::RestoreSnapshot { snapshot_id } => {
            match manager.restore_snapshot(&snapshot_id).await {
                Ok((ok, message)) => {
                    framed
                        .send(DaemonMessage::SnapshotRestored {
                            snapshot_id,
                            ok,
                            message,
                        })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: e.to_string(),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::ListWorkspaceSessions { workspace_id } => {
            let sessions = manager.list_workspace(&workspace_id).await;
            framed.send(DaemonMessage::SessionList { sessions }).await?;
        }

        ClientMessage::GetGitStatus { path } => {
            let info = crate::git::get_git_status(&path);
            framed.send(DaemonMessage::GitStatus { path, info }).await?;
        }

        ClientMessage::GetGitDiff {
            repo_path,
            file_path,
        } => {
            let diff = crate::git::git_diff(&repo_path, file_path.as_deref());
            let (diff, truncated) = cap_git_diff_for_ipc(&repo_path, file_path.as_deref(), diff);
            if truncated {
                tracing::warn!(
                    repo_path = %repo_path,
                    file_path = file_path.as_deref().unwrap_or(""),
                    "truncated git diff to fit IPC frame limit"
                );
            }
            framed
                .send(DaemonMessage::GitDiff {
                    repo_path,
                    file_path,
                    diff,
                })
                .await?;
        }

        ClientMessage::GetFilePreview { path, max_bytes } => {
            let (content, truncated, is_text) =
                crate::git::read_file_preview(&path, max_bytes.unwrap_or(65_536));
            framed
                .send(DaemonMessage::FilePreview {
                    path,
                    content,
                    truncated,
                    is_text,
                })
                .await?;
        }

        ClientMessage::SubscribeNotifications => {}

        ClientMessage::ScrubSensitive { text } => {
            let scrubbed = crate::scrub::scrub_sensitive(&text);
            framed
                .send(DaemonMessage::ScrubResult { text: scrubbed })
                .await?;
        }

        ClientMessage::CheckpointSession { id } => {
            let dump_dir = crate::criu::dump_dir_for_session(&id.to_string())
                .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/zorai-criu"));

            if !crate::criu::is_available() {
                framed
                    .send(DaemonMessage::SessionCheckpointed {
                        id,
                        ok: false,
                        path: None,
                        message: "CRIU is not available on this system".to_string(),
                    })
                    .await?;
            } else {
                framed
                    .send(DaemonMessage::SessionCheckpointed {
                        id,
                        ok: false,
                        path: Some(dump_dir.to_string_lossy().into_owned()),
                        message: "CRIU checkpoint: session PID tracking not yet integrated"
                            .to_string(),
                    })
                    .await?;
            }
        }

        ClientMessage::VerifyTelemetryIntegrity => match manager.verify_telemetry_integrity() {
            Ok(results) => {
                framed
                    .send(DaemonMessage::TelemetryIntegrityResult { results })
                    .await?;
            }
            Err(e) => {
                framed
                    .send(DaemonMessage::Error {
                        message: e.to_string(),
                    })
                    .await?;
            }
        },

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
            agent.mark_operator_present("send_message").await;
            let effective_thread_id =
                thread_id.or_else(|| Some(format!("thread_{}", uuid::Uuid::new_v4())));
            if let (Some(thread_id), Some(json)) =
                (effective_thread_id.as_ref(), workspace_context_json.as_deref())
            {
                match serde_json::from_str::<crate::agent::types::ThreadWorkspaceContext>(json) {
                    Ok(context) => {
                        if !agent
                            .set_thread_workspace_context(thread_id, Some(context.clone()))
                            .await
                        {
                            agent
                                .pending_thread_workspace_contexts
                                .write()
                                .await
                                .insert(thread_id.clone(), context);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "invalid workspace_context_json on AgentSendMessage")
                    }
                }
            }
            if let (Some(thread_id), Some(client_surface)) =
                (effective_thread_id.as_deref(), client_surface)
            {
                if let Some(expected_surface) = agent.get_thread_client_surface(thread_id).await {
                    if !client_surface_can_write_thread(expected_surface, client_surface) {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "unauthorized operator write for thread {thread_id}"
                                ),
                            })
                            .await?;
                        return Ok(true);
                    }
                }
            }
            if let Some(thread_id) = effective_thread_id.as_ref() {
                client_agent_threads.insert(thread_id.clone());
            }
            let agent = agent.clone();
            tokio::spawn(async move {
                let has_context = context_messages_json.is_some();
                tracing::info!(
                    thread_id = ?effective_thread_id,
                    content_len = content.len(),
                    has_context_json = has_context,
                    "AgentSendMessage received"
                );
                if let Some(ref json) = context_messages_json {
                    match serde_json::from_str::<Vec<zorai_protocol::AgentDbMessage>>(json) {
                        Ok(ctx) if !ctx.is_empty() => {
                            tracing::info!(
                                count = ctx.len(),
                                "seeding thread with context messages"
                            );
                            agent
                                .seed_thread_context(effective_thread_id.as_deref(), &ctx)
                                .await;
                        }
                        Ok(_) => tracing::info!("context_messages_json was empty array"),
                        Err(e) => {
                            tracing::warn!(error = %e, json_len = json.len(), "failed to parse context_messages_json")
                        }
                    }
                }
                if let Err(e) = Box::pin(agent.send_message_with_session_surface_and_target(
                    effective_thread_id.as_deref(),
                    session_id.as_deref(),
                    &content,
                    content_blocks_json.as_deref(),
                    client_surface,
                    target_agent_id.as_deref(),
                ))
                .await
                {
                    tracing::warn!(error = %e, "agent send_message_with_session failed");
                }
            });
        }

        ClientMessage::AgentDirectMessage {
            target,
            thread_id,
            content,
            session_id,
        } => {
            agent.mark_operator_present("direct_message").await;
            match Box::pin(agent.send_direct_message(
                &target,
                thread_id.as_deref(),
                session_id.as_deref(),
                &content,
            ))
            .await
            {
                Ok(result) => {
                    let thread_id = result.thread_id;
                    let response = result.response;
                    let provider_final_result_json = result
                        .provider_final_result
                        .as_ref()
                        .and_then(|value| serde_json::to_string(value).ok());
                    client_agent_threads.insert(thread_id.clone());
                    framed
                        .send(DaemonMessage::AgentDirectMessageResponse {
                            target,
                            thread_id,
                            response,
                            session_id,
                            provider_final_result_json,
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

        ClientMessage::AgentStopStream { thread_id } => {
            client_agent_threads.insert(thread_id.clone());
            let ok = agent.stop_stream(&thread_id).await;
            framed
                .send(DaemonMessage::AgentThreadControlled {
                    thread_id,
                    action: "stop".to_string(),
                    ok,
                })
                .await?;
        }
        ClientMessage::AgentForceCompact { thread_id } => {
            client_agent_threads.insert(thread_id.clone());
            let agent = agent.clone();
            tokio::spawn(async move {
                if let Err(error) = agent.force_compact_and_continue(&thread_id).await {
                    let _ = agent
                        .event_tx
                        .send(crate::agent::types::AgentEvent::WorkflowNotice {
                            thread_id: thread_id.clone(),
                            kind: "manual-compaction".to_string(),
                            message: format!("Manual compaction failed: {error}"),
                            details: None,
                        });
                    tracing::warn!(thread_id = %thread_id, error = %error, "agent force compact failed");
                }
            });
        }
        ClientMessage::AgentInternalDelegate {
            thread_id,
            target_agent_id,
            content,
            session_id,
            ..
        } => {
            agent.mark_operator_present("internal_delegate").await;
            let agent = agent.clone();
            tokio::spawn(async move {
                if let Err(error) = Box::pin(agent.send_internal_delegate_message(
                    thread_id.as_deref(),
                    &target_agent_id,
                    session_id.as_deref(),
                    &content,
                ))
                .await
                {
                    tracing::warn!(error = %error, "agent internal delegation failed");
                }
            });
        }
        ClientMessage::AgentThreadParticipantCommand {
            thread_id,
            target_agent_id,
            action,
            instruction,
            client_surface,
            ..
        } => {
            if let Some(client_surface) = client_surface {
                if let Some(expected_surface) = agent.get_thread_client_surface(&thread_id).await {
                    if !client_surface_can_write_thread(expected_surface, client_surface) {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "unauthorized operator write for thread {thread_id}"
                                ),
                            })
                            .await?;
                        return Ok(true);
                    }
                }
            }
            agent
                .mark_operator_present("thread_participant_command")
                .await;
            let agent = agent.clone();
            tokio::spawn(async move {
                if let Err(error) = agent
                    .apply_thread_participant_command(
                        &thread_id,
                        &target_agent_id,
                        &action,
                        instruction.as_deref(),
                    )
                    .await
                {
                    tracing::warn!(error = %error, "thread participant command failed");
                }
            });
        }
        ClientMessage::AgentSendParticipantSuggestion {
            thread_id,
            suggestion_id,
            client_surface,
            force_send,
            ..
        } => {
            if let Some(client_surface) = client_surface {
                if let Some(expected_surface) = agent.get_thread_client_surface(&thread_id).await {
                    if !client_surface_can_write_thread(expected_surface, client_surface) {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "unauthorized operator write for thread {thread_id}"
                                ),
                            })
                            .await?;
                        return Ok(true);
                    }
                }
            }
            agent
                .mark_operator_present("send_participant_suggestion")
                .await;
            let agent = agent.clone();
            tokio::spawn(async move {
                if force_send {
                    agent.stop_stream(&thread_id).await;
                }
                if let Err(error) = agent
                    .send_thread_participant_suggestion(&thread_id, &suggestion_id, None)
                    .await
                {
                    tracing::warn!(
                        error = %error,
                        "thread participant suggestion send failed"
                    );
                }
            });
        }
        ClientMessage::AgentDismissParticipantSuggestion {
            thread_id,
            suggestion_id,
            client_surface,
            ..
        } => {
            if let Some(client_surface) = client_surface {
                if let Some(expected_surface) = agent.get_thread_client_surface(&thread_id).await {
                    if !client_surface_can_write_thread(expected_surface, client_surface) {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "unauthorized operator write for thread {thread_id}"
                                ),
                            })
                            .await?;
                        return Ok(true);
                    }
                }
            }
            agent
                .mark_operator_present("dismiss_participant_suggestion")
                .await;
            let agent = agent.clone();
            tokio::spawn(async move {
                if let Err(error) = agent
                    .dismiss_thread_participant_suggestion(&thread_id, &suggestion_id)
                    .await
                {
                    tracing::warn!(
                        error = %error,
                        "thread participant suggestion dismissal failed"
                    );
                }
            });
        }
        ClientMessage::AgentRetryStreamNow { thread_id } => {
            client_agent_threads.insert(thread_id.clone());
            let ok = agent.retry_stream_now(&thread_id).await;
            framed
                .send(DaemonMessage::AgentThreadControlled {
                    thread_id,
                    action: "resume".to_string(),
                    ok,
                })
                .await?;
        }

        _ => unreachable!("message chunk should be exhaustive"),
    }
    Ok(true)
}
