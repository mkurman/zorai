use super::*;
use crate::agent::llm_client::CopilotInitiator;
use crate::agent::provider_resolution::apply_provider_model_override;
use crate::agent::task_worktree::{select_isolated_task_session, IsolatedTaskSessionPlan};
use std::path::Path;
use zorai_protocol::SecurityLevel;

const COMMUNITY_SCOUT_RESULT_LIMIT: usize = 5;
const PARTICIPANT_AGENT_FANOUT_TOOLS: &[&str] = &[
    zorai_protocol::tool_names::SPAWN_SUBAGENT,
    zorai_protocol::tool_names::MESSAGE_AGENT,
    zorai_protocol::tool_names::ROUTE_TO_SPECIALIST,
    zorai_protocol::tool_names::RUN_DIVERGENT,
];

#[derive(Clone)]
struct DirectThreadResponderConfig {
    agent_name: String,
    provider_id: String,
    model: Option<String>,
    base_url: Option<String>,
    reasoning_effort: Option<String>,
    context_window_tokens: Option<u32>,
    huggingface_provider: Option<String>,
    openrouter_provider_order: Vec<String>,
    openrouter_provider_ignore: Vec<String>,
    openrouter_allow_fallbacks: Option<bool>,
    system_prompt: String,
    persona_prompt: String,
    tool_filter: Option<crate::agent::subagent::tool_filter::ToolFilter>,
}

fn apply_openrouter_routing_override(
    provider_id: &str,
    provider_config: &mut ProviderConfig,
    order: &[String],
    ignore: &[String],
    allow_fallbacks: Option<bool>,
) {
    if provider_id != zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        return;
    }
    if !order.is_empty() {
        provider_config.openrouter_provider_order = order.to_vec();
    }
    if !ignore.is_empty() {
        provider_config.openrouter_provider_ignore = ignore.to_vec();
    }
    if allow_fallbacks.is_some() {
        provider_config.openrouter_allow_fallbacks = allow_fallbacks;
    }
}

fn is_workspace_agent_task(task: &AgentTask) -> bool {
    task.source.starts_with("workspace_")
        || task
            .thread_id
            .as_deref()
            .is_some_and(|thread_id| thread_id.starts_with("workspace-thread:"))
}

pub(super) fn spawned_subagent_turn(task: Option<&AgentTask>, identity_is_spawned: bool) -> bool {
    task.is_some_and(AgentTask::is_spawned_subagent) || identity_is_spawned
}

fn allow_workspace_task_tools(filter: &mut crate::agent::subagent::tool_filter::ToolFilter) {
    filter.allow_tools(
        crate::agent::tool_executor::workspace_task_tool_names()
            .iter()
            .copied(),
    );
}

fn allow_subagent_report_back_tools(filter: &mut crate::agent::subagent::tool_filter::ToolFilter) {
    filter.allow_tools([
        zorai_protocol::tool_names::REPORT_SUBAGENT_OUTCOME,
        zorai_protocol::tool_names::EXTEND_SUBAGENT_BUDGET,
    ]);
}

async fn load_current_task_for_send_message(
    engine: &AgentEngine,
    task_id: Option<&str>,
) -> Option<AgentTask> {
    let task_id = task_id?;
    {
        let tasks = engine.tasks.lock().await;
        if let Some(task) = tasks.iter().find(|task| task.id == task_id).cloned() {
            return Some(task);
        }
    }
    match engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(task_id.to_string()),
            status: None,
            statuses: Vec::new(),
            source: None,
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: Some(1),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await
        .into_iter()
        .next()
    {
        Some(task) => Some(task),
        None => None,
    }
}

type TaskProviderOverride = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<crate::agent::types::ApiTransport>,
    Option<u32>,
    Option<String>,
    Option<String>,
);

fn apply_thread_profile_to_task_provider_override(
    task_provider_override: Option<TaskProviderOverride>,
    profile: Option<&ThreadExecutionProfile>,
    has_task: bool,
) -> Option<TaskProviderOverride> {
    let nonempty = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    };
    let profile_provider = nonempty(profile.and_then(|profile| profile.provider.as_deref()));
    let profile_model = nonempty(profile.and_then(|profile| profile.model.as_deref()));
    if profile_provider.is_none() && profile_model.is_none() {
        return task_provider_override;
    }
    match task_provider_override {
        Some((
            provider,
            model,
            system_prompt,
            def_id,
            transport,
            context_window,
            huggingface,
            base_url,
        )) => {
            let provider_changed = profile_provider
                .as_ref()
                .is_some_and(|value| value != &provider);
            Some((
                profile_provider.unwrap_or(provider),
                profile_model.or(model),
                system_prompt,
                def_id,
                if provider_changed { None } else { transport },
                context_window,
                if provider_changed { None } else { huggingface },
                if provider_changed { None } else { base_url },
            ))
        }
        None if has_task => profile_provider
            .map(|provider| (provider, profile_model, None, None, None, None, None, None)),
        other => other,
    }
}

fn thread_artifact_prompt_block(data_root: &Path, thread_id: &str) -> String {
    let specs_dir = zorai_protocol::thread_specs_dir(data_root, thread_id);
    let media_dir = zorai_protocol::thread_media_dir(data_root, thread_id);
    let previews_dir = zorai_protocol::thread_previews_dir(data_root, thread_id);
    format!(
        "## Thread Artifact Directories\n\
         - specs dir: {}/\n\
         - media dir: {}/\n\
         - previews dir: {}/\n\
         - Place durable thread-scoped working specs and notes in the specs dir.\n\
         - Re-read relevant specs from that specs dir after handoff, pause, or restart.",
        specs_dir.display(),
        media_dir.display(),
        previews_dir.display(),
    )
}

async fn ensure_thread_artifact_dirs(data_root: &Path, thread_id: &str) -> Result<()> {
    for dir in [
        zorai_protocol::thread_specs_dir(data_root, thread_id),
        zorai_protocol::thread_media_dir(data_root, thread_id),
        zorai_protocol::thread_previews_dir(data_root, thread_id),
    ] {
        tokio::fs::create_dir_all(&dir).await.map_err(|error| {
            anyhow::anyhow!("create thread artifact dir {}: {error}", dir.display())
        })?;
    }
    Ok(())
}

fn build_direct_thread_responder_config(
    config: &AgentConfig,
    agent_scope_id: &str,
    sub_agents: &[SubAgentDefinition],
    execution_profile: Option<&ThreadExecutionProfile>,
) -> Result<Option<DirectThreadResponderConfig>> {
    let nonempty = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    };
    let resolved_target =
        crate::agent::agent_identity::resolve_agent_target(agent_scope_id, sub_agents);
    let resolved_scope = resolved_target.scope_id.as_str();
    if resolved_scope == MAIN_AGENT_ID {
        let profile_provider =
            nonempty(execution_profile.and_then(|profile| profile.provider.as_deref()));
        let profile_model =
            nonempty(execution_profile.and_then(|profile| profile.model.as_deref()));
        let profile_reasoning_effort =
            nonempty(execution_profile.and_then(|profile| profile.reasoning_effort.as_deref()));
        let profile_context_window_tokens =
            execution_profile.and_then(|profile| profile.context_window_tokens);
        if profile_provider.is_none()
            && profile_model.is_none()
            && profile_reasoning_effort.is_none()
            && profile_context_window_tokens.is_none()
        {
            return Ok(None);
        }
        return Ok(Some(DirectThreadResponderConfig {
            agent_name: MAIN_AGENT_NAME.to_string(),
            provider_id: profile_provider.unwrap_or_else(|| config.provider.clone()),
            model: profile_model,
            base_url: None,
            reasoning_effort: profile_reasoning_effort,
            context_window_tokens: profile_context_window_tokens,
            huggingface_provider: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            system_prompt: config.system_prompt.clone(),
            persona_prompt: String::new(),
            tool_filter: None,
        }));
    }
    if resolved_scope == CONCIERGE_AGENT_ID {
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        let Some(provider_config) =
            crate::agent::concierge::resolve_concierge_provider(config).ok()
        else {
            return Ok(None);
        };
        return Ok(Some(DirectThreadResponderConfig {
            agent_name: CONCIERGE_AGENT_NAME.to_string(),
            provider_id,
            model: Some(provider_config.model.clone()),
            base_url: nonempty(config.concierge.base_url.as_deref()),
            reasoning_effort: Some(provider_config.reasoning_effort.clone()),
            context_window_tokens: Some(provider_config.context_window_tokens),
            huggingface_provider: provider_config.huggingface_provider.clone(),
            openrouter_provider_order: config.concierge.openrouter_provider_order.clone(),
            openrouter_provider_ignore: config.concierge.openrouter_provider_ignore.clone(),
            openrouter_allow_fallbacks: config.concierge.openrouter_allow_fallbacks,
            system_prompt: crate::agent::concierge::concierge_system_prompt(),
            persona_prompt: String::new(),
            tool_filter: None,
        }));
    }
    let matched_def = resolved_target.matched_sub_agent.clone();
    let builtin_persona_overrides = builtin_persona_overrides(config, resolved_scope);
    let profile_provider =
        nonempty(execution_profile.and_then(|profile| profile.provider.as_deref()));
    let profile_model = nonempty(execution_profile.and_then(|profile| profile.model.as_deref()));
    let profile_reasoning_effort =
        nonempty(execution_profile.and_then(|profile| profile.reasoning_effort.as_deref()));
    let profile_context_window_tokens =
        execution_profile.and_then(|profile| profile.context_window_tokens);
    if is_explicit_builtin_persona_scope(resolved_scope)
        && builtin_persona_requires_setup(config, resolved_scope)
        && matched_def.is_none()
        && profile_provider.is_none()
        && profile_model.is_none()
    {
        return Err(builtin_persona_setup_error(resolved_scope));
    }
    let persona_prompt = if resolved_scope == crate::agent::agent_identity::WELES_AGENT_ID {
        crate::agent::agent_identity::build_weles_persona_prompt(
            crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
        )
    } else if let Some(def) = matched_def.as_ref().filter(|def| !def.builtin) {
        crate::agent::agent_identity::build_user_subagent_persona_prompt(def)
    } else {
        build_spawned_persona_prompt(resolved_scope)
    };
    Ok(Some(DirectThreadResponderConfig {
        agent_name: resolved_target.agent_name,
        provider_id: profile_provider
            .clone()
            .or_else(|| {
                matched_def
                    .as_ref()
                    .and_then(|def| nonempty(Some(def.provider.as_str())))
            })
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| nonempty(overrides.provider.as_deref()))
            })
            .unwrap_or_else(|| config.provider.clone()),
        model: profile_model
            .clone()
            .or_else(|| {
                matched_def
                    .as_ref()
                    .and_then(|def| nonempty(Some(def.model.as_str())))
            })
            .or_else(|| {
                builtin_persona_overrides.and_then(|overrides| nonempty(overrides.model.as_deref()))
            }),
        base_url: if profile_provider.is_some() {
            None
        } else {
            matched_def
                .as_ref()
                .and_then(|def| nonempty(def.base_url.as_deref()))
        },
        reasoning_effort: profile_reasoning_effort
            .clone()
            .or_else(|| {
                matched_def
                    .as_ref()
                    .and_then(|def| nonempty(def.reasoning_effort.as_deref()))
            })
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| nonempty(overrides.reasoning_effort.as_deref()))
            }),
        context_window_tokens: profile_context_window_tokens
            .or_else(|| {
                matched_def
                    .as_ref()
                    .and_then(|def| def.context_window_tokens)
            })
            .or_else(|| {
                builtin_persona_overrides.and_then(|overrides| overrides.context_window_tokens)
            }),
        huggingface_provider: matched_def
            .as_ref()
            .and_then(|def| def.huggingface_provider.clone()),
        openrouter_provider_order: matched_def
            .as_ref()
            .map(|def| def.openrouter_provider_order.clone())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .map(|overrides| overrides.openrouter_provider_order.clone())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_default(),
        openrouter_provider_ignore: matched_def
            .as_ref()
            .map(|def| def.openrouter_provider_ignore.clone())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .map(|overrides| overrides.openrouter_provider_ignore.clone())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_default(),
        openrouter_allow_fallbacks: matched_def
            .as_ref()
            .and_then(|def| def.openrouter_allow_fallbacks)
            .or_else(|| {
                builtin_persona_overrides.and_then(|overrides| overrides.openrouter_allow_fallbacks)
            }),
        system_prompt: matched_def
            .as_ref()
            .and_then(|def| def.system_prompt.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.system_prompt.clone()),
        persona_prompt,
        tool_filter: matched_def.as_ref().and_then(|def| {
            if def.tool_whitelist.is_some() || def.tool_blacklist.is_some() {
                crate::agent::subagent::tool_filter::ToolFilter::new(
                    def.tool_whitelist.clone(),
                    def.tool_blacklist.clone(),
                )
                .ok()
            } else {
                None
            }
        }),
    }))
}

