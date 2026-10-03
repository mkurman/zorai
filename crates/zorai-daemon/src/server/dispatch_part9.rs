use super::*;
use crate::agent::AgentEngine;
use anyhow::Result;
use std::sync::Arc;
use zorai_protocol::{ClientMessage, DaemonMessage};

pub(crate) async fn dispatch_part9(
    msg: &ClientMessage,
    agent: &Arc<AgentEngine>,
    framed: &mut ConnectionWriter,
    background_daemon_queues: &mut BackgroundSubsystemQueues,
    background_daemon_pending: &mut BackgroundPendingCounts,
    plugin_manager: &Arc<crate::plugin::PluginManager>,
) -> Result<bool> {
    if !matches!(
        msg,
        ClientMessage::SkillPublish { .. }
            | ClientMessage::AgentStatusQuery
            | ClientMessage::AgentStatisticsQuery { .. }
            | ClientMessage::AgentInspectPrompt { .. }
            | ClientMessage::AgentSetTierOverride { .. }
            | ClientMessage::PluginList { .. }
            | ClientMessage::PluginGet { .. }
            | ClientMessage::PluginEnable { .. }
            | ClientMessage::PluginDisable { .. }
            | ClientMessage::PluginInstall { .. }
            | ClientMessage::PluginUninstall { .. }
            | ClientMessage::PluginGetSettings { .. }
            | ClientMessage::PluginUpdateSettings { .. }
            | ClientMessage::PluginTestConnection { .. }
            | ClientMessage::PluginListCommands { .. }
            | ClientMessage::PluginOAuthStart { .. }
            | ClientMessage::PluginApiCall { .. }
            | ClientMessage::AgentExplainAction { .. }
            | ClientMessage::AgentStartDivergentSession { .. }
    ) {
        return Ok(false);
    }
    let msg = msg.clone();

    match msg {
        ClientMessage::SkillPublish { identifier } => {
            let variant = match agent.history.get_skill_variant(&identifier).await {
                Ok(Some(v)) => Some(v),
                _ => match agent
                    .history
                    .list_skill_variants(Some(&identifier), 1)
                    .await
                {
                    Ok(variants) => variants.into_iter().next(),
                    Err(_) => None,
                },
            };

            if let Some(v) = variant {
                if v.status != "proven" && v.status != "canonical" {
                    framed
                        .send(DaemonMessage::SkillPublishResult {
                            operation_id: None,
                            success: false,
                            message: format!(
                                "Only proven or canonical skills can be published; '{}' is {}.",
                                v.skill_name, v.status
                            ),
                        })
                        .await?;
                } else {
                    let config = agent.config.read().await;
                    let registry_url = config
                        .extra
                        .get("registry_url")
                        .and_then(|value| value.as_str())
                        .unwrap_or("https://registry.zorai.dev")
                        .to_string();
                    drop(config);

                    if !background_daemon_pending.has_capacity(BackgroundSubsystem::PluginIo) {
                        background_daemon_pending.note_rejection(BackgroundSubsystem::PluginIo);
                        framed
                            .send(DaemonMessage::Error {
                                message: "plugin_io background queue is full".to_string(),
                            })
                            .await?;
                        return Ok(true);
                    }

                    let operation = operation_registry().accept_operation(
                        OPERATION_KIND_SKILL_PUBLISH,
                        Some(skill_publish_dedup_key(&agent, &identifier)),
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
                    let skill_root = agent.history.data_dir().to_path_buf();
                    let registry_root = agent
                        .data_dir
                        .parent()
                        .unwrap_or(agent.data_dir.as_path())
                        .to_path_buf();
                    let machine_id = skill_root.to_string_lossy().to_string();
                    let background_daemon_tx =
                        background_daemon_queues.sender(BackgroundSubsystem::PluginIo);
                    spawn_background_operation(
                        BackgroundSubsystem::PluginIo,
                        operation_id,
                        background_daemon_tx,
                        background_daemon_pending,
                        async move {
                            let skill_dir = skill_root.join(
                                Path::new(&v.relative_path)
                                    .parent()
                                    .unwrap_or(Path::new(".")),
                            );

                            match prepare_publish(&skill_dir, &v, &machine_id) {
                                Ok((tarball, metadata)) => {
                                    let client = RegistryClient::new(registry_url, &registry_root);
                                    match client.publish_skill(&tarball, &metadata).await {
                                        Ok(()) => BackgroundOperationOutput::Completed(
                                            DaemonMessage::SkillPublishResult {
                                                operation_id: result_operation_id.clone(),
                                                success: true,
                                                message: format!(
                                                    "Published skill '{}'.",
                                                    v.skill_name
                                                ),
                                            },
                                        ),
                                        Err(e) => BackgroundOperationOutput::Failed(
                                            DaemonMessage::SkillPublishResult {
                                                operation_id: result_operation_id.clone(),
                                                success: false,
                                                message: format!(
                                                    "community skill publish failed: {e}"
                                                ),
                                            },
                                        ),
                                    }
                                }
                                Err(e) => BackgroundOperationOutput::Failed(
                                    DaemonMessage::SkillPublishResult {
                                        operation_id: result_operation_id.clone(),
                                        success: false,
                                        message: format!("failed to prepare skill publish: {e}"),
                                    },
                                ),
                            }
                        },
                    );
                }
            } else {
                framed
                    .send(DaemonMessage::SkillPublishResult {
                        operation_id: None,
                        success: false,
                        message: format!("Skill not found: {identifier}"),
                    })
                    .await?;
            }
        }

        ClientMessage::AgentStatusQuery => {
            let msg = agent.get_status_snapshot().await;
            framed.send(msg).await?;
        }

        ClientMessage::AgentStatisticsQuery {
            window,
            session_limit,
            session_offset,
            sessions_only,
        } => {
            let statistics = if sessions_only {
                agent
                    .history
                    .get_agent_session_page(window, session_limit, session_offset)
                    .await
            } else {
                agent
                    .history
                    .get_agent_statistics(window, session_limit, session_offset)
                    .await
            };
            match statistics {
                Ok(snapshot) => {
                    framed
                        .send(DaemonMessage::AgentStatisticsResponse {
                            statistics_json: serde_json::to_string(&snapshot)
                                .unwrap_or_else(|_| "{}".to_string()),
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

        ClientMessage::AgentInspectPrompt {
            agent_id,
            client_surface,
        } => {
            match agent
                .inspect_prompt_json(agent_id.as_deref(), client_surface)
                .await
            {
                Ok(prompt_json) => {
                    framed
                        .send(DaemonMessage::AgentPromptInspection { prompt_json })
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

        ClientMessage::AgentSetTierOverride { tier } => {
            use crate::agent::capability_tier::CapabilityTier;
            let parsed = tier.as_deref().and_then(CapabilityTier::from_str_loose);
            if tier.is_some() && parsed.is_none() {
                framed
                    .send(DaemonMessage::Error {
                        message: format!(
                            "Invalid tier '{}'. Expected: newcomer, familiar, power_user, expert.",
                            tier.unwrap_or_default()
                        ),
                    })
                    .await?;
            } else {
                agent.set_tier_override(parsed).await;
            }
        }

        ClientMessage::PluginList {} => {
            let plugins = plugin_manager.list_plugins().await;
            tracing::debug!(
                plugin_count = plugins.len(),
                "sending plugin list to client"
            );
            framed
                .send(DaemonMessage::PluginListResult { plugins })
                .await?;
        }
        ClientMessage::PluginGet { name } => match plugin_manager.get_plugin(&name).await {
            Some((info, settings_schema)) => {
                framed
                    .send(DaemonMessage::PluginGetResult {
                        plugin: Some(info),
                        settings_schema,
                    })
                    .await?;
            }
            None => {
                framed
                    .send(DaemonMessage::PluginGetResult {
                        plugin: None,
                        settings_schema: None,
                    })
                    .await?;
            }
        },
        ClientMessage::PluginEnable { name } => {
            let result = plugin_manager.set_enabled(&name, true).await;
            let (success, message) = match result {
                Ok(()) => (true, format!("Plugin '{}' enabled", name)),
                Err(e) => (false, format!("Failed to enable plugin '{}': {}", name, e)),
            };
            framed
                .send(DaemonMessage::PluginActionResult { success, message })
                .await?;
        }
        ClientMessage::PluginDisable { name } => {
            let result = plugin_manager.set_enabled(&name, false).await;
            let (success, message) = match result {
                Ok(()) => (true, format!("Plugin '{}' disabled", name)),
                Err(e) => (false, format!("Failed to disable plugin '{}': {}", name, e)),
            };
            framed
                .send(DaemonMessage::PluginActionResult { success, message })
                .await?;
        }
        ClientMessage::PluginInstall {
            dir_name,
            install_source,
        } => {
            let result = plugin_manager
                .register_plugin(&dir_name, &install_source)
                .await;
            let (success, message) = match result {
                Ok(info) => (
                    true,
                    format!(
                        "Plugin '{}' v{} registered successfully",
                        info.name, info.version
                    ),
                ),
                Err(e) => (false, format!("Failed to register plugin: {}", e)),
            };
            framed
                .send(DaemonMessage::PluginActionResult { success, message })
                .await?;
        }
        ClientMessage::PluginUninstall { name } => {
            let result = plugin_manager.unregister_plugin(&name).await;
            let (success, message) = match result {
                Ok(()) => (true, format!("Plugin '{}' unregistered", name)),
                Err(e) => (
                    false,
                    format!("Failed to unregister plugin '{}': {}", name, e),
                ),
            };
            framed
                .send(DaemonMessage::PluginActionResult { success, message })
                .await?;
        }

        ClientMessage::PluginGetSettings { name } => {
            let settings = plugin_manager.get_settings(&name).await;
            framed
                .send(DaemonMessage::PluginSettingsResult {
                    plugin_name: name,
                    settings,
                })
                .await?;
        }
        ClientMessage::PluginUpdateSettings {
            plugin_name,
            key,
            value,
            is_secret,
        } => {
            let result = plugin_manager
                .update_setting(&plugin_name, &key, &value, is_secret)
                .await;
            let (success, message) = match result {
                Ok(()) => (
                    true,
                    format!("Setting '{}' updated for plugin '{}'", key, plugin_name),
                ),
                Err(e) => (false, format!("Failed to update setting: {}", e)),
            };
            framed
                .send(DaemonMessage::PluginActionResult { success, message })
                .await?;
        }
        ClientMessage::PluginTestConnection { name } => {
            let (success, message) = plugin_manager.test_connection(&name).await;
            framed
                .send(DaemonMessage::PluginTestConnectionResult {
                    plugin_name: name,
                    success,
                    message,
                })
                .await?;
        }

        ClientMessage::PluginListCommands {} => {
            let commands = plugin_manager.list_commands().await;
            tracing::debug!(
                command_count = commands.len(),
                "sending plugin commands to client"
            );
            framed
                .send(DaemonMessage::PluginCommandsResult { commands })
                .await?;
        }

        ClientMessage::PluginOAuthStart { name } => {
            tracing::info!(plugin = %name, "OAuth2 flow start requested");
            if !background_daemon_pending.has_capacity(BackgroundSubsystem::PluginIo) {
                background_daemon_pending.note_rejection(BackgroundSubsystem::PluginIo);
                framed
                    .send(DaemonMessage::Error {
                        message: "plugin_io background queue is full".to_string(),
                    })
                    .await?;
                return Ok(true);
            }

            match plugin_manager.start_oauth_flow_for_plugin(&name).await {
                Ok(mut flow_state) => {
                    let operation = operation_registry().accept_operation(
                        OPERATION_KIND_PLUGIN_OAUTH_START,
                        Some(plugin_oauth_start_dedup_key(&agent, &name)),
                    );

                    framed
                        .send(DaemonMessage::OperationAccepted {
                            operation_id: operation.operation_id.clone(),
                            kind: operation.kind.clone(),
                            dedup: operation.dedup.clone(),
                            revision: operation.revision,
                        })
                        .await?;

                    let auth_url = flow_state.auth_url.clone();
                    framed
                        .send(DaemonMessage::PluginOAuthUrl {
                            name: name.clone(),
                            url: auth_url,
                        })
                        .await?;

                    let operation_id = Some(operation.operation_id.clone());
                    let result_operation_id = operation_id.clone();
                    let plugin_manager = plugin_manager.clone();
                    let background_daemon_tx =
                        background_daemon_queues.sender(BackgroundSubsystem::PluginIo);
                    spawn_background_operation(
                        BackgroundSubsystem::PluginIo,
                        operation_id,
                        background_daemon_tx,
                        background_daemon_pending,
                        async move {
                            match plugin_manager
                                .complete_oauth_flow(&name, &mut flow_state)
                                .await
                            {
                                Ok(()) => BackgroundOperationOutput::Completed(
                                    DaemonMessage::PluginOAuthComplete {
                                        operation_id: result_operation_id.clone(),
                                        name,
                                        success: true,
                                        error: None,
                                    },
                                ),
                                Err(e) => {
                                    tracing::warn!(plugin = %name, error = %e, "OAuth2 flow failed");
                                    BackgroundOperationOutput::Failed(
                                        DaemonMessage::PluginOAuthComplete {
                                            operation_id: result_operation_id.clone(),
                                            name,
                                            success: false,
                                            error: Some(e.to_string()),
                                        },
                                    )
                                }
                            }
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!(plugin = %name, error = %e, "OAuth2 flow start failed");
                    framed
                        .send(DaemonMessage::PluginOAuthComplete {
                            operation_id: None,
                            name,
                            success: false,
                            error: Some(e.to_string()),
                        })
                        .await?;
                }
            }
        }

        ClientMessage::PluginApiCall {
            plugin_name,
            endpoint_name,
            params,
        } => {
            if !background_daemon_pending.has_capacity(BackgroundSubsystem::PluginIo) {
                background_daemon_pending.note_rejection(BackgroundSubsystem::PluginIo);
                framed
                    .send(DaemonMessage::Error {
                        message: "plugin_io background queue is full".to_string(),
                    })
                    .await?;
                return Ok(true);
            }

            let params_json: serde_json::Value = serde_json::from_str(&params)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            let operation = operation_registry().accept_operation(
                OPERATION_KIND_PLUGIN_API_CALL,
                Some(plugin_api_call_dedup_key(
                    &agent,
                    &plugin_name,
                    &endpoint_name,
                    &params_json,
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
            let plugin_manager = plugin_manager.clone();
            let background_daemon_tx =
                background_daemon_queues.sender(BackgroundSubsystem::PluginIo);
            spawn_background_operation(
                BackgroundSubsystem::PluginIo,
                operation_id,
                background_daemon_tx,
                background_daemon_pending,
                async move {
                    #[cfg(test)]
                    if let Some(delay) = plugin_manager.test_api_call_delay().await {
                        tokio::time::sleep(delay).await;
                        return BackgroundOperationOutput::Failed(
                            DaemonMessage::PluginApiCallResult {
                                operation_id: result_operation_id.clone(),
                                plugin_name,
                                endpoint_name,
                                success: false,
                                result: crate::plugin::PluginApiError::Timeout.to_string(),
                                error_type: Some("timeout".to_string()),
                            },
                        );
                    }

                    match plugin_manager
                        .api_call(&plugin_name, &endpoint_name, params_json)
                        .await
                    {
                        Ok(result_text) => {
                            let (result, truncated) = cap_plugin_api_call_result_for_ipc(
                                result_operation_id.as_deref(),
                                &plugin_name,
                                &endpoint_name,
                                true,
                                result_text,
                                None,
                            );
                            if truncated {
                                tracing::warn!(
                                    plugin_name = %plugin_name,
                                    endpoint_name = %endpoint_name,
                                    "truncated plugin API result to fit IPC frame limit"
                                );
                            }
                            BackgroundOperationOutput::Completed(
                                DaemonMessage::PluginApiCallResult {
                                    operation_id: result_operation_id.clone(),
                                    plugin_name,
                                    endpoint_name,
                                    success: true,
                                    result,
                                    error_type: None,
                                },
                            )
                        }
                        Err(e) => {
                            let error_type = match &e {
                                crate::plugin::PluginApiError::SsrfBlocked { .. } => "ssrf_blocked",
                                crate::plugin::PluginApiError::RateLimited { .. } => "rate_limited",
                                crate::plugin::PluginApiError::Timeout => "timeout",
                                crate::plugin::PluginApiError::HttpError { .. } => "http_error",
                                crate::plugin::PluginApiError::TemplateError { .. } => {
                                    "template_error"
                                }
                                crate::plugin::PluginApiError::EndpointNotFound { .. } => {
                                    "endpoint_not_found"
                                }
                                crate::plugin::PluginApiError::PluginNotFound { .. } => {
                                    "plugin_not_found"
                                }
                                crate::plugin::PluginApiError::PluginDisabled { .. } => {
                                    "plugin_disabled"
                                }
                                crate::plugin::PluginApiError::AuthExpired { .. } => "auth_expired",
                            };
                            let (result, truncated) = cap_plugin_api_call_result_for_ipc(
                                result_operation_id.as_deref(),
                                &plugin_name,
                                &endpoint_name,
                                false,
                                e.to_string(),
                                Some(error_type),
                            );
                            if truncated {
                                tracing::warn!(
                                    plugin_name = %plugin_name,
                                    endpoint_name = %endpoint_name,
                                    "truncated plugin API error result to fit IPC frame limit"
                                );
                            }
                            BackgroundOperationOutput::Failed(DaemonMessage::PluginApiCallResult {
                                operation_id: result_operation_id.clone(),
                                plugin_name,
                                endpoint_name,
                                success: false,
                                result,
                                error_type: Some(error_type.to_string()),
                            })
                        }
                    }
                },
            );
        }
        ClientMessage::AgentExplainAction {
            action_id,
            step_index,
        } => {
            if !background_daemon_pending.has_capacity(BackgroundSubsystem::AgentWork) {
                background_daemon_pending.note_rejection(BackgroundSubsystem::AgentWork);
                framed
                    .send(DaemonMessage::Error {
                        message: "agent_work background queue is full".to_string(),
                    })
                    .await?;
                return Ok(true);
            }

            let operation = operation_registry().accept_operation(
                OPERATION_KIND_EXPLAIN_ACTION,
                Some(explain_action_dedup_key(&agent, &action_id, step_index)),
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
            let agent = agent.clone();
            let background_daemon_tx =
                background_daemon_queues.sender(BackgroundSubsystem::AgentWork);
            spawn_background_operation(
                BackgroundSubsystem::AgentWork,
                operation_id,
                background_daemon_tx,
                background_daemon_pending,
                async move {
                    let explanation = agent.handle_explain_action(&action_id, step_index).await;
                    let json = serde_json::to_string(&explanation).unwrap_or_default();
                    BackgroundOperationOutput::Completed(DaemonMessage::AgentExplanation {
                        operation_id: result_operation_id,
                        explanation_json: json,
                    })
                },
            );
        }
        ClientMessage::AgentStartDivergentSession {
            problem_statement,
            thread_id,
            goal_run_id,
            custom_framings_json,
        } => {
            if !background_daemon_pending.has_capacity(BackgroundSubsystem::AgentWork) {
                background_daemon_pending.note_rejection(BackgroundSubsystem::AgentWork);
                framed
                    .send(DaemonMessage::Error {
                        message: "agent_work background queue is full".to_string(),
                    })
                    .await?;
                return Ok(true);
            }

            let custom_framings = custom_framings_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<Vec<serde_json::Value>>(json).ok())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            let label = item.get("label")?.as_str()?.to_string();
                            let prompt = item.get("system_prompt_override")?.as_str()?.to_string();
                            Some(crate::agent::handoff::divergent::Framing {
                                label,
                                system_prompt_override: prompt,
                                task_id: None,
                                contribution_id: None,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .filter(|v| v.len() >= 2);

            let operation = operation_registry().accept_operation(
                OPERATION_KIND_START_DIVERGENT_SESSION,
                Some(start_divergent_session_dedup_key(
                    &agent,
                    &problem_statement,
                    &thread_id,
                    goal_run_id.as_deref(),
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
            let agent = agent.clone();
            let background_daemon_tx =
                background_daemon_queues.sender(BackgroundSubsystem::AgentWork);
            spawn_background_operation(
                BackgroundSubsystem::AgentWork,
                operation_id,
                background_daemon_tx,
                background_daemon_pending,
                async move {
                    match agent
                        .start_divergent_session(
                            &problem_statement,
                            custom_framings,
                            &thread_id,
                            goal_run_id.as_deref(),
                        )
                        .await
                    {
                        Ok(session_id) => {
                            let result = serde_json::json!({
                                "session_id": session_id,
                                "status": "started",
                            });
                            BackgroundOperationOutput::Completed(
                                DaemonMessage::AgentDivergentSessionStarted {
                                    operation_id: result_operation_id.clone(),
                                    session_json: serde_json::to_string(&result)
                                        .unwrap_or_default(),
                                },
                            )
                        }
                        Err(e) => BackgroundOperationOutput::Failed(DaemonMessage::Error {
                            message: format!("Failed to start divergent session: {e}"),
                        }),
                    }
                },
            );
        }
        _ => unreachable!("message chunk should be exhaustive"),
    }
    Ok(true)
}
