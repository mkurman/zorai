use super::*;
use crate::agent::AgentEngine;
use anyhow::Result;
use std::sync::Arc;
use zorai_protocol::{ClientMessage, DaemonMessage};

pub(crate) async fn dispatch_part7(
    msg: &ClientMessage,
    agent: &Arc<AgentEngine>,
    framed: &mut ConnectionWriter,
    background_daemon_queues: &mut BackgroundSubsystemQueues,
    background_daemon_pending: &mut BackgroundPendingCounts,
    last_concierge_welcome_fingerprint: &mut Option<String>,
    agent_event_rx: &mut Option<tokio::sync::broadcast::Receiver<crate::agent::types::AgentEvent>>,
) -> Result<bool> {
    if !matches!(
        msg,
        ClientMessage::AgentValidateProvider { .. }
            | ClientMessage::AgentSetSubAgent { .. }
            | ClientMessage::AgentRemoveSubAgent { .. }
            | ClientMessage::AgentListSubAgents
            | ClientMessage::AgentGetConciergeConfig
            | ClientMessage::AgentSetConciergeConfig { .. }
            | ClientMessage::AgentRequestConciergeWelcome
            | ClientMessage::AgentDismissConciergeWelcome
            | ClientMessage::AuditQuery { .. }
            | ClientMessage::AuditDismiss { .. }
            | ClientMessage::EscalationCancel { .. }
            | ClientMessage::SkillList { .. }
            | ClientMessage::AgentSemanticDocumentSync
            | ClientMessage::AgentRepairSemanticIndex { .. }
            | ClientMessage::SkillDiscover { .. }
            | ClientMessage::GuidelineDiscover { .. }
    ) {
        return Ok(false);
    }
    let msg = msg.clone();

    match msg {
        ClientMessage::AgentValidateProvider {
            provider_id,
            base_url,
            api_key,
            auth_source,
        } => {
            let _ = crate::agent::types::reload_custom_provider_catalog_from_default_path();
            let (resolved_url, resolved_key) = {
                let config = agent.config.read().await;
                let url = if base_url.is_empty() {
                    config
                        .providers
                        .get(&provider_id)
                        .map(|pc| pc.base_url.clone())
                        .filter(|u| !u.is_empty())
                        .or_else(|| {
                            if config.provider == provider_id {
                                Some(config.base_url.clone())
                            } else {
                                crate::agent::types::get_provider_definition(&provider_id)
                                    .map(|d| d.default_base_url.to_string())
                            }
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
                        .filter(|k| !k.is_empty())
                        .or_else(|| {
                            if config.provider == provider_id {
                                Some(config.api_key.clone())
                            } else {
                                crate::agent::types::custom_provider_config(&provider_id)
                                    .map(|pc| pc.api_key)
                            }
                        })
                        .unwrap_or_default()
                } else {
                    api_key
                };
                (url, key)
            };
            let auth_source = match auth_source.as_str() {
                "chatgpt_subscription" => crate::agent::types::AuthSource::ChatgptSubscription,
                "github_copilot" => crate::agent::types::AuthSource::GithubCopilot,
                _ => crate::agent::types::AuthSource::ApiKey,
            };
            tracing::info!(
                provider = %provider_id,
                url = %resolved_url,
                has_key = !resolved_key.is_empty(),
                "validating provider connection"
            );

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
                OPERATION_KIND_PROVIDER_VALIDATION,
                Some(provider_validation_dedup_key(&agent, &provider_id)),
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
            let provider_id_for_task = provider_id.clone();
            let background_daemon_tx =
                background_daemon_queues.sender(BackgroundSubsystem::ProviderIo);
            spawn_background_operation(
                BackgroundSubsystem::ProviderIo,
                operation_id,
                background_daemon_tx,
                background_daemon_pending,
                async move {
                    let (valid, error) =
                        match crate::agent::llm_client::validate_provider_connection(
                            &provider_id_for_task,
                            &resolved_url,
                            &resolved_key,
                            auth_source,
                        )
                        .await
                        {
                            Ok(_) => (true, None),
                            Err(e) => {
                                tracing::warn!(provider = %provider_id_for_task, error = %e, "provider validation failed");
                                (false, Some(e.to_string()))
                            }
                        };

                    BackgroundOperationOutput::Completed(DaemonMessage::AgentProviderValidation {
                        operation_id: result_operation_id,
                        provider_id: provider_id_for_task,
                        valid,
                        error,
                        models_json: None,
                    })
                },
            );
        }

        ClientMessage::AgentSetSubAgent { sub_agent_json } => {
            match serde_json::from_str::<crate::agent::types::SubAgentDefinition>(&sub_agent_json) {
                Ok(mut def) => {
                    crate::agent::canonicalize_weles_client_update(&mut def);
                    let requested_sub_agent_id = def.id.clone();
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

                    let operation = operation_registry().accept_operation(
                        OPERATION_KIND_SET_SUB_AGENT,
                        Some(set_sub_agent_dedup_key(
                            &agent,
                            &requested_sub_agent_id,
                            &sub_agent_json,
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
                    let sub_agent_json_for_task = sub_agent_json.clone();
                    let background_daemon_tx =
                        background_daemon_queues.sender(BackgroundSubsystem::ConfigReconcile);
                    spawn_background_operation(
                        BackgroundSubsystem::ConfigReconcile,
                        Some(operation.operation_id.clone()),
                        background_daemon_tx,
                        background_daemon_pending,
                        Box::pin(async move {
                            match agent.set_sub_agent(def).await {
                                Ok(()) => {
                                    let updated_sub_agent_json = agent
                                        .get_sub_agent(&requested_sub_agent_id)
                                        .await
                                        .and_then(|entry| serde_json::to_string(&entry).ok())
                                        .unwrap_or(sub_agent_json_for_task);
                                    BackgroundOperationOutput::Completed(
                                        DaemonMessage::AgentSubAgentUpdated {
                                            sub_agent_json: updated_sub_agent_json,
                                        },
                                    )
                                }
                                Err(e) => {
                                    BackgroundOperationOutput::Failed(DaemonMessage::AgentError {
                                        message: format!("Invalid sub-agent mutation: {e}"),
                                    })
                                }
                            }
                        }),
                    );
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: format!("Invalid sub-agent: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentRemoveSubAgent { sub_agent_id } => {
            if !background_daemon_pending.has_capacity(BackgroundSubsystem::ConfigReconcile) {
                background_daemon_pending.note_rejection(BackgroundSubsystem::ConfigReconcile);
                framed
                    .send(DaemonMessage::Error {
                        message: "config_reconcile background queue is full".to_string(),
                    })
                    .await?;
                return Ok(true);
            }

            let operation = operation_registry().accept_operation(
                OPERATION_KIND_REMOVE_SUB_AGENT,
                Some(remove_sub_agent_dedup_key(&agent, &sub_agent_id)),
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
            let sub_agent_id_for_task = sub_agent_id.clone();
            let background_daemon_tx =
                background_daemon_queues.sender(BackgroundSubsystem::ConfigReconcile);
            spawn_background_operation(
                BackgroundSubsystem::ConfigReconcile,
                Some(operation.operation_id.clone()),
                background_daemon_tx,
                background_daemon_pending,
                async move {
                    match agent.remove_sub_agent(&sub_agent_id_for_task).await {
                        Ok(_) => BackgroundOperationOutput::Completed(
                            DaemonMessage::AgentSubAgentRemoved {
                                sub_agent_id: sub_agent_id_for_task,
                            },
                        ),
                        Err(e) => BackgroundOperationOutput::Failed(DaemonMessage::AgentError {
                            message: format!("Invalid sub-agent mutation: {e}"),
                        }),
                    }
                },
            );
        }

        ClientMessage::AgentListSubAgents => {
            tracing::info!("dispatch: AgentListSubAgents received");
            let list = agent.list_sub_agents().await;
            let json = serde_json::to_string(&list).unwrap_or_default();
            let json_len = json.len();
            framed
                .send(DaemonMessage::AgentSubAgentList {
                    sub_agents_json: json,
                })
                .await?;
            tracing::info!(json_len, "dispatch: AgentSubAgentList sent");
        }

        ClientMessage::AgentGetConciergeConfig => {
            let concierge = agent.get_concierge_config().await;
            let json = serde_json::to_string(&concierge).unwrap_or_default();
            framed
                .send(DaemonMessage::AgentConciergeConfig { config_json: json })
                .await?;
        }

        ClientMessage::AgentSetConciergeConfig { config_json } => {
            match serde_json::from_str::<crate::agent::types::ConciergeConfig>(&config_json) {
                Ok(mut concierge_config) => {
                    let sends_prompt = serde_json::from_str::<serde_json::Value>(&config_json)
                        .ok()
                        .is_some_and(|value| value.get("system_prompt").is_some());
                    if !sends_prompt {
                        concierge_config.system_prompt =
                            agent.get_concierge_config().await.system_prompt;
                    }
                    agent.set_concierge_config(concierge_config).await;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::AgentError {
                            message: format!("Invalid concierge config: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentRequestConciergeWelcome => {
            tracing::info!("server: received AgentRequestConciergeWelcome");

            if !background_daemon_pending.has_capacity(BackgroundSubsystem::ConciergeWork) {
                background_daemon_pending.note_rejection(BackgroundSubsystem::ConciergeWork);
                framed
                    .send(DaemonMessage::Error {
                        message: "concierge_work background queue is full".to_string(),
                    })
                    .await?;
                return Ok(true);
            }

            let (operation, newly_accepted) = operation_registry()
                .accept_operation_with_dedup_status(
                    OPERATION_KIND_CONCIERGE_WELCOME,
                    Some(concierge_welcome_dedup_key(&agent)),
                );

            framed
                .send(DaemonMessage::OperationAccepted {
                    operation_id: operation.operation_id.clone(),
                    kind: operation.kind.clone(),
                    dedup: operation.dedup.clone(),
                    revision: operation.revision,
                })
                .await?;

            if agent_event_rx.is_none() {
                *agent_event_rx = Some(agent.subscribe());
            }

            if !newly_accepted {
                tracing::info!(
                    operation_id = %operation.operation_id,
                    "skipping duplicate concierge welcome side-effect (deduplicated)"
                );
                return Ok(true);
            }

            let agent = agent.clone();
            let operation_id = Some(operation.operation_id.clone());
            let background_daemon_tx =
                background_daemon_queues.sender(BackgroundSubsystem::ConciergeWork);
            spawn_background_side_effect(
                BackgroundSubsystem::ConciergeWork,
                operation_id,
                background_daemon_tx,
                background_daemon_pending,
                async move {
                    agent.request_concierge_welcome().await;
                    BackgroundSideEffectOutcome::Completed
                },
            );
        }

        ClientMessage::AgentDismissConciergeWelcome => {
            agent.concierge.prune_welcome_messages(&agent.threads).await;
            agent
                .persist_thread_by_id(crate::agent::concierge::CONCIERGE_THREAD_ID)
                .await;
            *last_concierge_welcome_fingerprint = None;
            framed
                .send(DaemonMessage::AgentConciergeWelcomeDismissed)
                .await?;
        }

        ClientMessage::AuditQuery {
            action_types,
            since,
            limit,
        } => {
            let action_types_ref = action_types.as_deref();
            let since_i64 = since.map(|s| s as i64);
            let limit = limit.unwrap_or(100);
            match agent
                .history
                .list_action_audit(action_types_ref, since_i64, limit)
                .await
            {
                Ok(rows) => {
                    let public_entries: Vec<zorai_protocol::AuditEntryPublic> = rows
                        .into_iter()
                        .map(|r| zorai_protocol::AuditEntryPublic {
                            id: r.id,
                            timestamp: r.timestamp,
                            action_type: r.action_type,
                            summary: r.summary,
                            explanation: r.explanation,
                            confidence: r.confidence,
                            confidence_band: r.confidence_band,
                            causal_trace_id: r.causal_trace_id,
                            thread_id: r.thread_id,
                            goal_run_id: r.goal_run_id,
                            task_id: r.task_id,
                        })
                        .collect();
                    let entries_json = serde_json::to_string(&public_entries).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AuditList { entries_json })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("audit query failed: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AuditDismiss { entry_id } => {
            tracing::info!(entry_id = %entry_id, "Audit dismiss requested");
            let result = agent.history.dismiss_audit_entry(&entry_id).await;
            let msg = match result {
                Ok(()) => DaemonMessage::AuditDismissResult {
                    success: true,
                    message: format!("Dismissed audit entry {}", entry_id),
                },
                Err(e) => DaemonMessage::AuditDismissResult {
                    success: false,
                    message: format!("Failed to dismiss: {}", e),
                },
            };
            framed.send(msg).await?;
        }

        ClientMessage::EscalationCancel { thread_id } => {
            tracing::info!(thread_id = %thread_id, "escalation cancel requested by user (D-13)");

            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;

            let audit_id = format!("audit-esc-cancel-{}", uuid::Uuid::new_v4());
            let summary = format!("User cancelled escalation for thread {thread_id}");

            let audit_entry = crate::history::AuditEntryRow {
                id: audit_id.clone(),
                timestamp: now_ms,
                action_type: "escalation".to_string(),
                summary: summary.clone(),
                explanation: Some(summary.clone()),
                confidence: None,
                confidence_band: None,
                causal_trace_id: None,
                thread_id: Some(thread_id.clone()),
                goal_run_id: None,
                task_id: None,
                raw_data_json: Some(
                    serde_json::json!({
                        "action": "cancel",
                        "thread_id": thread_id,
                        "outcome": "cancelled_by_user",
                    })
                    .to_string(),
                ),
            };

            if let Err(e) = agent.history.insert_action_audit(&audit_entry).await {
                tracing::warn!("failed to record escalation cancel audit: {e}");
            }

            let _ = agent
                .event_tx
                .send(crate::agent::types::AgentEvent::EscalationUpdate {
                    thread_id: thread_id.clone(),
                    from_level: "unknown".to_string(),
                    to_level: "L0".to_string(),
                    reason: "User took over (I'll handle this)".to_string(),
                    attempts: 0,
                    audit_id: Some(audit_id.clone()),
                });

            let _ = agent
                .event_tx
                .send(crate::agent::types::AgentEvent::AuditAction {
                    id: audit_id,
                    timestamp: now_ms as u64,
                    action_type: "escalation".to_string(),
                    summary: summary.clone(),
                    explanation: Some(summary.clone()),
                    confidence: None,
                    confidence_band: None,
                    causal_trace_id: None,
                    thread_id: Some(thread_id.clone()),
                });

            framed
                .send(DaemonMessage::EscalationCancelResult {
                    success: true,
                    message: format!(
                        "Escalation cancelled for thread {thread_id}. You now have control."
                    ),
                })
                .await?;
        }

        ClientMessage::SkillList {
            status,
            limit,
            cursor,
        } => {
            let limit = limit.clamp(1, 200);
            let result = if let Some(ref st) = status {
                agent
                    .history
                    .list_skill_variants_by_status_page(st, cursor.as_deref(), limit)
                    .await
            } else {
                agent
                    .history
                    .list_skill_variants_page(None, cursor.as_deref(), limit)
                    .await
            };
            match result {
                Ok(page) => {
                    let variants: Vec<zorai_protocol::SkillVariantPublic> = page
                        .variants
                        .into_iter()
                        .map(|r| zorai_protocol::SkillVariantPublic {
                            variant_id: r.variant_id,
                            skill_name: r.skill_name,
                            variant_name: r.variant_name,
                            relative_path: r.relative_path,
                            status: r.status,
                            use_count: r.use_count,
                            success_count: r.success_count,
                            failure_count: r.failure_count,
                            context_tags: r.context_tags,
                            created_at: r.created_at,
                            updated_at: r.updated_at,
                        })
                        .collect();
                    framed
                        .send(DaemonMessage::SkillListResult {
                            variants,
                            next_cursor: page.next_cursor,
                        })
                        .await?;
                }
                Err(e) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("skill list failed: {e}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::SkillDiscover {
            query,
            session_id,
            limit,
            cursor,
        } => {
            let limit = limit.clamp(1, 20);
            match agent
                .discover_skill_recommendations_public(&query, session_id, limit, cursor.as_deref())
                .await
            {
                Ok(result) => match serde_json::to_string(&result) {
                    Ok(result_json) => {
                        framed
                            .send(DaemonMessage::SkillDiscoverResult { result_json })
                            .await?;
                    }
                    Err(error) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!("skill discovery serialization failed: {error}"),
                            })
                            .await?;
                    }
                },
                Err(error) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("skill discovery failed: {error}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentSemanticDocumentSync => {
            match agent.sync_semantic_document_indexes_public().await {
                Ok(result) => match serde_json::to_string(&result) {
                    Ok(result_json) => {
                        framed
                            .send(DaemonMessage::SemanticDocumentSyncResult { result_json })
                            .await?;
                    }
                    Err(error) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "semantic document sync serialization failed: {error}"
                                ),
                            })
                            .await?;
                    }
                },
                Err(error) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("semantic document sync failed: {error}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::AgentRepairSemanticIndex { confirmed } => {
            match agent.repair_semantic_index_public(confirmed).await {
                Ok(result) => match serde_json::to_string(&result) {
                    Ok(result_json) => {
                        framed
                            .send(DaemonMessage::SemanticIndexRepairResult { result_json })
                            .await?;
                    }
                    Err(error) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "semantic index repair serialization failed: {error}"
                                ),
                            })
                            .await?;
                    }
                },
                Err(error) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("semantic index repair failed: {error}"),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::GuidelineDiscover {
            query,
            session_id,
            limit,
            cursor,
        } => {
            let limit = limit.clamp(1, 20);
            match agent
                .discover_guideline_recommendations_public(
                    &query,
                    session_id,
                    limit,
                    cursor.as_deref(),
                )
                .await
            {
                Ok(result) => match serde_json::to_string(&result) {
                    Ok(result_json) => {
                        framed
                            .send(DaemonMessage::GuidelineDiscoverResult { result_json })
                            .await?;
                    }
                    Err(error) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "guideline discovery serialization failed: {error}"
                                ),
                            })
                            .await?;
                    }
                },
                Err(error) => {
                    framed
                        .send(DaemonMessage::Error {
                            message: format!("guideline discovery failed: {error}"),
                        })
                        .await?;
                }
            }
        }

        _ => unreachable!("message chunk should be exhaustive"),
    }
    Ok(true)
}