async fn current_visible_thread_responder_is_active_participant(
    engine: &AgentEngine,
    thread_id: &str,
) -> bool {
    if is_internal_dm_thread(thread_id)
        || is_participant_playground_thread(thread_id)
        || crate::agent::agent_identity::is_goal_run_thread(thread_id)
        || is_internal_handoff_thread(thread_id)
    {
        return false;
    }
    let Some(active_agent_id) = engine.active_agent_id_for_thread(thread_id).await else {
        return false;
    };
    engine
        .list_thread_participants(thread_id)
        .await
        .into_iter()
        .any(|participant| {
            participant.status == ThreadParticipantStatus::Active
                && participant.agent_id.eq_ignore_ascii_case(&active_agent_id)
        })
}

async fn visible_thread_has_participants(engine: &AgentEngine, thread_id: &str) -> bool {
    if is_internal_dm_thread(thread_id)
        || is_participant_playground_thread(thread_id)
        || crate::agent::agent_identity::is_goal_run_thread(thread_id)
        || is_internal_handoff_thread(thread_id)
    {
        return false;
    }
    !engine.list_thread_participants(thread_id).await.is_empty()
}

fn spawn_background_community_scout(
    engine: &AgentEngine,
    thread_id: &str,
    query: &str,
    config: &AgentConfig,
) {
    if !config.skill_recommendation.background_community_search {
        return;
    }

    let event_tx = engine.event_tx.clone();
    let data_dir = engine
        .data_dir
        .parent()
        .unwrap_or(engine.data_dir.as_path())
        .to_path_buf();
    let registry_url = config
        .extra
        .get("registry_url")
        .and_then(|value| value.as_str())
        .unwrap_or("https://registry.zorai.dev")
        .to_string();
    let community_preapprove_timeout_secs = config
        .skill_recommendation
        .community_preapprove_timeout_secs;
    let suggest_global_enable_after_approvals = config
        .skill_recommendation
        .suggest_global_enable_after_approvals;
    let thread_id = thread_id.to_string();
    let query = query.to_string();

    tokio::spawn(async move {
        let (candidates, error) =
            match crate::agent::skill_recommendation::discover_community_skills(
                &data_dir,
                &registry_url,
                &query,
                COMMUNITY_SCOUT_RESULT_LIMIT,
            )
            .await
            {
                Ok(candidates) => (candidates, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };

        let message = if candidates.is_empty() {
            "Background community scout found no additional install candidates.".to_string()
        } else {
            format!(
                "Background community scout found {} install candidate(s).",
                candidates.len()
            )
        };
        let details = serde_json::json!({
            "query": query,
            "candidates": candidates,
            "community_preapprove_timeout_secs": community_preapprove_timeout_secs,
            "suggest_global_enable_after_approvals": suggest_global_enable_after_approvals,
            "error": error,
        });

        let _ = event_tx.send(AgentEvent::WorkflowNotice {
            thread_id,
            kind: "skill-community-scout".to_string(),
            message,
            details: Some(details.to_string()),
        });
    });
}

fn retain_resolved_session(
    resolved_session_id: Option<zorai_protocol::SessionId>,
    compatible_source: Option<&zorai_protocol::SessionInfo>,
) -> Option<zorai_protocol::SessionId> {
    compatible_source
        .map(|session| session.id)
        .or(resolved_session_id)
}

impl<'a> SendMessageRunner<'a> {
    pub(super) async fn initialize(
        engine: &'a AgentEngine,
        thread_id: Option<&'a str>,
        stored_user_content: &'a str,
        stored_user_content_blocks: &[AgentContentBlock],
        llm_user_content: &'a str,
        task_id: Option<&'a str>,
        preferred_session_hint: Option<&'a str>,
        stream_chunk_timeout_override: Option<std::time::Duration>,
        client_surface: Option<zorai_protocol::ClientSurface>,
        record_operator: bool,
        reuse_existing_user_message: bool,
        initial_scheduled_retry_cycles: u32,
    ) -> Result<Self> {
        let mut config = engine.config.read().await.clone();
        let (tid, is_new_thread) = engine
            .get_or_create_thread(thread_id, stored_user_content)
            .await;
        ensure_thread_artifact_dirs(engine.history.data_root(), &tid).await?;
        engine.ensure_thread_messages_loaded(&tid).await;
        if let Some(client_surface) = client_surface {
            engine.set_thread_client_surface(&tid, client_surface).await;
        }
        if let Some(task_id) = task_id {
            let mut persist_task_queue_after_thread_init = false;
            let (current_task, thread_changed) = {
                let mut tasks = engine.tasks.lock().await;
                match tasks.iter_mut().find(|task| task.id == task_id) {
                    Some(task) => {
                        let thread_changed = task.thread_id.as_deref() != Some(tid.as_str());
                        if thread_changed {
                            task.thread_id = Some(tid.clone());
                            persist_task_queue_after_thread_init = true;
                        }
                        (Some(task.clone()), thread_changed)
                    }
                    None => (None, false),
                }
            };
            let (current_task, thread_changed) = match current_task {
                Some(current_task) => (Some(current_task), thread_changed),
                None => {
                    if let Some(mut task) =
                        load_current_task_for_send_message(engine, Some(task_id)).await
                    {
                        let thread_changed = task.thread_id.as_deref() != Some(tid.as_str());
                        if thread_changed {
                            task.thread_id = Some(tid.clone());
                            if let Err(error) = engine.history.upsert_agent_task(&task).await {
                                tracing::warn!(
                                    task_id,
                                    "failed to persist task thread initialization: {error}"
                                );
                            } else {
                                let mut tasks = engine.tasks.lock().await;
                                if !tasks.iter().any(|entry| entry.id == task.id) {
                                    tasks.push_back(task.clone());
                                }
                                persist_task_queue_after_thread_init = true;
                            }
                        }
                        (Some(task), thread_changed)
                    } else {
                        (None, false)
                    }
                }
            };
            if let Some(current_task) = current_task {
                engine
                    .set_thread_identity_from_task(&tid, &current_task)
                    .await;
                engine.persist_thread_by_id(&tid).await;
                if thread_changed && persist_task_queue_after_thread_init {
                    engine.persist_tasks().await;
                    engine.emit_task_update(
                        &current_task,
                        Some(format!("Task thread initialized: {tid}")),
                    );
                }
                if let Some(goal_run_id) = current_task.goal_run_id.as_deref() {
                    engine
                        .sync_goal_run_with_task(goal_run_id, &current_task)
                        .await;
                }
            }
        }
        if !reuse_existing_user_message {
            if record_operator {
                engine
                    .clear_deferred_visible_thread_continuations(&tid)
                    .await;
            }
            {
                let mut threads = engine.threads.write().await;
                if let Some(thread) = threads.get_mut(&tid) {
                    thread.messages.push(AgentMessage::user_with_blocks(
                        stored_user_content,
                        stored_user_content_blocks.to_vec(),
                        now_millis(),
                    ));
                    thread.updated_at = now_millis();
                }
            }
            engine.persist_thread_by_id(&tid).await;
            if record_operator {
                engine
                    .record_operator_message(&tid, stored_user_content, is_new_thread)
                    .await?;
                if let Err(error) = engine.maybe_sync_thread_to_honcho(&tid).await {
                    tracing::warn!(thread_id = %tid, error = %error, "failed to sync thread to Honcho");
                }
            }

            if let Some(ack_message) = engine.take_continuity_acknowledgment(&tid).await {
                let mut threads = engine.threads.write().await;
                if let Some(thread) = threads.get_mut(&tid) {
                    let mut msg = AgentMessage::user(&ack_message, now_millis());
                    msg.role = MessageRole::System;
                    thread.messages.push(msg);
                }
            }
            if let Some(hint) = engine.try_augment_plugin_command(stored_user_content).await {
                let mut threads = engine.threads.write().await;
                if let Some(thread) = threads.get_mut(&tid) {
                    let mut msg = AgentMessage::user(&hint, now_millis());
                    msg.role = MessageRole::System;
                    thread.messages.push(msg);
                }
            }
        }
        if config.mlflow_tracing.effectively_enabled() {
            let threads = engine.threads.read().await;
            if let Some(thread) = threads.get(&tid) {
                if let Some(message) = thread
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == MessageRole::User)
                {
                    engine.mlflow_tracing.push_turn_anchor(
                        &tid,
                        crate::agent::mlflow_tracing::MlflowTurnAnchor {
                            user_message_id: message.id.clone(),
                            content: message.content.clone(),
                            timestamp_ms: message.timestamp,
                        },
                    );
                }
            }
        }
        let agent_scope_id = current_agent_scope_id();
        let sub_agents = engine.list_sub_agents().await;
        let current_task_for_setup = load_current_task_for_send_message(engine, task_id).await;
        let task_provider_override = current_task_for_setup.as_ref().and_then(|t| {
            if let Some(provider) = t.override_provider.as_ref() {
                let def_base_url = t.sub_agent_def_id.as_ref().and_then(|def_id| {
                    sub_agents
                        .iter()
                        .find(|def| def.id == *def_id)
                        .filter(|def| def.provider == *provider)
                        .and_then(|def| def.base_url.clone())
                });
                return Some((
                    provider.clone(),
                    t.override_model.clone(),
                    t.override_system_prompt.clone(),
                    t.sub_agent_def_id.clone(),
                    t.override_api_transport,
                    None,
                    None,
                    def_base_url,
                ));
            }

            let sub_agent_def_id = t.sub_agent_def_id.as_ref()?;
            let def = sub_agents.iter().find(|def| def.id == *sub_agent_def_id)?;
            Some((
                def.provider.clone(),
                Some(def.model.clone()),
                t.override_system_prompt.clone(),
                Some(def.id.clone()),
                def.api_transport,
                def.context_window_tokens,
                def.huggingface_provider.clone(),
                def.base_url.clone(),
            ))
        });
        let thread_execution_profile = engine
            .thread_execution_profiles
            .read()
            .await
            .get(&tid)
            .cloned();
        let task_provider_override = apply_thread_profile_to_task_provider_override(
            task_provider_override,
            thread_execution_profile.as_ref(),
            task_id.is_some(),
        );
        let task_execution_profile_reasoning = thread_execution_profile
            .as_ref()
            .and_then(|profile| profile.reasoning_effort.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let mut direct_thread_responder = task_id
            .is_none()
            .then(|| {
                build_direct_thread_responder_config(
                    &config,
                    &agent_scope_id,
                    &sub_agents,
                    thread_execution_profile.as_ref(),
                )
            })
            .transpose()?
            .flatten();
        let active_provider_id = task_provider_override
            .as_ref()
            .map(|(provider_id, _, _, _, _, _, _, _)| provider_id.as_str())
            .or_else(|| {
                direct_thread_responder
                    .as_ref()
                    .map(|responder| responder.provider_id.as_str())
            })
            .unwrap_or(config.provider.as_str())
            .to_string();
        let provider_config = match if let Some((
            ref sub_provider,
            ref sub_model,
            _,
            _,
            sub_transport,
            sub_context_window_tokens,
            ref sub_huggingface_provider,
            ref sub_base_url,
        )) = task_provider_override
        {
            let mut pc = engine.resolve_sub_agent_provider_config(&config, sub_provider)?;
            if let Some(model) = sub_model {
                apply_provider_model_override(sub_provider, &mut pc, model);
            }
            crate::agent::provider_resolution::apply_role_transport_override(
                sub_provider,
                &mut pc,
                sub_transport,
            );
            if let Some(base_url) = sub_base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                pc.base_url = base_url.to_string();
            }
            if let Some(reasoning_effort) = task_execution_profile_reasoning.as_ref() {
                pc.reasoning_effort = reasoning_effort.clone();
            }
            if let Some(context_window_tokens) = thread_execution_profile
                .as_ref()
                .and_then(|profile| profile.context_window_tokens)
                .or(sub_context_window_tokens)
                .filter(|tokens| *tokens > 0)
            {
                pc.context_window_tokens = context_window_tokens;
            }
            if let Some(huggingface_provider) = sub_huggingface_provider
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            {
                pc.huggingface_provider = Some(huggingface_provider);
            }
            Ok(pc)
        } else if let Some(responder) = direct_thread_responder.as_ref() {
            let mut pc =
                engine.resolve_sub_agent_provider_config(&config, &responder.provider_id)?;
            if let Some(model) = responder.model.as_ref() {
                apply_provider_model_override(&responder.provider_id, &mut pc, model);
            }
            if let Some(base_url) = responder
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                pc.base_url = base_url.to_string();
            }
            if let Some(reasoning_effort) = responder.reasoning_effort.as_ref() {
                pc.reasoning_effort = reasoning_effort.clone();
            }
            if let Some(context_window_tokens) =
                responder.context_window_tokens.filter(|tokens| *tokens > 0)
            {
                pc.context_window_tokens = context_window_tokens;
            }
            if let Some(huggingface_provider) = responder
                .huggingface_provider
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            {
                pc.huggingface_provider = Some(huggingface_provider);
            }
            apply_openrouter_routing_override(
                &responder.provider_id,
                &mut pc,
                &responder.openrouter_provider_order,
                &responder.openrouter_provider_ignore,
                responder.openrouter_allow_fallbacks,
            );
            Ok(pc)
        } else {
            engine.resolve_provider_config(&config)
        } {
            Ok(provider_config) => provider_config,
            Err(error) => {
                let error_text = sanitize_upstream_failure_message(&error.to_string());
                engine
                    .add_assistant_message(
                        &tid,
                        &format!("Error: {error_text}"),
                        0,
                        0,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .await;
                engine.persist_thread_by_id(&tid).await;
                engine
                    .emit_turn_error_completion(&tid, &error_text, None, None)
                    .await;
                return Err(error);
            }
        };
        config.provider = active_provider_id.clone();
        config.base_url = provider_config.base_url.clone();
        config.model = provider_config.model.clone();
        config.api_key = provider_config.api_key.clone();
        config.assistant_id = provider_config.assistant_id.clone();
        config.auth_source = provider_config.auth_source;
        config.api_transport = provider_config.api_transport;
        config.reasoning_effort = provider_config.reasoning_effort.clone();
        config.context_window_tokens = provider_config.context_window_tokens;
        let (stream_generation, stream_cancel_token, stream_retry_now) =
            engine.begin_stream_cancellation(&tid).await;
        engine
            .tag_stream_provider(&tid, stream_generation, &active_provider_id)
            .await;
        if engine.operator_stream_stop_requested(&tid).await {
            stream_cancel_token.cancel();
        }
        let onecontext_bootstrap = if is_new_thread {
            engine
                .onecontext_bootstrap_for_new_thread(stored_user_content)
                .await
        } else {
            None
        };
        let mut preferred_session_id =
            resolve_preferred_session_id(&engine.session_manager, preferred_session_hint).await;
        if let Some(task) = current_task_for_setup.as_ref() {
            match engine.ensure_isolated_task_workspace(&tid, task).await {
                Ok(Some(isolated)) => {
                    let sessions = engine.session_manager.list().await;
                    match select_isolated_task_session(
                        &sessions,
                        task.session_id.as_deref(),
                        preferred_session_hint,
                        &isolated.workspace_root,
                        &isolated.root,
                    ) {
                        IsolatedTaskSessionPlan::Reuse(isolated_session) => {
                            preferred_session_id = Some(isolated_session.id);
                            let isolated_session_id = isolated_session.id.to_string();
                            if task.session_id.as_deref() != Some(isolated_session_id.as_str()) {
                                let _ = engine
                                    .bind_task_isolated_session(task, isolated_session.id)
                                    .await;
                            }
                        }
                        IsolatedTaskSessionPlan::CloneFrom(source_session) => {
                            match engine
                                .session_manager
                                .clone_session(
                                    source_session.id,
                                    source_session.workspace_id.clone(),
                                    None,
                                    None,
                                    false,
                                    Some(isolated.root.clone()),
                                )
                                .await
                            {
                                Ok((isolated_session_id, _, _)) => {
                                    preferred_session_id = Some(isolated_session_id);
                                    let _ = engine
                                        .bind_task_isolated_session(task, isolated_session_id)
                                        .await;
                                    engine
                                        .register_agent_terminal_lease(
                                            crate::agent::terminal_leases::AgentTerminalLease::new(
                                                isolated_session_id,
                                                source_session.workspace_id.clone(),
                                                Some(task.id.clone()),
                                                task.thread_id
                                                    .clone()
                                                    .or_else(|| Some(tid.clone())),
                                                crate::agent::task_prompt::now_millis(),
                                            ),
                                        )
                                        .await;
                                    let _ = engine.event_tx.send(AgentEvent::WorkspaceCommand {
                                        command: "attach_agent_terminal".to_string(),
                                        args: serde_json::json!({
                                            "workspace_id": source_session.workspace_id,
                                            "session_id": isolated_session_id.to_string(),
                                            "pane_name": format!("Isolated {}", task.title),
                                            "cwd": isolated.root,
                                            "task_id": task.id,
                                            "goal_run_id": task.goal_run_id,
                                            "branch": isolated.branch,
                                            "worktree_created": isolated.created,
                                            "auto_owned": true,
                                        }),
                                    });
                                }
                                Err(error) => {
                                    preferred_session_id = retain_resolved_session(
                                        preferred_session_id,
                                        Some(&source_session),
                                    );
                                    tracing::warn!(
                                        thread_id = %tid,
                                        task_id = %task.id,
                                        %error,
                                        "failed to clone isolated task terminal"
                                    );
                                }
                            }
                        }
                        IsolatedTaskSessionPlan::Unavailable => {
                            preferred_session_id =
                                retain_resolved_session(preferred_session_id, None);
                            tracing::warn!(
                                thread_id = %tid,
                                task_id = %task.id,
                                workspace_root = %isolated.workspace_root,
                                "no project-compatible terminal is available for isolated task"
                            );
                        }
                    }
                }
                Ok(None) => {
                    if preferred_session_id.is_none() {
                        preferred_session_id = engine
                            .session_manager
                            .list()
                            .await
                            .into_iter()
                            .find(|session| session.workspace_id.is_some())
                            .map(|session| session.id);
                    }
                }
                Err(error) => tracing::warn!(
                    thread_id = %tid,
                    task_id = %task.id,
                    %error,
                    "failed to provision isolated task worktree"
                ),
            }
        }
        let skill_preflight = if super::skill_preflight::should_run_skill_preflight_for_turn(
            record_operator,
            task_id.is_some(),
            stored_user_content,
        ) {
            engine
                .build_skill_preflight_context(
                    &tid,
                    stored_user_content,
                    preferred_session_id.clone(),
                )
                .await?
        } else {
            None
        };
        let mut skill_preflight = match skill_preflight {
            Some(context) => Some(context),
            None => engine
                .get_thread_skill_discovery_state(&tid)
                .await
                .filter(|state| !state.compliant)
                .map(|state| super::skill_preflight::SkillPreflightContext {
                    prompt_context: super::skill_preflight::build_skill_gate_override_prompt(
                        &state,
                    ),
                    workflow_message: super::skill_preflight::skill_preflight_workflow_message(
                        &state,
                    ),
                    workflow_details: serde_json::to_string(&state).ok(),
                    state,
                }),
        };
        if let Some(skill_preflight) = skill_preflight.as_mut() {
            let mut next_state = skill_preflight.state.clone();
            if let Some(previous_state) = engine.get_thread_skill_discovery_state(&tid).await {
                if super::skill_preflight::preserve_noncompliant_mesh_state(
                    &previous_state,
                    &mut next_state,
                ) {
                    skill_preflight.workflow_message =
                        super::skill_preflight::skill_preflight_workflow_message(&next_state);
                    skill_preflight.workflow_details = serde_json::to_string(&next_state).ok();
                    skill_preflight.prompt_context =
                        super::skill_preflight::build_skill_gate_override_prompt(&next_state);
                }
            }
            skill_preflight.state = next_state.clone();
            engine
                .set_thread_skill_discovery_state(&tid, next_state)
                .await;
        }
        let memory = engine.current_memory_snapshot().await;
        let memory_paths = memory_paths_for_scope(&engine.data_dir, &agent_scope_id);
        let base_prompt = if let Some((_, _, Some(ref override_prompt), _, _, _, _, _)) =
            task_provider_override
        {
            format!("{}\n\n{}", override_prompt, config.system_prompt)
        } else if let Some(responder) = direct_thread_responder.as_ref() {
            format!(
                "{}\n\n{}",
                responder.persona_prompt, responder.system_prompt
            )
        } else {
            config.system_prompt.clone()
        };
        let operator_model_summary = engine.build_operator_model_prompt_summary().await;
        let operational_context = engine.build_operational_context_summary().await;
        let causal_guidance = engine.build_causal_guidance_summary().await;
        let learned_patterns = {
            let hs = engine.heuristic_store.read().await;
            let patterns = build_learned_patterns_section(&hs);
            if patterns.is_empty() {
                None
            } else {
                Some(patterns)
            }
        };
        let (
            current_task_snapshot,
            is_durable_goal_task,
            mut task_tool_filter,
            task_context_budget,
            task_termination_eval,
            task_type_for_trace,
        ) = {
            let current_task = current_task_for_setup.clone();
            let is_goal = current_task
                .as_ref()
                .and_then(|task| task.goal_run_id.as_ref())
                .is_some();
            let filter = current_task.as_ref().and_then(|task| {
                if task.tool_whitelist.is_some() || task.tool_blacklist.is_some() {
                    let mut filter = crate::agent::subagent::tool_filter::ToolFilter::new(
                        task.tool_whitelist.clone(),
                        task.tool_blacklist.clone(),
                    )
                    .ok()?;
                    if is_workspace_agent_task(task) {
                        allow_workspace_task_tools(&mut filter);
                    }
                    if task.is_spawned_subagent() {
                        allow_subagent_report_back_tools(&mut filter);
                    }
                    Some(filter)
                } else {
                    None
                }
            });
            let budget = current_task.as_ref().and_then(|task| {
                task.context_budget_tokens.map(|max_tokens| {
                    crate::agent::subagent::context_budget::ContextBudget::new(
                        max_tokens,
                        task.context_overflow_action
                            .unwrap_or(crate::agent::types::ContextOverflowAction::Compress),
                    )
                })
            });
            let termination = current_task
                .as_ref()
                .and_then(|task| task.termination_conditions.as_deref())
                .and_then(|dsl| {
                    crate::agent::subagent::termination::TerminationEvaluator::parse(dsl).ok()
                });
            let task_type = current_task
                .as_ref()
                .map(|task| classify_task(task).to_string())
                .unwrap_or_default();
            (
                current_task,
                is_goal,
                filter,
                budget,
                termination,
                task_type,
            )
        };
        let internal_dm_thread = is_internal_dm_thread(&tid);
        let _participant_playground_thread = is_participant_playground_thread(&tid);
        let workspace_task_context = current_task_snapshot
            .as_ref()
            .is_some_and(is_workspace_agent_task)
            || engine
                .history
                .get_workspace_task_by_thread_id(&tid)
                .await?
                .is_some();
        if workspace_task_context {
            if let Some(filter) = task_tool_filter.as_mut() {
                allow_workspace_task_tools(filter);
            }
            if let Some(filter) = direct_thread_responder
                .as_mut()
                .and_then(|responder| responder.tool_filter.as_mut())
            {
                allow_workspace_task_tools(filter);
            }
        }
        if current_task_snapshot
            .as_ref()
            .is_some_and(AgentTask::is_spawned_subagent)
        {
            if let Some(filter) = task_tool_filter.as_mut() {
                allow_subagent_report_back_tools(filter);
            }
        }
        let initial_copilot_initiator = if record_operator {
            CopilotInitiator::User
        } else {
            CopilotInitiator::Agent
        };
        let weles_runtime_override = current_task_snapshot.as_ref().and_then(|task| {
            (task.sub_agent_def_id.as_deref()
                == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID))
            .then_some(task)
            .and_then(|task| {
                task.override_system_prompt
                    .as_deref()
                    .and_then(|override_prompt| {
                        crate::agent::weles_governance::parse_weles_internal_override_payload(
                            override_prompt,
                        )
                    })
            })
        });
        let runtime_context_query = select_runtime_context_query(
            current_task_snapshot.as_ref().and_then(|task| {
                task.goal_step_title
                    .as_deref()
                    .or(Some(task.title.as_str()))
            }),
            current_task_snapshot.as_ref().and_then(|task| {
                task.goal_run_title
                    .as_deref()
                    .or(Some(task.description.as_str()))
            }),
            Some(stored_user_content),
        );
        let runtime_work_scope = format_runtime_work_scope_label(
            current_task_snapshot
                .as_ref()
                .and_then(|task| task.goal_run_title.as_deref()),
            current_task_snapshot
                .as_ref()
                .and_then(|task| task.goal_step_title.as_deref()),
            current_task_snapshot
                .as_ref()
                .map(|task| task.title.as_str()),
        );
        let runtime_continuity = build_runtime_continuity_context(
            engine,
            runtime_work_scope.as_deref(),
            runtime_context_query.as_deref(),
        )
        .await;
        let structured_memory_summary =
            crate::agent::memory_context::build_structured_memory_summary(
                &memory,
                &memory_paths,
                runtime_continuity.continuity_summary.as_deref(),
                runtime_continuity.negative_constraints_context.as_deref(),
            );
        let existing_memory_injection_state = engine.get_thread_memory_injection_state(&tid).await;
        let mut system_prompt = if let Some((scope, _marker, inspection_context)) =
            weles_runtime_override.as_ref()
        {
            let tool_name = inspection_context
                .get("tool_name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let tool_args = inspection_context
                .get("tool_args")
                .unwrap_or(&serde_json::Value::Null);
            let security_level = match inspection_context
                .get("security_level")
                .and_then(|value| value.as_str())
                .unwrap_or("moderate")
            {
                "highest" => SecurityLevel::Highest,
                "lowest" => SecurityLevel::Lowest,
                "yolo" => SecurityLevel::Yolo,
                _ => SecurityLevel::Moderate,
            };
            let suspicion_reasons = inspection_context
                .get("suspicion_reasons")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let task_health_signals = inspection_context.get("task_health_signals");
            let mut prompt = build_weles_governance_runtime_prompt(
                &config,
                tool_name,
                tool_args,
                security_level,
                &suspicion_reasons,
                current_task_snapshot.as_ref(),
                task_health_signals,
            );
            if scope == crate::agent::agent_identity::WELES_VITALITY_SCOPE {
                prompt.push_str("\n\n## WELES Vitality Mode\n- This run is an internal vitality/self-health check.");
            }
            prompt
        } else {
            let identity_is_spawned = engine
                .thread_identity_metadata
                .read()
                .await
                .get(&tid)
                .is_some_and(|identity| identity.is_spawned_subagent());
            build_system_prompt(
                &config,
                &base_prompt,
                &memory,
                &memory_paths,
                &agent_scope_id,
                &sub_agents,
                spawned_subagent_turn(current_task_snapshot.as_ref(), identity_is_spawned),
                operator_model_summary.as_deref(),
                operational_context.as_deref(),
                causal_guidance.as_deref(),
                learned_patterns.as_deref(),
                None,
                runtime_continuity.continuity_summary.as_deref(),
                runtime_continuity.negative_constraints_context.as_deref(),
                client_surface,
            )
        };
        let runtime_agent_name = task_provider_override
            .as_ref()
            .and_then(|(_, _, prompt, sub_agent_def_id, _, _, _, _)| {
                if sub_agent_def_id.as_deref()
                    == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
                {
                    Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string())
                } else {
                    extract_persona_name(prompt.as_deref())
                }
            })
            .or_else(|| {
                direct_thread_responder
                    .as_ref()
                    .map(|responder| responder.agent_name.clone())
            })
            .unwrap_or_else(|| MAIN_AGENT_NAME.to_string());
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&build_runtime_identity_prompt(
            &runtime_agent_name,
            &active_provider_id,
            &provider_config.model,
        ));
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&thread_artifact_prompt_block(
            engine.history.data_root(),
            &tid,
        ));
        if let Some(workspace_context) = engine.get_thread_workspace_context(&tid).await {
            if let Some(workspace_prompt) = workspace_context.prompt_block() {
                system_prompt.push_str("\n\n");
                system_prompt.push_str(&workspace_prompt);
            }
        }
        if let Some(injection_state) =
            crate::agent::memory_context::append_structured_memory_summary_if_needed(
                &mut system_prompt,
                existing_memory_injection_state.as_ref(),
                &structured_memory_summary,
                false,
                weles_runtime_override.is_none(),
            )
        {
            engine
                .set_thread_memory_injection_state(&tid, injection_state)
                .await;
        }
        if let Some(memory_palace_context) = engine
            .build_memory_palace_prompt_context(&tid, task_id)
            .await
        {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&memory_palace_context);
        }
        if let Some(parent_child_context) = engine.build_parent_child_prompt_context(task_id).await
        {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&parent_child_context);
        }
        if let Some(anticipatory_context) = engine.build_anticipatory_prompt_context(&tid).await {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&anticipatory_context);
        }
        match engine
            .build_protocol_prompt_context(&tid, stored_user_content)
            .await
        {
            Ok(Some(protocol_context)) => {
                system_prompt.push_str("\n\n");
                system_prompt.push_str(&protocol_context);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(thread_id = %tid, error = %error, "failed to decode emergent protocol token for prompt");
            }
        }
        if internal_dm_thread {
            system_prompt.push_str(
                "\n\n## Internal DM Constraints\n- This thread is an internal DM between agents.\n- Internal DMs are for discussion and coordination only.\n- Do not continue visible-thread work here.\n- Do not call tools in this thread.\n- If a visible thread continuation was explicitly requested, reply briefly here and stop. The daemon will continue the visible thread separately.\n",
            );
        }
        if let Some(recall) = onecontext_bootstrap.as_deref() {
            system_prompt.push_str("\n\n## OneContext Recall\n");
            system_prompt
                .push_str("Use this as historical context from prior sessions when relevant:\n");
            system_prompt.push_str(recall);
        }
        if let Some(skill_preflight) = skill_preflight.as_ref() {
            system_prompt.push_str("\n\n## Preloaded Skills\n");
            system_prompt.push_str(&skill_preflight.prompt_context);
        }
        match engine
            .maybe_build_honcho_context(&tid, stored_user_content)
            .await
        {
            Ok(Some(honcho_context)) => {
                system_prompt.push_str("\n\n## Cross-Session Memory\n");
                system_prompt.push_str(&honcho_context);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(thread_id = %tid, error = %error, "failed to build Honcho context");
            }
        }
        engine.emit_workflow_notice(
            &tid,
            "memory-consulted",
            "Loaded persistent memory, user profile, and local skill paths for this turn.",
            Some(format!(
                "memory_dir={}; skills_dir={}",
                memory_paths.memory_dir.display(),
                skills_dir(&engine.data_dir).display()
            )),
        );
        if skill_preflight.is_some() {
            let skill_preflight = skill_preflight.as_ref().expect("checked Some");
            engine.emit_workflow_notice(
                &tid,
                "skill-preflight",
                skill_preflight.workflow_message.clone(),
                skill_preflight.workflow_details.clone(),
            );
            spawn_background_community_scout(engine, &tid, stored_user_content, &config);
        }
        let has_workspace_topology = engine.session_manager.read_workspace_topology().is_some();
        let mut tools = get_available_tools(&config, &engine.data_dir, has_workspace_topology);
        crate::agent::tool_executor::filter_tools_for_client_surface(
            &mut tools,
            engine.get_thread_client_surface(&tid).await,
        );
        if let Some(filter) = &task_tool_filter {
            tools = filter.filtered_tools(tools);
        }
        if let Some(filter) = direct_thread_responder
            .as_ref()
            .and_then(|responder| responder.tool_filter.as_ref())
        {
            tools = filter.filtered_tools(tools);
        }
        let participant_managed_thread = visible_thread_has_participants(engine, &tid).await;
        if participant_managed_thread {
            tools.retain(|tool| tool.function.name != zorai_protocol::tool_names::LIST_AGENTS);
        } else {
            tools
                .retain(|tool| tool.function.name != zorai_protocol::tool_names::LIST_PARTICIPANTS);
        }
        if current_visible_thread_responder_is_active_participant(engine, &tid).await {
            tools.retain(|tool| {
                !PARTICIPANT_AGENT_FANOUT_TOOLS.contains(&tool.function.name.as_str())
            });
        }
        let preferred_tool_fallbacks = {
            let model = engine.operator_model.read().await;
            let adaptation =
                crate::agent::operator_model::BehaviorAdaptationProfile::from_model(&model);
            (
                adaptation.preferred_tool_fallbacks,
                adaptation.prompt_for_clarification,
            )
        };
        {
            let hs = engine.heuristic_store.read().await;
            super::tool_executor::reorder_tools_by_heuristics(
                &mut tools,
                &hs,
                &task_type_for_trace,
                &preferred_tool_fallbacks.0,
                preferred_tool_fallbacks.1,
            );
        }
        let tools_were_filtered = task_tool_filter.is_some()
            || direct_thread_responder
                .as_ref()
                .and_then(|responder| responder.tool_filter.as_ref())
                .is_some();
        let deferred_tool_pool = if config.tools.deferred_tool_loading && !tools_were_filtered {
            super::tool_executor::partition_deferred_tools(&mut tools)
        } else {
            Vec::new()
        };
        if !deferred_tool_pool.is_empty() {
            system_prompt.push_str(&format!(
                "\n\n## Deferred Tools\n- Only your most-used tools are currently active. {} more niche tools (routines, triggers, debate, collaboration, model switching, media, compliance/provenance, workspace layout, external messaging, etc.) exist but are withheld to save context.\n- To use one: call `tool_search` with what you need, then `load_tools` with the exact name(s) from the results, then call the now-activated tool. Calling a withheld tool directly also activates it.",
                deferred_tool_pool.len(),
            ));
        }
        if let Some(task) = current_task_snapshot.as_ref() {
            engine.ensure_subagent_runtime(task, Some(&tid)).await;
        }
        let retry_strategy = if !config.auto_retry {
            RetryStrategy::Bounded {
                max_retries: 0,
                retry_delay_ms: config.retry_delay_ms,
            }
        } else if is_durable_goal_task {
            RetryStrategy::DurableRateLimited
        } else {
            RetryStrategy::Bounded {
                max_retries: config.max_retries,
                retry_delay_ms: config.retry_delay_ms,
            }
        };
        let max_loops = config.max_tool_loops;
        engine
            .commit_thread_execution_profile_if_unchanged(
                &tid,
                thread_execution_profile.as_ref(),
                ThreadExecutionProfile {
                    provider: Some(active_provider_id.clone()),
                    model: (!provider_config.model.trim().is_empty())
                        .then(|| provider_config.model.clone()),
                    reasoning_effort: (!provider_config.reasoning_effort.trim().is_empty())
                        .then(|| provider_config.reasoning_effort.clone()),
                    context_window_tokens: Some(provider_config.context_window_tokens),
                },
            )
            .await;
        let _ = engine.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: tid.clone(),
        });
        Ok(Self {
            engine,
            task_id,
            stored_user_content,
            stored_user_content_blocks: stored_user_content_blocks.to_vec(),
            llm_user_content,
            stream_chunk_timeout_override,
            tid,
            reuse_existing_user_message,
            config,
            provider_config,
            preferred_session_id,
            onecontext_bootstrap,
            skill_preflight,
            agent_scope_id,
            runtime_agent_name,
            active_provider_id,
            memory_paths,
            base_prompt,
            operator_model_summary,
            operational_context,
            learned_patterns,
            continuity_summary: runtime_continuity.continuity_summary,
            negative_constraints_context: runtime_continuity.negative_constraints_context,
            system_prompt,
            current_task_snapshot,
            is_durable_goal_task,
            task_tool_filter,
            task_context_budget,
            task_termination_eval,
            task_type_for_trace: task_type_for_trace.clone(),
            initial_copilot_initiator,
            tools,
            deferred_tool_pool,
            retry_strategy,
            max_loops,
            stream_generation,
            stream_cancel_token,
            stream_retry_now,
            loop_count: 0,
            was_cancelled: false,
            interrupted_for_approval: false,
            terminated_for_budget: false,
            report_back_phase: None,
            tools_before_report_back: None,
            subagent_report: None,
            needs_turn_done: false,
            policy_aborted_retry: false,
            previous_tool_signature: None,
            previous_tool_outcome: None,
            last_tool_error: None,
            consecutive_same_tool_calls: 0,
            last_pre_compaction_flush_signature: None,
            recorded_compaction_provenance: false,
            trace_collector: crate::agent::learning::traces::TraceCollector::new(
                &task_type_for_trace,
                now_millis(),
            ),
            termination_tool_calls: 0,
            termination_tool_successes: 0,
            termination_consecutive_errors: 0,
            termination_total_errors: 0,
            loop_started_at: now_millis(),
            stream_timeout_count: 0,
            tool_ack_emitted: false,
            tool_sequence_repaired: false,
            retry_status_visible: initial_scheduled_retry_cycles > 0,
            scheduled_retry_cycles: initial_scheduled_retry_cycles,
            assistant_output_visible: false,
            tool_side_effect_committed: false,
            attempted_recovery_signatures: std::collections::HashSet::new(),
            recent_policy_tool_outcomes: VecDeque::new(),
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
            turn_done_emitted: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_identity::{
        MAIN_AGENT_ID, MAIN_AGENT_NAME, WELES_AGENT_ID, WELES_AGENT_NAME,
    };
    use crate::agent::types::{ApiTransport, AuthSource, ProviderConfig};
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    fn session(id: &str, cwd: &str) -> zorai_protocol::SessionInfo {
        zorai_protocol::SessionInfo {
            id: uuid::Uuid::parse_str(id).expect("valid session id"),
            title: Some("Agent lane".to_string()),
            cwd: Some(cwd.to_string()),
            cols: 120,
            rows: 40,
            created_at: 1,
            workspace_id: Some("workspace".to_string()),
            exit_code: None,
            is_alive: true,
            active_command: None,
        }
    }

    #[test]
    fn unavailable_isolation_keeps_resolved_turn_session() {
        let turn = session("11111111-1111-1111-1111-111111111111", "/unrelated");

        assert_eq!(retain_resolved_session(Some(turn.id), None), Some(turn.id));
    }

    #[test]
    fn clone_failure_prefers_compatible_source_over_unrelated_turn_session() {
        let turn = session("11111111-1111-1111-1111-111111111111", "/unrelated");
        let source = session("22222222-2222-2222-2222-222222222222", "/repo");

        assert_eq!(
            retain_resolved_session(Some(turn.id), Some(&source)),
            Some(source.id)
        );
    }

    #[tokio::test]
    async fn active_participant_responder_cannot_use_agent_fanout_tools() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_active_participant_tool_blacklist";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(WELES_AGENT_NAME.to_string()),
                title: "Participant tool blacklist".to_string(),
                messages: vec![AgentMessage::user("check this thread", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
        engine
            .set_thread_handoff_state(
                thread_id,
                ThreadHandoffState {
                    origin_agent_id: MAIN_AGENT_ID.to_string(),
                    active_agent_id: WELES_AGENT_ID.to_string(),
                    responder_stack: vec![
                        ThreadResponderFrame {
                            agent_id: MAIN_AGENT_ID.to_string(),
                            agent_name: MAIN_AGENT_NAME.to_string(),
                            entered_at: 1,
                            entered_via_handoff_event_id: None,
                            linked_thread_id: None,
                        },
                        ThreadResponderFrame {
                            agent_id: WELES_AGENT_ID.to_string(),
                            agent_name: WELES_AGENT_NAME.to_string(),
                            entered_at: 2,
                            entered_via_handoff_event_id: Some("handoff-1".to_string()),
                            linked_thread_id: Some("dm:svarog:weles".to_string()),
                        },
                    ],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;
        engine
            .upsert_thread_participant(thread_id, "weles", "verify claims")
            .await
            .expect("participant should register");

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "check this thread",
            &[],
            "check this thread",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        for forbidden_tool in PARTICIPANT_AGENT_FANOUT_TOOLS {
            assert!(
                !tool_names.contains(forbidden_tool),
                "active participant responder should not see {forbidden_tool}"
            );
        }
        assert!(
            tool_names.contains(&zorai_protocol::tool_names::HANDOFF_THREAD_AGENT),
            "active participant responder should still see handoff_thread_agent"
        );
    }

    #[tokio::test]
    async fn tui_originated_turn_does_not_expose_managed_terminal_tools() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let runner = SendMessageRunner::initialize(
            &engine,
            Some("thread_tui_tool_surface"),
            "inspect repository state",
            &[],
            "inspect repository state",
            None,
            None,
            None,
            Some(zorai_protocol::ClientSurface::Tui),
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            !tool_names.contains(&zorai_protocol::tool_names::EXECUTE_MANAGED_COMMAND),
            "TUI-originated turns should not expose managed terminal execution as a direct tool"
        );
        assert!(
            !tool_names.contains(&zorai_protocol::tool_names::ALLOCATE_TERMINAL),
            "TUI-originated turns should not expose managed terminal allocation"
        );
        for terminal_tool in [
            zorai_protocol::tool_names::LIST_SESSIONS,
            zorai_protocol::tool_names::LIST_TERMINALS,
            zorai_protocol::tool_names::READ_ACTIVE_TERMINAL_CONTENT,
            zorai_protocol::tool_names::RUN_TERMINAL_COMMAND,
            zorai_protocol::tool_names::TYPE_IN_TERMINAL,
        ] {
            assert!(
                !tool_names.contains(&terminal_tool),
                "TUI-originated turns should not expose {terminal_tool}"
            );
        }
        assert!(
            tool_names.contains(&zorai_protocol::tool_names::BASH_COMMAND),
            "TUI-originated turns should still expose headless shell execution"
        );
    }

    #[tokio::test]
    async fn inherited_tui_surface_does_not_expose_managed_terminal_tools() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_spawned_from_tui";
        engine
            .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
            .await;

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "continue delegated work",
            &[],
            "continue delegated work",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            !tool_names.contains(&zorai_protocol::tool_names::EXECUTE_MANAGED_COMMAND),
            "threads inheriting TUI origin should not expose managed terminal execution"
        );
        assert!(
            !tool_names.contains(&zorai_protocol::tool_names::ALLOCATE_TERMINAL),
            "threads inheriting TUI origin should not expose managed terminal allocation"
        );
        for terminal_tool in [
            zorai_protocol::tool_names::LIST_SESSIONS,
            zorai_protocol::tool_names::LIST_TERMINALS,
            zorai_protocol::tool_names::READ_ACTIVE_TERMINAL_CONTENT,
            zorai_protocol::tool_names::RUN_TERMINAL_COMMAND,
            zorai_protocol::tool_names::TYPE_IN_TERMINAL,
        ] {
            assert!(
                !tool_names.contains(&terminal_tool),
                "threads inheriting TUI origin should not expose {terminal_tool}"
            );
        }
    }

    #[tokio::test]
    async fn subagent_task_without_explicit_provider_uses_registered_definition_runtime() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "svarog-provider".to_string();
        config.base_url = "http://127.0.0.1:2/v1".to_string();
        config.model = "svarog-model".to_string();
        config.api_key = "test-key".to_string();
        config.api_transport = ApiTransport::ChatCompletions;
        config.providers.insert(
            "weles-provider".to_string(),
            ProviderConfig {
                base_url: "http://127.0.0.1:1/v1".to_string(),
                model: "weles-model".to_string(),
                api_key: "test-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::ChatCompletions,
                reasoning_effort: "medium".to_string(),
                context_window_tokens: 222_000,
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );
        config.builtin_sub_agents.weles.provider = Some("weles-provider".to_string());
        config.builtin_sub_agents.weles.model = Some("weles-model".to_string());
        config.builtin_sub_agents.weles.reasoning_effort = Some("medium".to_string());
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let task = engine
            .enqueue_task(
                "Repository state".to_string(),
                "Inspect repository state".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "event_trigger",
                None,
                None,
                Some("dm:svarog:weles".to_string()),
                Some("event_trigger".to_string()),
            )
            .await;
        {
            let mut tasks = engine.tasks.lock().await;
            let task = tasks
                .iter_mut()
                .find(|entry| entry.id == task.id)
                .expect("enqueued task should be present");
            task.sub_agent_def_id =
                Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
            task.override_system_prompt =
                Some(crate::agent::agent_identity::build_weles_persona_prompt(
                    crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
                ));
        }

        let runner = SendMessageRunner::initialize(
            &engine,
            Some("dm:svarog:weles"),
            "inspect repository state",
            &[],
            "inspect repository state",
            Some(&task.id),
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        assert_eq!(runner.config.provider, "weles-provider");
        assert_eq!(runner.provider_config.model, "weles-model");
        assert_eq!(runner.provider_config.reasoning_effort, "medium");
        assert_eq!(runner.provider_config.context_window_tokens, 222_000);
    }

    #[tokio::test]
    async fn runner_initialization_uses_persisted_task_provider_override() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4".to_string();
        config.providers.insert(
            "task-provider".to_string(),
            ProviderConfig {
                base_url: String::new(),
                model: "provider-default".to_string(),
                api_key: "task-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                reasoning_effort: "low".to_string(),
                context_window_tokens: 123_456,
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let thread_id = "thread-persisted-task-provider-override";
        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Persisted task provider override".to_string(),
                messages: vec![AgentMessage::user("check task override", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        let mut task = engine
            .enqueue_task(
                "Persisted provider override task".to_string(),
                "Initialize runner from persisted task override.".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some(thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        task.override_provider = Some("task-provider".to_string());
        task.override_model = Some("task-model".to_string());
        {
            let mut tasks = engine.tasks.lock().await;
            if let Some(existing) = tasks.iter_mut().find(|entry| entry.id == task.id) {
                *existing = task.clone();
            }
        }
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "check task override",
            &[],
            "check task override",
            Some(&task.id),
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize from persisted task metadata");

        assert_eq!(runner.config.provider, "task-provider");
        assert_eq!(runner.provider_config.model, "task-model");
        assert_eq!(runner.provider_config.context_window_tokens, 123_456);
    }

    fn spawnable_researcher() -> SubAgentDefinition {
        SubAgentDefinition {
            base_url: None,
            claude_permission_mode: None,
            id: "researcher".to_string(),
            name: "Researcher".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            role: Some("investigator".to_string()),
            system_prompt: Some("Investigate the assigned area.".to_string()),
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_window_tokens: None,
            max_duration_secs: None,
            supervisor_config: None,
            enabled: true,
            builtin: false,
            immutable_identity: false,
            disable_allowed: true,
            delete_allowed: true,
            protected_reason: None,
            reasoning_effort: None,
            api_transport: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            huggingface_provider: None,
            created_at: 1,
        }
    }

    async fn insert_test_thread(engine: &AgentEngine, thread_id: &str, title: &str) {
        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: title.to_string(),
                messages: vec![AgentMessage::user("continue", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    #[tokio::test]
    async fn spawned_subagent_system_prompt_omits_available_subagents() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4-mini".to_string();
        config.base_url = "http://127.0.0.1:1/v1".to_string();
        config.api_key = "test-key".to_string();
        config.sub_agents.push(spawnable_researcher());
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let main_thread_id = "thread_main_roster";
        insert_test_thread(&engine, main_thread_id, "Main thread").await;
        let main_runner = SendMessageRunner::initialize(
            &engine,
            Some(main_thread_id),
            "plan the work",
            &[],
            "plan the work",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("main-thread runner should initialize");
        assert!(
            main_runner
                .system_prompt
                .contains("## Subagent Supervision")
                && main_runner.system_prompt.contains("`list_agents`")
                && main_runner.system_prompt.contains("`spawn_subagent`")
                && !main_runner.system_prompt.contains("## Available Sub-Agents")
                && !main_runner.system_prompt.contains("Researcher"),
            "normal threads should fetch spawnable specialists on demand instead of inlining the roster"
        );

        let child_thread_id = "thread_spawned_child";
        insert_test_thread(&engine, child_thread_id, "Spawned child").await;
        let child_task = engine
            .enqueue_task(
                "Investigate parser".to_string(),
                "Stay scoped to the parser failure.".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some(main_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        engine
            .set_thread_identity_from_task(child_thread_id, &child_task)
            .await;

        let child_runner = SendMessageRunner::initialize(
            &engine,
            Some(child_thread_id),
            "execute spawned assignment",
            &[],
            "execute spawned assignment",
            Some(&child_task.id),
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("spawned-child runner should initialize");
        assert!(
            !child_runner
                .system_prompt
                .contains("## Available Sub-Agents")
                && !child_runner
                    .system_prompt
                    .contains("## Subagent Supervision"),
            "spawned subagents must not receive subagent supervision or spawn tooling in their initial prompt"
        );

        let continued_runner = SendMessageRunner::initialize(
            &engine,
            Some(child_thread_id),
            "continue in the reserved child thread",
            &[],
            "continue in the reserved child thread",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("continued spawned-thread runner should initialize");
        assert!(
            !continued_runner
                .system_prompt
                .contains("## Available Sub-Agents"),
            "continuing a spawned child thread must still omit spawnable specialist tooling"
        );
    }

    #[tokio::test]
    async fn goal_run_and_handoff_parent_thread_keeps_supervision_roster() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4-mini".to_string();
        config.base_url = "http://127.0.0.1:1/v1".to_string();
        config.api_key = "test-key".to_string();
        config.sub_agents.push(spawnable_researcher());
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let thread_id = "thread_goal_conversation";
        insert_test_thread(&engine, thread_id, "Goal conversation").await;

        for source in ["goal_run", "handoff"] {
            let task = engine
                .enqueue_task(
                    format!("{source} step"),
                    "Advance the parent conversation.".to_string(),
                    "normal",
                    None,
                    None,
                    Vec::new(),
                    None,
                    source,
                    None,
                    None,
                    Some(thread_id.to_string()),
                    Some("daemon".to_string()),
                )
                .await;
            assert!(
                !task.is_spawned_subagent(),
                "{source} tasks are parent turns even when parent_thread_id is the conversation"
            );
            engine.set_thread_identity_from_task(thread_id, &task).await;

            let runner = SendMessageRunner::initialize(
                &engine,
                Some(thread_id),
                "continue parent work",
                &[],
                "continue parent work",
                Some(&task.id),
                None,
                None,
                None,
                true,
                true,
                0,
            )
            .await
            .expect("parent-turn runner should initialize");
            assert!(
                runner.system_prompt.contains("## Subagent Supervision")
                    && runner.system_prompt.contains("`list_agents`")
                    && runner.system_prompt.contains("`spawn_subagent`")
                    && !runner.system_prompt.contains("## Available Sub-Agents")
                    && !runner.system_prompt.contains("Researcher")
                    && !runner.system_prompt.contains("## Subagent Report-Back"),
                "{source} parent turns must keep on-demand subagent tooling and omit the inlined roster"
            );

            let continued = SendMessageRunner::initialize(
                &engine,
                Some(thread_id),
                "continue without a task id",
                &[],
                "continue without a task id",
                None,
                None,
                None,
                None,
                true,
                true,
                0,
            )
            .await
            .expect("identity-only parent continuation should initialize");
            assert!(
                continued.system_prompt.contains("## Subagent Supervision")
                    && continued.system_prompt.contains("`list_agents`")
                    && !continued.system_prompt.contains("## Available Sub-Agents"),
                "{source} continuations that only read identity must not switch into child report-back or inline a roster"
            );
        }
    }

    #[test]
    fn spawned_subagent_turn_ignores_unspawned_identity() {
        assert!(
            !spawned_subagent_turn(None, false),
            "identity-only parent continuations must not enter the child report-back prompt"
        );
        assert!(
            spawned_subagent_turn(None, true),
            "identity-only spawned-child continuations must still enter report-back"
        );
    }

    #[test]
    fn direct_thread_responder_config_preserves_user_defined_subagent() {
        let mut config = AgentConfig::default();
        config.system_prompt = "Main system prompt".to_string();
        let sub_agents = vec![SubAgentDefinition {
            base_url: None,
            claude_permission_mode: None,
            id: "dola".to_string(),
            name: "Dola".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            role: Some("specialist".to_string()),
            system_prompt: Some("Handle delegated work.".to_string()),
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_window_tokens: None,
            max_duration_secs: None,
            supervisor_config: None,
            enabled: true,
            builtin: false,
            immutable_identity: false,
            disable_allowed: true,
            delete_allowed: true,
            protected_reason: None,
            reasoning_effort: Some("medium".to_string()),
            api_transport: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            huggingface_provider: None,
            created_at: 1,
        }];

        let responder = build_direct_thread_responder_config(&config, "dola", &sub_agents, None)
            .expect("config build should succeed")
            .expect("custom subagent should produce a direct responder config");

        assert_eq!(responder.agent_name, "Dola");
        assert_eq!(responder.provider_id, "openai");
        assert_eq!(responder.model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(responder.reasoning_effort.as_deref(), Some("medium"));
        assert_eq!(responder.system_prompt, "Handle delegated work.");
        assert!(
            responder.persona_prompt.contains("Dola"),
            "persona prompt should identify the targeted subagent"
        );
    }

    #[test]
    fn subagent_responder_uses_explicit_thread_execution_profile() {
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-main".to_string();
        let sub_agents = vec![SubAgentDefinition {
            id: "kimus".to_string(),
            name: "Kimus".to_string(),
            provider: "alibaba-token-plan".to_string(),
            model: "qwen3.8-max".to_string(),
            base_url: None,
            role: None,
            system_prompt: Some("Handle delegated work.".to_string()),
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_window_tokens: Some(128_000),
            max_duration_secs: None,
            supervisor_config: None,
            enabled: true,
            builtin: false,
            immutable_identity: false,
            disable_allowed: true,
            delete_allowed: true,
            protected_reason: None,
            reasoning_effort: Some("medium".to_string()),
            api_transport: None,
            claude_permission_mode: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            huggingface_provider: None,
            created_at: 1,
        }];
        let profile = ThreadExecutionProfile {
            provider: Some("ollama".to_string()),
            model: Some("kimi-k3:cloud".to_string()),
            reasoning_effort: Some("high".to_string()),
            context_window_tokens: Some(256_000),
        };

        let responder =
            build_direct_thread_responder_config(&config, "kimus", &sub_agents, Some(&profile))
                .expect("config build should succeed")
                .expect("subagent should produce a direct responder config");

        assert_eq!(responder.agent_name, "Kimus");
        assert_eq!(responder.provider_id, "ollama");
        assert_eq!(responder.model.as_deref(), Some("kimi-k3:cloud"));
        assert_eq!(responder.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(responder.context_window_tokens, Some(256_000));
        assert_eq!(responder.system_prompt, "Handle delegated work.");
    }

    #[test]
    fn main_responder_uses_explicit_thread_execution_profile() {
        let mut config = AgentConfig::default();
        config.provider = "openrouter".to_string();
        config.model = "meta/muse-spark-1.2-contributor".to_string();
        config.system_prompt = "Svarog system prompt".to_string();
        let profile = ThreadExecutionProfile {
            provider: Some("openai".to_string()),
            model: Some("gpt-5.6-sol".to_string()),
            reasoning_effort: Some("high".to_string()),
            context_window_tokens: Some(400_000),
        };

        let responder =
            build_direct_thread_responder_config(&config, MAIN_AGENT_ID, &[], Some(&profile))
                .expect("main responder profile should resolve")
                .expect("explicit main-thread profile should create an override");

        assert_eq!(responder.agent_name, MAIN_AGENT_NAME);
        assert_eq!(responder.provider_id, "openai");
        assert_eq!(responder.model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(responder.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(responder.context_window_tokens, Some(400_000));
    }

    #[tokio::test]
    async fn restored_spawned_persona_thread_uses_persisted_execution_profile() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4-mini".to_string();
        config.base_url = "http://127.0.0.1:1/v1".to_string();
        config.api_key = "test-key".to_string();
        config.system_prompt = "Main system prompt".to_string();
        let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
        let thread_id = "thread_restored_spawned_dazhbog";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Restored spawned Dazhbog".to_string(),
                messages: vec![AgentMessage::user("continue", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
        engine
            .set_thread_handoff_state(
                thread_id,
                ThreadHandoffState {
                    origin_agent_id: MAIN_AGENT_ID.to_string(),
                    active_agent_id: "dazhbog".to_string(),
                    responder_stack: vec![
                        ThreadResponderFrame {
                            agent_id: MAIN_AGENT_ID.to_string(),
                            agent_name: MAIN_AGENT_NAME.to_string(),
                            entered_at: 1,
                            entered_via_handoff_event_id: None,
                            linked_thread_id: None,
                        },
                        ThreadResponderFrame {
                            agent_id: "dazhbog".to_string(),
                            agent_name: "Dazhbog".to_string(),
                            entered_at: 2,
                            entered_via_handoff_event_id: Some("handoff-1".to_string()),
                            linked_thread_id: None,
                        },
                    ],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;
        engine
            .set_thread_execution_profile(
                thread_id,
                Some(ThreadExecutionProfile {
                    provider: Some("openai".to_string()),
                    model: Some("gpt-5.4-mini".to_string()),
                    reasoning_effort: Some("high".to_string()),
                    context_window_tokens: Some(1_048_576),
                }),
            )
            .await;
        engine.persist_thread_by_id(thread_id).await;

        let rehydrated = AgentEngine::new_test(
            SessionManager::new_test(root.path()).await,
            config,
            root.path(),
        )
        .await;
        rehydrated.hydrate().await.expect("rehydrate engine");

        let agent_scope_id = rehydrated
            .agent_scope_id_for_turn(Some(thread_id), None)
            .await;
        assert_eq!(agent_scope_id, "dazhbog");

        let runner = crate::agent::agent_identity::run_with_agent_scope(agent_scope_id, async {
            SendMessageRunner::initialize(
                &rehydrated,
                Some(thread_id),
                "continue",
                &[],
                "continue",
                None,
                None,
                None,
                None,
                true,
                true,
                0,
            )
            .await
        })
        .await
        .expect("runner should initialize from persisted execution profile");

        assert_eq!(runner.provider_config.model, "gpt-5.4-mini");
        assert_eq!(runner.provider_config.reasoning_effort, "high");
        assert_eq!(runner.runtime_agent_name, "Dazhbog");
    }

    #[tokio::test]
    async fn participant_managed_thread_replaces_list_agents_with_list_participants() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_participant_managed_tool_substitution";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
                title: "Participant-managed thread".to_string(),
                messages: vec![AgentMessage::user("check this thread", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
        engine
            .upsert_thread_participant(thread_id, "weles", "watch this thread")
            .await
            .expect("participant should register");

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "check this thread",
            &[],
            "check this thread",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            !tool_names.contains(&zorai_protocol::tool_names::LIST_AGENTS),
            "participant-managed thread should hide list_agents"
        );
        assert!(
            tool_names.contains(&zorai_protocol::tool_names::LIST_PARTICIPANTS),
            "participant-managed thread should expose list_participants"
        );
    }

    #[tokio::test]
    async fn runner_exposes_goal_run_tools_to_agents() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-goal-run-tools";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
                title: "Goal run tools".to_string(),
                messages: vec![AgentMessage::user("start a real goal", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "start a real goal",
            &[],
            "start a real goal",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            tool_names.contains(&zorai_protocol::tool_names::START_GOAL_RUN),
            "runner should expose start_goal_run"
        );
        assert!(
            tool_names.contains(&zorai_protocol::tool_names::LIST_GOAL_RUNS),
            "runner should expose list_goal_runs"
        );
    }

    #[tokio::test]
    async fn task_scoped_subagent_override_uses_thread_execution_profile_reasoning_effort() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let thread_id = "thread_task_scoped_subagent_reasoning_override";
        let task_id = "task_subagent_reasoning_override";
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4-mini".to_string();
        config.base_url = "http://127.0.0.1:1/v1".to_string();
        config.api_key = "test-key".to_string();
        config.reasoning_effort = "medium".to_string();
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Subagent".to_string()),
                title: "Task scoped subagent override".to_string(),
                messages: vec![AgentMessage::user("continue", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
        engine
            .set_thread_execution_profile(
                thread_id,
                Some(ThreadExecutionProfile {
                    provider: Some("openai".to_string()),
                    model: Some("gpt-5.4-mini".to_string()),
                    reasoning_effort: Some("high".to_string()),
                    context_window_tokens: Some(1_048_576),
                }),
            )
            .await;
        engine.tasks.lock().await.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Run scoped subagent".to_string(),
            description: "Continue the delegated subagent task.".to_string(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "subagent".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: Some("openai".to_string()),
            override_model: Some("gpt-5.4-mini".to_string()),
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "continue",
            &[],
            "continue",
            Some(task_id),
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        assert_eq!(runner.provider_config.reasoning_effort, "high");
    }

    #[tokio::test]
    async fn agent_task_prompt_runs_skill_preflight_without_operator_recording() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4-mini".to_string();
        config.base_url = "http://127.0.0.1:1/v1".to_string();
        config.api_key = "test-key".to_string();
        config.system_prompt = "Main system prompt".to_string();
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let task_id = "task_workspace_review_preflight";
        engine.tasks.lock().await.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Review workspace task".to_string(),
            description: "Review completion of the workspace implementation task.".to_string(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: "workspace_review".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: Some("openai".to_string()),
            override_model: Some("gpt-5.4-mini".to_string()),
            override_api_transport: None,
            override_system_prompt: Some(build_spawned_persona_prompt("qa")),
            sub_agent_def_id: None,
        });

        let prompt = "Review the delivered workspace task against the definition of done and submit the workspace review verdict.";
        let runner = SendMessageRunner::initialize(
            &engine,
            None,
            prompt,
            &[],
            prompt,
            Some(task_id),
            None,
            None,
            None,
            false,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        assert!(
            runner.system_prompt.contains("## Local Guidelines"),
            "agent task prompt should include the normal guideline section"
        );
        assert!(
            runner.system_prompt.contains("## Preloaded Skills"),
            "agent task prompt should run skill preflight even when the message is not recorded as an operator turn"
        );
    }

    #[tokio::test]
    async fn workspace_review_task_whitelist_keeps_workspace_review_tools() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4-mini".to_string();
        config.base_url = "http://127.0.0.1:1/v1".to_string();
        config.api_key = "test-key".to_string();
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let task_id = "task_workspace_review_whitelist";
        engine.tasks.lock().await.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Review workspace task".to_string(),
            description: "Review completion of the workspace implementation task.".to_string(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: "workspace_review".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: Some(vec![zorai_protocol::tool_names::CANCEL_TASK.to_string()]),
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: Some("openai".to_string()),
            override_model: Some("gpt-5.4-mini".to_string()),
            override_api_transport: None,
            override_system_prompt: Some(build_spawned_persona_prompt("qa")),
            sub_agent_def_id: None,
        });

        let prompt = "Review the delivered workspace task against the definition of done and submit the workspace review verdict.";
        let runner = SendMessageRunner::initialize(
            &engine,
            None,
            prompt,
            &[],
            prompt,
            Some(task_id),
            None,
            None,
            None,
            false,
            true,
            0,
        )
        .await
        .expect("runner should initialize");
        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            tool_names.contains(&zorai_protocol::tool_names::CANCEL_TASK),
            "original task whitelist entries should stay available"
        );
        assert!(
            tool_names.contains(&zorai_protocol::tool_names::WORKSPACE_SUBMIT_REVIEW),
            "workspace reviewer must be able to close the workspace task"
        );
        assert!(
            tool_names.contains(&zorai_protocol::tool_names::WORKSPACE_LIST_TASKS),
            "workspace reviewer should retain workspace task context tools"
        );
    }

    #[tokio::test]
    async fn workspace_assignee_thread_whitelist_keeps_workspace_completion_tools() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4-mini".to_string();
        config.base_url = "http://127.0.0.1:1/v1".to_string();
        config.api_key = "test-key".to_string();
        config.sub_agents.push(SubAgentDefinition {
            base_url: None,
            claude_permission_mode: None,
            id: "qa".to_string(),
            name: "QA".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            role: Some("reviewer".to_string()),
            system_prompt: Some("Handle workspace tasks.".to_string()),
            tool_whitelist: Some(vec![zorai_protocol::tool_names::CANCEL_TASK.to_string()]),
            tool_blacklist: None,
            context_budget_tokens: None,
            context_window_tokens: None,
            max_duration_secs: None,
            supervisor_config: None,
            enabled: true,
            builtin: false,
            immutable_identity: false,
            disable_allowed: true,
            delete_allowed: true,
            protected_reason: None,
            reasoning_effort: Some("medium".to_string()),
            api_transport: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            huggingface_provider: None,
            created_at: 1,
        });
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let mut task = engine
            .create_workspace_task(
                zorai_protocol::WorkspaceTaskCreate {
                    workspace_id: "main".to_string(),
                    title: "Implement workspace item".to_string(),
                    task_type: zorai_protocol::WorkspaceTaskType::Thread,
                    description: "Complete this workspace thread task.".to_string(),
                    definition_of_done: None,
                    priority: None,
                    assignee: Some(zorai_protocol::WorkspaceActor::Subagent("qa".to_string())),
                    reviewer: Some(zorai_protocol::WorkspaceActor::Subagent("qa".to_string())),
                },
                zorai_protocol::WorkspaceActor::User,
            )
            .await
            .expect("create workspace task");
        task.status = zorai_protocol::WorkspaceTaskStatus::InProgress;
        engine
            .history
            .upsert_workspace_task(&task)
            .await
            .expect("persist workspace task");
        let thread_id = task.thread_id.clone().expect("workspace thread id");
        engine.threads.write().await.insert(
            thread_id.clone(),
            AgentThread {
                id: thread_id.clone(),
                agent_name: Some("QA".to_string()),
                title: "Workspace assignee thread".to_string(),
                messages: vec![AgentMessage::user("continue", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
        engine
            .set_thread_handoff_state(
                &thread_id,
                ThreadHandoffState {
                    origin_agent_id: MAIN_AGENT_ID.to_string(),
                    active_agent_id: "qa".to_string(),
                    responder_stack: vec![ThreadResponderFrame {
                        agent_id: "qa".to_string(),
                        agent_name: "QA".to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    }],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;

        let runner = crate::agent::agent_identity::run_with_agent_scope("qa".to_string(), async {
            SendMessageRunner::initialize(
                &engine,
                Some(&thread_id),
                "continue",
                &[],
                "continue",
                None,
                None,
                None,
                None,
                true,
                true,
                0,
            )
            .await
        })
        .await
        .expect("runner should initialize");
        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            tool_names.contains(&zorai_protocol::tool_names::CANCEL_TASK),
            "original subagent whitelist entries should stay available"
        );
        assert!(
            tool_names.contains(&zorai_protocol::tool_names::WORKSPACE_SUBMIT_COMPLETION),
            "workspace assignee must be able to submit completion"
        );
        assert!(
            tool_names.contains(&zorai_protocol::tool_names::WORKSPACE_SUBMIT_REVIEW),
            "workspace reviewer must be able to submit review from the workspace thread"
        );
    }

    #[tokio::test]
    async fn non_participant_responder_keeps_agent_fanout_tools() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_non_participant_tool_access";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(WELES_AGENT_NAME.to_string()),
                title: "Non-participant tool access".to_string(),
                messages: vec![AgentMessage::user("check this thread", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
        engine
            .set_thread_handoff_state(
                thread_id,
                ThreadHandoffState {
                    origin_agent_id: MAIN_AGENT_ID.to_string(),
                    active_agent_id: WELES_AGENT_ID.to_string(),
                    responder_stack: vec![
                        ThreadResponderFrame {
                            agent_id: MAIN_AGENT_ID.to_string(),
                            agent_name: MAIN_AGENT_NAME.to_string(),
                            entered_at: 1,
                            entered_via_handoff_event_id: None,
                            linked_thread_id: None,
                        },
                        ThreadResponderFrame {
                            agent_id: WELES_AGENT_ID.to_string(),
                            agent_name: WELES_AGENT_NAME.to_string(),
                            entered_at: 2,
                            entered_via_handoff_event_id: Some("handoff-1".to_string()),
                            linked_thread_id: Some("dm:svarog:weles".to_string()),
                        },
                    ],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "check this thread",
            &[],
            "check this thread",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            tool_names.contains(&zorai_protocol::tool_names::SPAWN_SUBAGENT),
            "non-participant responder should still see spawn_subagent"
        );
        assert!(
            tool_names.contains(&zorai_protocol::tool_names::MESSAGE_AGENT),
            "non-participant responder should still see message_agent"
        );
    }

    #[tokio::test]
    async fn runner_prioritizes_ask_questions_when_clarification_is_requested() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_clarification_priority";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
                title: "Clarification priority".to_string(),
                messages: vec![AgentMessage::user("help me", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        {
            let mut model = engine.operator_model.write().await;
            model.cognitive_style.message_count = 1;
            model.operator_satisfaction.label = "strained".to_string();
            model.operator_satisfaction.score = 0.18;
            model.implicit_feedback.correction_message_count = 1;
        }

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "help me",
            &[],
            "help me",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();
        let ask_questions_index = tool_names
            .iter()
            .position(|name| *name == zorai_protocol::tool_names::ASK_QUESTIONS)
            .expect("ask_questions should remain available for non-goal clarification");
        let search_files_index = tool_names
            .iter()
            .position(|name| *name == zorai_protocol::tool_names::SEARCH_FILES)
            .expect("search_files should be available");
        assert!(
            ask_questions_index < search_files_index,
            "clarification-priority should move ask_questions ahead of generic unscored tools"
        );
    }

    #[tokio::test]
    async fn durable_goal_task_clarification_priority_does_not_reintroduce_ask_questions() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let task_id = "goal-task-clarification-blacklist";

        engine.goal_runs.lock().await.push_back(GoalRun {
            id: "goal-run-clarification-blacklist".to_string(),
            title: "Goal Inventory".to_string(),
            goal: "Write durable goal artifacts".to_string(),
            client_request_id: None,
            status: GoalRunStatus::Running,
            priority: TaskPriority::Normal,
            created_at: 1,
            updated_at: 1,
            started_at: Some(1),
            completed_at: None,
            thread_id: Some("thread-goal-clarification-blacklist".to_string()),
            root_thread_id: None,
            supervision_thread_id: None,
            active_thread_id: None,
            execution_thread_ids: Vec::new(),
            session_id: Some("session-1".to_string()),
            current_step_index: 0,
            current_step_title: Some("Write plan".to_string()),
            current_step_kind: Some(GoalRunStepKind::Command),
            launch_assignment_snapshot: Vec::new(),
            runtime_assignment_list: Vec::new(),
            planner_owner_profile: None,
            current_step_owner_profile: None,
            step_owner_overrides: std::collections::BTreeMap::new(),
            replan_count: 0,
            max_replans: 2,
            plan_summary: None,
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            step_failure_history: Vec::new(),
            stopped_reason: None,
            child_task_ids: Vec::new(),
            child_task_count: 0,
            approval_count: 0,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            active_task_id: None,
            pending_review_report: None,
            duration_ms: None,
            steps: vec![GoalRunStep {
                id: "goal-step-1".to_string(),
                position: 0,
                title: "Write plan".to_string(),
                instructions: "Write durable goal artifacts".to_string(),
                kind: GoalRunStepKind::Command,
                success_criteria: "plan written".to_string(),
                session_id: Some("session-1".to_string()),
                status: GoalRunStepStatus::InProgress,
                task_id: Some(task_id.to_string()),
                summary: None,
                error: None,
                started_at: Some(1),
                completed_at: None,
            }],
            events: Vec::new(),
            dossier: None,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            model_usage: Vec::new(),
            autonomy_level: AutonomyLevel::Aware,
            authorship_tag: None,
        });

        engine.tasks.lock().await.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Execute goal step".to_string(),
            description: "Write durable goal artifacts".to_string(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: Some("session-1".to_string()),
            goal_run_id: Some("goal-run-clarification-blacklist".to_string()),
            goal_run_title: Some("Goal Inventory".to_string()),
            goal_step_id: Some("goal-step-1".to_string()),
            goal_step_title: Some("Write plan".to_string()),
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: Some(vec![zorai_protocol::tool_names::ASK_QUESTIONS.to_string()]),
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });

        {
            let mut model = engine.operator_model.write().await;
            model.cognitive_style.message_count = 1;
            model.operator_satisfaction.label = "strained".to_string();
            model.operator_satisfaction.score = 0.18;
            model.implicit_feedback.correction_message_count = 1;
        }

        let runner = SendMessageRunner::initialize(
            &engine,
            None,
            "continue goal work",
            &[],
            "continue goal work",
            Some(task_id),
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();
        assert!(
            !tool_names.contains(&zorai_protocol::tool_names::ASK_QUESTIONS),
            "goal task clarification prioritization must not reintroduce ask_questions"
        );
    }

    #[tokio::test]
    async fn runner_prompt_includes_thread_artifact_directories() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-artifacts-1";
        let specs_dir = zorai_protocol::thread_specs_dir(engine.history.data_root(), thread_id);
        let media_dir = zorai_protocol::thread_media_dir(engine.history.data_root(), thread_id);
        let previews_dir =
            zorai_protocol::thread_previews_dir(engine.history.data_root(), thread_id);

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Thread artifacts".to_string(),
                messages: vec![AgentMessage::user("check thread artifacts", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "check thread artifacts",
            &[],
            "check thread artifacts",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        assert!(
            runner
                .system_prompt
                .contains(&format!("{}/", specs_dir.display())),
            "expected specs dir in thread prompt"
        );
        assert!(
            runner
                .system_prompt
                .contains(&format!("{}/", media_dir.display())),
            "expected media dir in thread prompt"
        );
        assert!(
            runner
                .system_prompt
                .contains(&format!("{}/", previews_dir.display())),
            "expected previews dir in thread prompt"
        );
        assert!(specs_dir.is_dir(), "expected specs dir to be created");
        assert!(media_dir.is_dir(), "expected media dir to be created");
        assert!(previews_dir.is_dir(), "expected previews dir to be created");
    }

    #[tokio::test]
    async fn direct_responder_model_override_updates_context_window_to_model_catalog() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4".to_string();
        config.context_window_tokens = 400_000;
        config.providers.insert(
            "alibaba-coding-plan".to_string(),
            ProviderConfig {
                base_url: String::new(),
                model: "qwen3.6-plus".to_string(),
                api_key: "dashscope-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                reasoning_effort: "low".to_string(),
                context_window_tokens: 983_616,
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );
        config.builtin_sub_agents.mokosh.provider = Some("alibaba-coding-plan".to_string());
        config.builtin_sub_agents.mokosh.model = Some("glm-5".to_string());
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let thread_id = "thread_direct_responder_model_override_window";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Mokosh".to_string()),
                title: "Direct responder model override".to_string(),
                messages: vec![AgentMessage::user("check window", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        let mut events = engine.subscribe();
        let runner =
            crate::agent::agent_identity::run_with_agent_scope("mokosh".to_string(), async {
                SendMessageRunner::initialize(
                    &engine,
                    Some(thread_id),
                    "check window",
                    &[],
                    "check window",
                    None,
                    None,
                    None,
                    None,
                    true,
                    true,
                    0,
                )
                .await
            })
            .await
            .expect("runner should initialize");

        assert_eq!(runner.provider_config.model, "glm-5");
        assert_eq!(runner.provider_config.context_window_tokens, 202_752);
        assert_eq!(runner.config.context_window_tokens, 202_752);
        let stored_profile = engine
            .thread_execution_profiles
            .read()
            .await
            .get(thread_id)
            .cloned()
            .expect("runtime profile should be available before streaming starts");
        assert_eq!(
            stored_profile.provider.as_deref(),
            Some("alibaba-coding-plan")
        );
        assert_eq!(stored_profile.model.as_deref(), Some("glm-5"));
        assert_eq!(stored_profile.reasoning_effort.as_deref(), Some("low"));
        assert_eq!(stored_profile.context_window_tokens, Some(202_752));
        let mut received_events = Vec::new();
        let mut saw_profile_reload = false;
        while let Ok(event) = events.try_recv() {
            if matches!(
                &event,
                AgentEvent::ThreadReloadRequired { thread_id: emitted } if emitted == thread_id
            ) {
                saw_profile_reload = true;
            }
            received_events.push(event);
        }
        assert!(
            saw_profile_reload,
            "initializing the responder should notify clients to refresh runtime profile metadata, got {received_events:?}"
        );
    }
}
