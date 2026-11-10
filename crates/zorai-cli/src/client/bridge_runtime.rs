use anyhow::{Context, Result};
use base64::Engine;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::codec::Framed;
use zorai_protocol::{
    ApprovalDecision, ClientMessage, DaemonMessage, ManagedCommandRequest, ManagedCommandSource,
    SecurityLevel, ZoraiCodec,
};

use super::bridge_protocol::{emit_bridge_event, BridgeCommand, BridgeEvent};
use super::connection::connect;

const BASE64_ENGINE: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

pub async fn run_bridge(
    session: Option<String>,
    shell: Option<String>,
    cwd: Option<String>,
    workspace: Option<String>,
    cols: u16,
    rows: u16,
    attach_only: bool,
) -> Result<()> {
    let mut framed = connect().await?;
    let session_id = if let Some(session) = session {
        let id = session.parse().context("invalid session ID")?;
        match attach_bridge_session(&mut framed, id).await {
            Ok(attached_id) => attached_id,
            Err(error) if attach_only && is_missing_session_error(&error) => {
                anyhow::bail!("terminal session {id} has ended")
            }
            Err(error) if is_missing_session_error(&error) => {
                tracing::warn!(requested_session = %id, error = %error, "saved session missing; spawning replacement session");
                spawn_bridge_session(
                    &mut framed,
                    shell.clone(),
                    cwd.clone(),
                    workspace.clone(),
                    cols,
                    rows,
                )
                .await?
            }
            Err(error) => return Err(error),
        }
    } else {
        spawn_bridge_session(
            &mut framed,
            shell.clone(),
            cwd.clone(),
            workspace.clone(),
            cols,
            rows,
        )
        .await?
    };

    emit_bridge_event(BridgeEvent::Ready {
        session_id: session_id.to_string(),
    })?;

    framed
        .send(ClientMessage::GetScrollback {
            id: session_id,
            max_lines: None,
        })
        .await
        .ok();

    framed
        .send(ClientMessage::Resize {
            id: session_id,
            cols,
            rows,
        })
        .await
        .ok();

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        tokio::select! {
            maybe_line = stdin_lines.next_line() => {
                match maybe_line? {
                    Some(line) => {
                        let command: BridgeCommand = match serde_json::from_str(&line) {
                            Ok(command) => command,
                            Err(error) => {
                                emit_bridge_event(BridgeEvent::Error {
                                    message: format!("invalid bridge command: {error}"),
                                })?;
                                continue;
                            }
                        };

                        match command {
                            BridgeCommand::Input { data } => {
                                let bytes = BASE64_ENGINE
                                    .decode(data)
                                    .context("invalid input payload")?;
                                framed
                                    .send(ClientMessage::Input { id: session_id, data: bytes })
                                    .await?;
                            }
                            BridgeCommand::Resize { cols, rows } => {
                                framed
                                    .send(ClientMessage::Resize { id: session_id, cols, rows })
                                    .await?;
                            }
                            BridgeCommand::ExecuteManaged {
                                command,
                                rationale,
                                allow_network,
                                sandbox_enabled,
                                security_level,
                                cwd,
                                language_hint,
                                source,
                            } => {
                                let source = match source.as_deref() {
                                    Some("human") => ManagedCommandSource::Human,
                                    Some("replay") => ManagedCommandSource::Replay,
                                    Some("gateway") => ManagedCommandSource::Gateway,
                                    _ => ManagedCommandSource::Agent,
                                };
                                let security_level = match security_level.as_deref() {
                                    Some("highest") => SecurityLevel::Highest,
                                    Some("lowest") => SecurityLevel::Lowest,
                                    Some("yolo") => SecurityLevel::Yolo,
                                    _ => SecurityLevel::Moderate,
                                };
                                framed
                                    .send(ClientMessage::ExecuteManagedCommand {
                                        id: session_id,
                                        request: ManagedCommandRequest {
                                            command,
                                            rationale,
                                            allow_network,
                                            sandbox_enabled: sandbox_enabled.unwrap_or(false),
                                            security_level,
                                            cwd,
                                            language_hint,
                                            source,
                                        },
                                        client_surface: Some(zorai_protocol::ClientSurface::Electron),
                                    })
                                    .await?;
                            }
                            BridgeCommand::ApprovalDecision { approval_id, decision } => {
                                let decision = match decision.as_str() {
                                    "approve-session" => ApprovalDecision::ApproveSession,
                                    "deny" => ApprovalDecision::Deny,
                                    _ => ApprovalDecision::ApproveOnce,
                                };
                                framed
                                    .send(ClientMessage::ResolveApproval {
                                        id: session_id,
                                        approval_id,
                                        decision,
                                    })
                                    .await?;
                            }
                            BridgeCommand::SearchHistory { query, limit } => {
                                framed
                                    .send(ClientMessage::SearchHistory { query, limit })
                                    .await?;
                            }
                            BridgeCommand::GenerateSkill { query, title } => {
                                framed
                                    .send(ClientMessage::GenerateSkill { query, title })
                                    .await?;
                            }
                            BridgeCommand::FindSymbol { workspace_root, symbol, limit } => {
                                framed
                                    .send(ClientMessage::FindSymbol {
                                        workspace_root,
                                        symbol,
                                        limit,
                                    })
                                    .await?;
                            }
                            BridgeCommand::ListSnapshots { workspace_id } => {
                                framed
                                    .send(ClientMessage::ListSnapshots { workspace_id })
                                    .await?;
                            }
                            BridgeCommand::RestoreSnapshot { snapshot_id } => {
                                framed
                                    .send(ClientMessage::RestoreSnapshot { snapshot_id })
                                    .await?;
                            }
                            BridgeCommand::Shutdown => {
                                break;
                            }
                            BridgeCommand::KillSession => {
                                framed
                                    .send(ClientMessage::KillSession { id: session_id })
                                    .await?;
                                break;
                            }
                        }
                    }
                    None => break,
                }
            }
            maybe_message = framed.next() => {
                match maybe_message {
                    Some(Ok(DaemonMessage::Output { id, data })) if id == session_id => {
                        if !data.is_empty() {
                            emit_bridge_event(BridgeEvent::Output {
                                session_id: id.to_string(),
                                data: BASE64_ENGINE.encode(data),
                            })?;
                        }
                    }
                    Some(Ok(DaemonMessage::CommandStarted { id, command })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::CommandStarted {
                            session_id: id.to_string(),
                            command_b64: BASE64_ENGINE.encode(command),
                        })?;
                    }
                    Some(Ok(DaemonMessage::CommandFinished { id, exit_code })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::CommandFinished {
                            session_id: id.to_string(),
                            exit_code,
                        })?;
                    }
                    Some(Ok(DaemonMessage::CwdChanged { id, cwd })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::CwdChanged {
                            session_id: id.to_string(),
                            cwd,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ManagedCommandQueued { id, execution_id, position, snapshot })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ManagedQueued {
                            session_id: id.to_string(),
                            execution_id,
                            position,
                            snapshot,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ApprovalRequired { id, approval })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ApprovalRequired {
                            session_id: id.to_string(),
                            approval,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ApprovalResolved { id, approval_id, decision })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ApprovalResolved {
                            session_id: id.to_string(),
                            approval_id,
                            decision,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ManagedCommandStarted { id, execution_id, command, source })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ManagedStarted {
                            session_id: id.to_string(),
                            execution_id,
                            command,
                            source,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ManagedCommandFinished { id, execution_id, command, exit_code, duration_ms, snapshot })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ManagedFinished {
                            session_id: id.to_string(),
                            execution_id,
                            command,
                            exit_code,
                            duration_ms,
                            snapshot,
                        })?;
                    }
                    Some(Ok(DaemonMessage::ManagedCommandRejected { id, execution_id, message })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::ManagedRejected {
                            session_id: id.to_string(),
                            execution_id,
                            message,
                        })?;
                    }
                    Some(Ok(DaemonMessage::HistorySearchResult { query, summary, hits })) => {
                        emit_bridge_event(BridgeEvent::HistorySearchResult { query, summary, hits })?;
                    }
                    Some(Ok(DaemonMessage::SkillGenerated { title, path })) => {
                        emit_bridge_event(BridgeEvent::SkillGenerated { title, path })?;
                    }
                    Some(Ok(DaemonMessage::SymbolSearchResult { symbol, matches })) => {
                        emit_bridge_event(BridgeEvent::SymbolSearchResult { symbol, matches })?;
                    }
                    Some(Ok(DaemonMessage::SnapshotList { snapshots })) => {
                        emit_bridge_event(BridgeEvent::SnapshotList { snapshots })?;
                    }
                    Some(Ok(DaemonMessage::SnapshotRestored { snapshot_id, ok, message })) => {
                        emit_bridge_event(BridgeEvent::SnapshotRestored { snapshot_id, ok, message })?;
                    }
                    Some(Ok(DaemonMessage::OscNotification { id, notification })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::OscNotification {
                            session_id: id.to_string(),
                            notification,
                        })?;
                    }
                    Some(Ok(DaemonMessage::Scrollback { id, data })) if id == session_id => {
                        if !data.is_empty() {
                            emit_bridge_event(BridgeEvent::Output {
                                session_id: id.to_string(),
                                data: BASE64_ENGINE.encode(data),
                            })?;
                        }
                    }
                    Some(Ok(DaemonMessage::SessionExited { id, exit_code })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::SessionExited {
                            session_id: id.to_string(),
                            exit_code,
                        })?;
                        break;
                    }
                    Some(Ok(DaemonMessage::SessionKilled { id })) if id == session_id => {
                        emit_bridge_event(BridgeEvent::SessionExited {
                            session_id: id.to_string(),
                            exit_code: None,
                        })?;
                        break;
                    }
                    Some(Ok(DaemonMessage::Error { message })) => {
                        emit_bridge_event(BridgeEvent::Error { message })?;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(error.into()),
                    None => {
                        emit_bridge_event(BridgeEvent::Error {
                            message: "daemon connection closed".to_string(),
                        })?;
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn attach_bridge_session(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    id: uuid::Uuid,
) -> Result<uuid::Uuid> {
    framed
        .send(ClientMessage::AttachSession { id })
        .await
        .context("failed to attach to daemon session")?;

    match framed.next().await {
        Some(Ok(DaemonMessage::SessionAttached { id })) => Ok(id),
        Some(Ok(DaemonMessage::Error { message })) => anyhow::bail!(message),
        Some(Ok(other)) => anyhow::bail!("unexpected response: {other:?}"),
        Some(Err(error)) => Err(error.into()),
        None => anyhow::bail!("daemon closed connection while attaching"),
    }
}

async fn spawn_bridge_session(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    shell: Option<String>,
    cwd: Option<String>,
    workspace: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<uuid::Uuid> {
    framed
        .send(ClientMessage::SpawnSession {
            shell,
            cwd,
            env: None,
            workspace_id: workspace,
            cols,
            rows,
        })
        .await
        .context("failed to request session spawn")?;

    match framed.next().await {
        Some(Ok(DaemonMessage::SessionSpawned { id })) => Ok(id),
        Some(Ok(DaemonMessage::Error { message })) => anyhow::bail!(message),
        Some(Ok(other)) => anyhow::bail!("unexpected response: {other:?}"),
        Some(Err(error)) => Err(error.into()),
        None => anyhow::bail!("daemon closed connection while spawning session"),
    }
}

fn is_missing_session_error(error: &anyhow::Error) -> bool {
    error
        .to_string()
        .to_ascii_lowercase()
        .contains("session not found")
}
