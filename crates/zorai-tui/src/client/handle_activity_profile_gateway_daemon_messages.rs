use super::*;
use crate::client::{ClientEvent, DaemonClient};
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, warn};
use zorai_protocol::DaemonMessage;

pub(crate) async fn dispatch_client_event(
    event_tx: &mpsc::Sender<ClientEvent>,
    event: ClientEvent,
    context: &'static str,
) {
    if let Err(err) = event_tx.send(event).await {
        warn!(target: "zorai_tui::client", context, error = %err, "dropped client event");
    }
}

impl DaemonClient {
    pub(crate) async fn handle_activity_profile_gateway_daemon_messages(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
    ) {
        match message {
            DaemonMessage::AgentThreadMessagePinResult { result_json } => {
                if let Ok(result) =
                    serde_json::from_str::<crate::client::ThreadMessagePinResultVm>(&result_json)
                {
                    dispatch_client_event(
                        event_tx,
                        ClientEvent::ThreadMessagePinResult(result),
                        "thread_message_pin_result",
                    )
                    .await;
                } else {
                    dispatch_client_event(
                        event_tx,
                        ClientEvent::Error(
                            "failed to parse thread pin mutation result".to_string(),
                        ),
                        "thread_message_pin_result_parse_error",
                    )
                    .await;
                }
            }
            DaemonMessage::AgentWhatsAppLinkStatus {
                state,
                phone,
                last_error,
            } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::WhatsAppLinkStatus {
                        state,
                        phone,
                        last_error,
                    },
                    "whatsapp_link_status",
                )
                .await;
            }
            DaemonMessage::AgentWhatsAppLinkQr {
                ascii_qr,
                expires_at_ms,
            } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::WhatsAppLinkQr {
                        ascii_qr,
                        expires_at_ms,
                    },
                    "whatsapp_link_qr",
                )
                .await;
            }
            DaemonMessage::AgentWhatsAppLinked { phone } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::WhatsAppLinked { phone },
                    "whatsapp_linked",
                )
                .await;
            }
            DaemonMessage::AgentWhatsAppLinkError {
                message,
                recoverable,
            } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::WhatsAppLinkError {
                        message,
                        recoverable,
                    },
                    "whatsapp_link_error",
                )
                .await;
            }
            DaemonMessage::AgentWhatsAppLinkDisconnected { reason } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::WhatsAppLinkDisconnected { reason },
                    "whatsapp_link_disconnected",
                )
                .await;
            }
            DaemonMessage::AgentExplanation {
                operation_id: _,
                explanation_json,
            } => {
                let payload = serde_json::from_str::<serde_json::Value>(&explanation_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                dispatch_client_event(
                    event_tx,
                    ClientEvent::AgentExplanation(payload),
                    "agent_explanation",
                )
                .await;
            }
            DaemonMessage::AgentDivergentSessionStarted {
                operation_id: _,
                session_json,
            } => {
                let payload = serde_json::from_str::<serde_json::Value>(&session_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                dispatch_client_event(
                    event_tx,
                    ClientEvent::DivergentSessionStarted(payload),
                    "divergent_session_started",
                )
                .await;
            }
            DaemonMessage::AgentDivergentSession { session_json } => {
                let payload = serde_json::from_str::<serde_json::Value>(&session_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                dispatch_client_event(
                    event_tx,
                    ClientEvent::DivergentSession(payload),
                    "divergent_session",
                )
                .await;
            }
            DaemonMessage::AgentStatusResponse {
                tier,
                activity,
                active_thread_id,
                active_goal_run_id,
                active_goal_run_title,
                provider_health_json,
                gateway_statuses_json,
                recent_actions_json,
                diagnostics_json,
                ..
            } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::StatusSnapshot(AgentStatusSnapshotVm {
                        tier,
                        activity,
                        active_thread_id,
                        active_goal_run_id,
                        active_goal_run_title,
                        provider_health_json,
                        gateway_statuses_json,
                        recent_actions_json,
                    }),
                    "status_snapshot",
                )
                .await;
                if let Ok(diagnostics) =
                    serde_json::from_str::<serde_json::Value>(&diagnostics_json)
                {
                    dispatch_client_event(
                        event_tx,
                        ClientEvent::StatusDiagnostics {
                            operator_profile_sync_state: diagnostics
                                .get("operator_profile_sync_state")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .to_string(),
                            operator_profile_sync_dirty: diagnostics
                                .get("operator_profile_sync_dirty")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            operator_profile_scheduler_fallback: diagnostics
                                .get("operator_profile_scheduler_fallback")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            diagnostics_json,
                        },
                        "status_diagnostics",
                    )
                    .await;
                }
            }
            DaemonMessage::AgentStatisticsResponse { statistics_json } => {
                if let Ok(snapshot) = serde_json::from_str::<zorai_protocol::AgentStatisticsSnapshot>(
                    &statistics_json,
                ) {
                    dispatch_client_event(
                        event_tx,
                        ClientEvent::StatisticsSnapshot(snapshot),
                        "statistics_snapshot",
                    )
                    .await;
                }
            }
            DaemonMessage::AgentPromptInspection { prompt_json } => {
                if let Ok(prompt) = serde_json::from_str::<AgentPromptInspectionVm>(&prompt_json) {
                    dispatch_client_event(
                        event_tx,
                        ClientEvent::PromptInspection(prompt),
                        "prompt_inspection",
                    )
                    .await;
                }
            }
            DaemonMessage::AgentOperatorProfileSessionStarted { session_id, kind } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::OperatorProfileSessionStarted { session_id, kind },
                    "operator_profile_session_started",
                )
                .await;
            }
            DaemonMessage::AgentOperatorProfileQuestion {
                session_id,
                question_id,
                field_key,
                prompt,
                input_kind,
                optional,
            } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::OperatorProfileQuestion {
                        session_id,
                        question_id,
                        field_key,
                        prompt,
                        input_kind,
                        optional,
                    },
                    "operator_profile_question",
                )
                .await;
            }
            DaemonMessage::AgentOperatorProfileProgress {
                session_id,
                answered,
                remaining,
                completion_ratio,
            } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::OperatorProfileProgress {
                        session_id,
                        answered,
                        remaining,
                        completion_ratio,
                    },
                    "operator_profile_progress",
                )
                .await;
            }
            DaemonMessage::AgentOperatorProfileSummary { summary_json } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::OperatorProfileSummary { summary_json },
                    "operator_profile_summary",
                )
                .await;
            }
            DaemonMessage::AgentOperatorModel { model_json } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::OperatorModelSummary { model_json },
                    "operator_model_summary",
                )
                .await;
            }
            DaemonMessage::AgentOperatorModelReset { ok } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::OperatorModelReset { ok },
                    "operator_model_reset",
                )
                .await;
            }
            DaemonMessage::AgentCollaborationSessions { sessions_json } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::CollaborationSessions { sessions_json },
                    "collaboration_sessions",
                )
                .await;
            }
            DaemonMessage::AgentCollaborationVoteResult { report_json } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::CollaborationVoteResult { report_json },
                    "collaboration_vote_result",
                )
                .await;
            }
            DaemonMessage::AgentGeneratedTools { tools_json } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::GeneratedTools { tools_json },
                    "generated_tools",
                )
                .await;
            }
            DaemonMessage::AgentSpeechToTextResult { content } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::SpeechToTextResult { content },
                    "speech_to_text_result",
                )
                .await;
            }
            DaemonMessage::AgentTextToSpeechResult { content } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::TextToSpeechResult { content },
                    "text_to_speech_result",
                )
                .await;
            }
            DaemonMessage::AgentGenerateImageResult { content } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::GenerateImageResult { content },
                    "generate_image_result",
                )
                .await;
            }
            DaemonMessage::AgentGenerateVideoResult { content } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::GenerateVideoResult { content },
                    "generate_video_result",
                )
                .await;
            }
            DaemonMessage::AgentOperatorProfileSessionCompleted {
                session_id,
                updated_fields,
            } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::OperatorProfileSessionCompleted {
                        session_id,
                        updated_fields,
                    },
                    "operator_profile_session_completed",
                )
                .await;
            }
            DaemonMessage::AgentTierChanged {
                previous_tier: _,
                new_tier,
                reason: _,
            } => {
                dispatch_client_event(
                    event_tx,
                    ClientEvent::TierChanged { new_tier },
                    "agent_tier_changed",
                )
                .await;
            }
            DaemonMessage::SemanticIndexRepairResult { result_json } => {
                match serde_json::from_str::<zorai_protocol::SemanticIndexRepairResultPublic>(
                    &result_json,
                ) {
                    Ok(result) => {
                        let mut summary = format!(
                            "Semantic index repair: removed_index={} cleared_completions={} cleared_deletions={} reset_failed_jobs={}",
                            result.removed_vector_index,
                            result.cleared_completions,
                            result.cleared_deletions,
                            result.reset_failed_jobs,
                        );
                        if let Some(backup) = result.backup_path.as_deref() {
                            summary.push_str(&format!(" backup={backup}"));
                        }
                        dispatch_client_event(
                            event_tx,
                            ClientEvent::SemanticIndexRepaired { summary },
                            "semantic_index_repaired",
                        )
                        .await;
                    }
                    Err(err) => {
                        dispatch_client_event(
                            event_tx,
                            ClientEvent::Error(format!(
                                "failed to parse semantic index repair result: {err}"
                            )),
                            "semantic_index_repair_parse_error",
                        )
                        .await;
                    }
                }
            }
            DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. } => {
                debug!("Ignoring gateway runtime daemon message in TUI client");
            }
            DaemonMessage::Error { message } => {
                dispatch_client_event(event_tx, ClientEvent::Error(message), "daemon_error").await;
            }
            DaemonMessage::AgentError { message } => {
                dispatch_client_event(event_tx, ClientEvent::Error(message), "daemon_agent_error")
                    .await;
            }
            _ => unreachable!(
                "activity/profile/gateway daemon message dispatch should be exhaustive"
            ),
        }
    }
}
