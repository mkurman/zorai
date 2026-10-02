use super::*;
use crate::agent::operator_model::SatisfactionAdaptationMode;
use crate::agent::types::AgentTask;
use crate::agent::types::TaskStatus;
use crate::agent::SendMessageOutcome;
use crate::history::AgentTaskListQuery;
use std::collections::HashSet;

const MAX_RECURSIVE_SUBAGENT_DEPTH: u8 = 3;
const RECURSIVE_SUBAGENT_BUDGET_CURVE: [f64; 3] = [1.0, 0.6, 0.3];
const DEFAULT_SUBAGENT_MAX_DURATION_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, Default)]
struct RequestedSubagentBudget {
    pub(crate) max_tokens: Option<u32>,
    pub(crate) max_wall_time_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct DerivedSubagentLimits {
    pub(crate) child_depth: u8,
    pub(crate) max_depth: u8,
    pub(crate) context_budget_tokens: Option<u32>,
    pub(crate) max_duration_secs: Option<u64>,
}

fn budget_fraction_for_depth(depth: u8) -> f64 {
    RECURSIVE_SUBAGENT_BUDGET_CURVE
        .get(depth.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(0.1)
}

fn adaptive_subagent_budget_factor(mode: SatisfactionAdaptationMode) -> f64 {
    match mode {
        SatisfactionAdaptationMode::Normal => 1.0,
        SatisfactionAdaptationMode::Tightened => 0.8,
        SatisfactionAdaptationMode::Minimal => 0.6,
    }
}

fn adapt_default_subagent_max_depth(
    mode: SatisfactionAdaptationMode,
    child_depth: u8,
    inherited_max_depth: u8,
) -> u8 {
    match mode {
        SatisfactionAdaptationMode::Normal => inherited_max_depth,
        SatisfactionAdaptationMode::Tightened => {
            inherited_max_depth.min(child_depth.saturating_add(1))
        }
        SatisfactionAdaptationMode::Minimal => inherited_max_depth.min(child_depth),
    }
}

fn adapt_derived_context_budget(
    value: Option<u32>,
    mode: SatisfactionAdaptationMode,
) -> Option<u32> {
    let factor = adaptive_subagent_budget_factor(mode);
    value.map(|current| {
        ((current as f64 * factor).round() as u32)
            .max(256)
            .min(current)
    })
}

fn adapt_derived_duration_budget(
    value: Option<u64>,
    mode: SatisfactionAdaptationMode,
) -> Option<u64> {
    let factor = adaptive_subagent_budget_factor(mode);
    value.map(|current| {
        ((current as f64 * factor).round() as u64)
            .max(30)
            .min(current)
    })
}

pub(super) fn parse_subagent_containment_scope(scope: Option<&str>) -> Option<(u8, u8)> {
    let scope = scope?.trim();
    let payload = scope.strip_prefix("subagent-depth:")?;
    let (depth, max_depth) = payload.split_once('/')?;
    let depth = depth.trim().parse::<u8>().ok()?;
    let max_depth = max_depth.trim().parse::<u8>().ok()?;
    Some((depth, max_depth))
}

fn format_subagent_containment_scope(depth: u8, max_depth: u8) -> String {
    format!("subagent-depth:{depth}/{max_depth}")
}

pub(super) fn compute_task_delegation_depth(task: &AgentTask, all_tasks: &[AgentTask]) -> u8 {
    let mut depth = 0u8;
    let mut current_parent_id = task.parent_task_id.as_deref();
    while let Some(parent_id) = current_parent_id {
        depth = depth.saturating_add(1);
        current_parent_id = all_tasks
            .iter()
            .find(|candidate| candidate.id == parent_id)
            .and_then(|parent| parent.parent_task_id.as_deref());
    }
    depth
}

pub(super) fn effective_subagent_max_depth(task: &AgentTask, all_tasks: &[AgentTask]) -> u8 {
    parse_subagent_containment_scope(task.containment_scope.as_deref())
        .map(|(_, max_depth)| max_depth)
        .unwrap_or_else(|| compute_task_delegation_depth(task, all_tasks).max(1))
}

pub(super) fn extract_tool_call_limit(dsl: Option<&str>) -> Option<u32> {
    let mut remaining = dsl?;
    let mut limit = None::<u32>;
    let marker = "tool_call_count(";
    while let Some(idx) = remaining.find(marker) {
        let after = &remaining[idx + marker.len()..];
        let Some(close_idx) = after.find(')') else {
            break;
        };
        if let Ok(value) = after[..close_idx].trim().parse::<u32>() {
            limit = Some(limit.map_or(value, |current| current.min(value)));
        }
        remaining = &after[close_idx + 1..];
    }
    limit
}

async fn reserve_subagent_thread_id(agent: &AgentEngine) -> String {
    agent.reserve_unique_thread_id().await
}

/// Active (non-terminal) subagent tasks sharing the caller's parent scope:
/// - a spawned subagent caller: siblings with the same parent_task_id or
///   parent_thread_id (excluding the caller itself);
/// - the main agent caller: subagent tasks spawned from this thread.
///
/// These are the only agents a sibling `message_agent` DM may target.
async fn active_sibling_subagent_tasks(
    agent: &AgentEngine,
    caller_task_id: Option<&str>,
    parent_thread_id: &str,
) -> Vec<AgentTask> {
    let caller_task = match caller_task_id {
        Some(id) => find_task_for_spawn(agent, id).await,
        None => None,
    };
    let scope_parent_task_id = caller_task
        .as_ref()
        .and_then(|task| task.parent_task_id.clone());
    let scope_parent_thread_id = caller_task
        .as_ref()
        .and_then(|task| task.parent_thread_id.clone())
        .unwrap_or_else(|| parent_thread_id.to_string());

    let active_statuses = [
        TaskStatus::Queued,
        TaskStatus::InProgress,
        TaskStatus::AwaitingApproval,
        TaskStatus::Blocked,
        TaskStatus::FailedAnalyzing,
        TaskStatus::BudgetExceeded,
    ];
    let tasks = list_subagent_tasks_for_spawn(agent).await;
    tasks
        .into_iter()
        .filter(|task| {
            if Some(task.id.as_str()) == caller_task_id {
                return false;
            }
            if !active_statuses.contains(&task.status) {
                return false;
            }
            match scope_parent_task_id.as_deref() {
                Some(parent_id) => task.parent_task_id.as_deref() == Some(parent_id),
                None => {
                    task.parent_thread_id.as_deref() == Some(scope_parent_thread_id.as_str())
                        || task.thread_id.as_deref() == Some(scope_parent_thread_id.as_str())
                }
            }
        })
        .collect()
}

/// Persona ids already claimed by active sibling tasks in the caller's
/// parent scope — used to guarantee unique persona names per scope so
/// persona-keyed internal DM threads cannot collide across subagents.
async fn active_sibling_persona_ids(siblings: &[AgentTask]) -> Vec<String> {
    siblings
        .iter()
        .filter_map(|task| {
            crate::agent::agent_identity::extract_persona_id(task.override_system_prompt.as_deref())
        })
        .collect()
}

fn is_global_service_target(resolved_target_id: &str) -> bool {
    matches!(
        resolved_target_id,
        crate::agent::agent_identity::MAIN_AGENT_ID
            | crate::agent::agent_identity::CONCIERGE_AGENT_ID
            | crate::agent::agent_identity::WELES_AGENT_ID
    )
}

fn sibling_task_matches_target(
    task: &AgentTask,
    raw_target: &str,
    resolved_target_id: &str,
) -> bool {
    let normalized = raw_target.trim().to_ascii_lowercase();
    if task.id.eq_ignore_ascii_case(raw_target.trim()) {
        return true;
    }
    if task.title.eq_ignore_ascii_case(raw_target.trim()) {
        return true;
    }
    crate::agent::agent_identity::extract_persona_id(task.override_system_prompt.as_deref())
        .map(|persona_id| {
            persona_id.eq_ignore_ascii_case(resolved_target_id)
                || persona_id.eq_ignore_ascii_case(&normalized)
        })
        .unwrap_or(false)
}

async fn subagent_tasks_snapshot(agent: &AgentEngine) -> Vec<AgentTask> {
    list_subagent_tasks_for_spawn(agent).await
}

async fn find_task_for_spawn(agent: &AgentEngine, task_id: &str) -> Option<AgentTask> {
    {
        let tasks = agent.tasks.lock().await;
        if let Some(task) = tasks.iter().find(|task| task.id == task_id).cloned() {
            return Some(task);
        }
    }

    let persisted_task = agent
        .list_tasks_filtered(&AgentTaskListQuery {
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
        .next();
    if persisted_task.is_some() {
        return persisted_task;
    }

    None
}

async fn list_subagent_tasks_for_spawn(agent: &AgentEngine) -> Vec<AgentTask> {
    let mut tasks = agent
        .list_tasks_filtered(&AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: Some("subagent".to_string()),
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await;
    let mut task_ids = tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<HashSet<_>>();
    for task in agent
        .tasks
        .lock()
        .await
        .iter()
        .filter(|task| task.source == "subagent")
    {
        if task_ids.insert(task.id.clone()) {
            tasks.push(task.clone());
        }
    }
    tasks
}

async fn resolve_effective_subagent_provider_config(
    agent: &AgentEngine,
    task: &AgentTask,
) -> Result<crate::agent::types::ProviderConfig> {
    let config = agent.config.read().await.clone();
    if let Some(provider_id) = task.override_provider.as_deref() {
        let mut provider_config = agent.resolve_sub_agent_provider_config(&config, provider_id)?;
        if let Some(model) = task.override_model.as_deref() {
            crate::agent::provider_resolution::apply_provider_model_override(
                provider_id,
                &mut provider_config,
                model,
            );
        }
        crate::agent::provider_resolution::apply_role_transport_override(
            provider_id,
            &mut provider_config,
            task.override_api_transport,
        );
        return Ok(provider_config);
    }

    agent.resolve_provider_config(&config)
}

async fn seed_reserved_subagent_thread(
    agent: &AgentEngine,
    thread_id: &str,
    title: &str,
    target_agent_id: Option<&str>,
    execution_profile: ThreadExecutionProfile,
) {
    let _ = agent
        .get_or_create_thread_with_target(Some(thread_id), title, target_agent_id)
        .await;
    agent
        .set_thread_execution_profile(thread_id, Some(execution_profile))
        .await;
    agent.persist_thread_by_id(thread_id).await;
}

fn parse_requested_subagent_budget(
    args: &serde_json::Value,
) -> Result<Option<RequestedSubagentBudget>> {
    let Some(budget) = args.get("budget") else {
        return Ok(None);
    };
    let Some(budget) = budget.as_object() else {
        anyhow::bail!("'budget' must be an object when provided");
    };
    let max_tokens = budget
        .get("max_tokens")
        .and_then(|value| value.as_u64())
        .map(|value| value.min(u32::MAX as u64) as u32);
    let max_wall_time_secs = budget
        .get("max_wall_time_secs")
        .and_then(|value| value.as_u64());

    Ok(Some(RequestedSubagentBudget {
        max_tokens,
        max_wall_time_secs,
    }))
}

fn derive_subagent_limits(
    current_task: Option<&AgentTask>,
    all_tasks: &[AgentTask],
    requested_max_depth: Option<u8>,
    requested_budget: Option<RequestedSubagentBudget>,
    default_context_window_tokens: u32,
    adaptation_mode: SatisfactionAdaptationMode,
) -> Result<DerivedSubagentLimits> {
    let parent_depth = current_task
        .map(|task| compute_task_delegation_depth(task, all_tasks))
        .unwrap_or(0);
    let parent_max_depth = current_task
        .map(|task| effective_subagent_max_depth(task, all_tasks))
        .unwrap_or(1);
    let child_depth = parent_depth.saturating_add(1);
    if child_depth > MAX_RECURSIVE_SUBAGENT_DEPTH {
        anyhow::bail!(
            "recursive subagent depth limit exceeded: requested depth {} but hard cap is {}",
            child_depth,
            MAX_RECURSIVE_SUBAGENT_DEPTH
        );
    }

    let max_depth = requested_max_depth.unwrap_or_else(|| {
        adapt_default_subagent_max_depth(adaptation_mode, child_depth, parent_max_depth)
    });
    if max_depth == 0 {
        anyhow::bail!("'max_depth' must be at least 1");
    }
    if max_depth > MAX_RECURSIVE_SUBAGENT_DEPTH {
        anyhow::bail!(
            "requested max_depth {} exceeds hard cap {}",
            max_depth,
            MAX_RECURSIVE_SUBAGENT_DEPTH
        );
    }
    if max_depth < child_depth {
        anyhow::bail!(
            "requested max_depth {} is below child delegation depth {}",
            max_depth,
            child_depth
        );
    }
    if current_task.is_some() && max_depth > parent_max_depth {
        anyhow::bail!(
            "requested max_depth {} exceeds parent allowance {}",
            max_depth,
            parent_max_depth
        );
    }

    let fraction = budget_fraction_for_depth(child_depth);
    let derived_context_budget = {
        let base = (default_context_window_tokens as f64 * fraction).round() as u32;
        current_task
            .and_then(|task| task.context_budget_tokens)
            .map(|parent: u32| parent.min(base))
            .or(Some(base.max(256)))
    };
    let derived_max_duration = {
        let base = (DEFAULT_SUBAGENT_MAX_DURATION_SECS as f64 * fraction).round() as u64;
        current_task
            .and_then(|task| task.max_duration_secs)
            .map(|parent: u64| parent.min(base.max(30)))
            .or(Some(base.max(30)))
    };
    let requested_budget = requested_budget.unwrap_or_default();
    if let Some(current_task) = current_task {
        if let (Some(requested), Some(parent)) = (
            requested_budget.max_tokens,
            current_task.context_budget_tokens,
        ) {
            if requested > parent {
                anyhow::bail!(
                    "requested budget.max_tokens {} exceeds parent context budget {}",
                    requested,
                    parent
                );
            }
        }
        if let (Some(requested), Some(parent)) = (
            requested_budget.max_wall_time_secs,
            current_task.max_duration_secs,
        ) {
            if requested > parent {
                anyhow::bail!(
                    "requested budget.max_wall_time_secs {} exceeds parent max_duration_secs {}",
                    requested,
                    parent
                );
            }
        }
    }

    Ok(DerivedSubagentLimits {
        child_depth,
        max_depth,
        context_budget_tokens: requested_budget
            .max_tokens
            .or_else(|| adapt_derived_context_budget(derived_context_budget, adaptation_mode)),
        max_duration_secs: requested_budget
            .max_wall_time_secs
            .or_else(|| adapt_derived_duration_budget(derived_max_duration, adaptation_mode)),
    })
}

pub(crate) async fn execute_spawn_subagent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
) -> Result<String> {
    fn contains_hidden_weles_fields(args: &serde_json::Value) -> bool {
        [
            "weles_internal_scope",
            "weles_tool_name",
            "weles_tool_args",
            "weles_security_level",
            "weles_suspicion_reasons",
        ]
        .iter()
        .any(|key| args.get(key).is_some())
    }

    if contains_hidden_weles_fields(args) {
        anyhow::bail!(
            "daemon-owned WELES governance fields are unavailable to normal spawn_subagent callers"
        );
    }

    if args
        .get("title")
        .and_then(|value| value.as_str())
        .is_some_and(crate::agent::agent_identity::is_lead_only_persona)
        || args
            .get("role")
            .and_then(|value| value.as_str())
            .is_some_and(crate::agent::agent_identity::is_lead_only_persona)
        || args
            .get("agent")
            .and_then(|value| value.as_str())
            .is_some_and(crate::agent::agent_identity::is_lead_only_persona)
        || args
            .get("sub_agent")
            .and_then(|value| value.as_str())
            .is_some_and(crate::agent::agent_identity::is_lead_only_persona)
    {
        anyhow::bail!(crate::agent::agent_identity::LEAD_PERSONA_SPAWN_ERROR);
    }

    let title = args
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'title' argument"))?
        .to_string();
    let description = args
        .get("description")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'description' argument"))?
        .to_string();
    let provider_override = args
        .get("provider")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let model_override = args
        .get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let reasoning_effort_override = args
        .get("reasoning_effort")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if model_override.is_some() && provider_override.is_none() {
        anyhow::bail!(
            "'model' requires an explicit 'provider'. Use `list_providers` first, then `list_models` for the chosen provider."
        );
    }
    let runtime = normalize_task_runtime(args.get("runtime").and_then(|value| value.as_str()))?;
    if runtime != "daemon" {
        let status = agent
            .external_agent_status(&runtime)
            .await
            .ok_or_else(|| anyhow::anyhow!("runtime {runtime} is not configured"))?;
        if !status.available {
            anyhow::bail!("runtime {runtime} is not available on this machine");
        }
    }

    let priority = args
        .get("priority")
        .and_then(|value| value.as_str())
        .unwrap_or("normal");
    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let dependencies = args
        .get("dependencies")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let task_snapshot = if let Some(current_task_id) = task_id {
        find_task_for_spawn(agent, current_task_id).await
    } else {
        None
    };
    let subagent_tasks = list_subagent_tasks_for_spawn(agent).await;
    let inherited_client_surface = match agent.get_thread_client_surface(thread_id).await {
        Some(client_surface) => Some(client_surface),
        None => match task_snapshot
            .as_ref()
            .and_then(|task| task.goal_run_id.as_deref())
        {
            Some(goal_run_id) => agent.get_goal_run_client_surface(goal_run_id).await,
            None => None,
        },
    };
    let requested_max_depth = args
        .get("max_depth")
        .and_then(|value| value.as_u64())
        .map(|value| value.min(u8::MAX as u64) as u8);
    let requested_budget = parse_requested_subagent_budget(args)?;
    let scheduled_at = parse_scheduled_at(args)?;
    let effective_sub_agents = agent.list_sub_agents().await;
    let matched_def = effective_sub_agents
        .iter()
        .find(|sa| sa.enabled && sa.matches_spawn_request(&title))
        .cloned();
    if let Some(def) = matched_def.as_ref() {
        if crate::agent::agent_identity::is_lead_only_persona(&def.id)
            || crate::agent::agent_identity::is_lead_only_persona(&def.name)
            || def
                .role
                .as_deref()
                .is_some_and(crate::agent::agent_identity::is_lead_only_persona)
        {
            anyhow::bail!(crate::agent::agent_identity::LEAD_PERSONA_SPAWN_ERROR);
        }
        if let Some(reason) = def.protected_reason.as_deref() {
            anyhow::bail!(
                "protected sub-agent '{}' is reserved and cannot be spawned via spawn_subagent: {}",
                def.name,
                reason
            );
        }
    }
    // Reject spawning a second active instance of the same sub-agent
    // definition under one parent scope: duplicate persona names collide in
    // persona-keyed internal DM thread routing and split work context.
    if let Some(def) = matched_def.as_ref() {
        let def_scope_id = def
            .id
            .strip_suffix("_builtin")
            .unwrap_or(def.id.as_str())
            .to_ascii_lowercase();
        let siblings = active_sibling_subagent_tasks(agent, task_id, thread_id).await;
        let duplicate = siblings.iter().find(|task| {
            crate::agent::agent_identity::extract_persona_id(task.override_system_prompt.as_deref())
                .map(|persona_id| persona_id.eq_ignore_ascii_case(&def_scope_id))
                .unwrap_or(false)
                || task
                    .sub_agent_def_id
                    .as_deref()
                    .map(|id| id.eq_ignore_ascii_case(&def.id))
                    .unwrap_or(false)
        });
        if let Some(duplicate) = duplicate {
            anyhow::bail!(
                "sub-agent '{}' is already active under this parent scope as task {} (thread {}). Spawn with a different title, wait for it to finish, or message the existing instance directly instead of duplicating it.",
                def.name,
                duplicate.id,
                duplicate.thread_id.as_deref().unwrap_or("(none)")
            );
        }
    }

    let mut chosen_session = args
        .get("session")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let mut allocated_lane_summary = None;
    let mut allocated_lane_session = None;
    if chosen_session.is_none() && scheduled_at.is_none() {
        let default_source_session = task_snapshot
            .as_ref()
            .and_then(|task| task.session_id.as_deref())
            .map(ToOwned::to_owned);
        let lane_request = serde_json::json!({
            "session": default_source_session,
            "cwd": args.get("cwd").and_then(|value| value.as_str()),
            "pane_name": format!("Subagent · {}", title.chars().take(24).collect::<String>()),
        });
        if let Ok(lane) = allocate_terminal_lane(
            &lane_request,
            session_manager,
            preferred_session_id,
            event_tx,
            "Subagent",
        )
        .await
        {
            chosen_session = Some(lane.session_id.to_string());
            allocated_lane_summary = Some(format!(
                "allocated terminal {} in workspace {} as \"{}\"",
                lane.session_id, lane.workspace_id, lane.pane_name
            ));
            allocated_lane_session = Some((lane.session_id, lane.workspace_id));
        }
    }

    let mut subagent = agent
        .enqueue_task(
            title.clone(),
            description,
            priority,
            command,
            chosen_session,
            dependencies,
            scheduled_at,
            "subagent",
            task_snapshot
                .as_ref()
                .and_then(|task| task.goal_run_id.clone()),
            task_id.map(ToOwned::to_owned),
            Some(thread_id.to_string()),
            Some(runtime.clone()),
        )
        .await;

    if let Some((session_id, workspace_id)) = allocated_lane_session {
        agent
            .register_agent_terminal_lease(crate::agent::terminal_leases::AgentTerminalLease::new(
                session_id,
                Some(workspace_id),
                Some(subagent.id.clone()),
                None,
                crate::agent::task_prompt::now_millis(),
            ))
            .await;
    }

    if let Some(provider_id) = provider_override.as_deref() {
        validate_spawn_provider_override(agent, provider_id, model_override.as_deref()).await?;
        subagent.override_provider = Some(provider_id.to_string());
        subagent.override_model = model_override.clone();
    }

    if let Some(def) = matched_def.as_ref() {
        subagent.override_provider = Some(def.provider.clone());
        subagent.override_model = Some(def.model.clone());
        subagent.override_api_transport = def.api_transport;
        subagent.override_system_prompt = def.system_prompt.clone();
        subagent.sub_agent_def_id = Some(def.id.clone());

        if def.tool_whitelist.is_some() {
            subagent.tool_whitelist = def.tool_whitelist.clone();
        }
        if def.tool_blacklist.is_some() {
            subagent.tool_blacklist = def.tool_blacklist.clone();
        }
        if def.context_budget_tokens.is_some() {
            subagent.context_budget_tokens = def.context_budget_tokens;
        }
        if def.max_duration_secs.is_some() {
            subagent.max_duration_secs = def.max_duration_secs;
        }
        if def.supervisor_config.is_some() {
            subagent.supervisor_config = def.supervisor_config.clone();
        }
        crate::agent::task_crud::enforce_goal_task_autonomy_tool_blacklist(&mut subagent);
    }

    let mut effective_provider_config =
        resolve_effective_subagent_provider_config(agent, &subagent).await?;
    if let Some(reasoning_effort) = reasoning_effort_override
        .clone()
        .or_else(|| {
            matched_def
                .as_ref()
                .and_then(|def| def.reasoning_effort.clone())
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        effective_provider_config.reasoning_effort = reasoning_effort;
    }
    if let Some(context_window_tokens) = matched_def
        .as_ref()
        .and_then(|def| def.context_window_tokens)
        .filter(|tokens| *tokens > 0)
    {
        effective_provider_config.context_window_tokens = context_window_tokens;
    }
    if let Some(huggingface_provider) = matched_def
        .as_ref()
        .and_then(|def| def.huggingface_provider.clone())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        effective_provider_config.huggingface_provider = Some(huggingface_provider);
    }
    let adaptation_mode = {
        let model = agent.operator_model.read().await;
        SatisfactionAdaptationMode::from_label(&model.operator_satisfaction.label)
    };
    let derived_limits = derive_subagent_limits(
        task_snapshot.as_ref(),
        &subagent_tasks,
        requested_max_depth,
        requested_budget,
        effective_provider_config.context_window_tokens,
        adaptation_mode,
    )?;

    subagent.containment_scope = Some(format_subagent_containment_scope(
        derived_limits.child_depth,
        derived_limits.max_depth,
    ));
    subagent.context_budget_tokens = derived_limits.context_budget_tokens;
    subagent.max_duration_secs = derived_limits.max_duration_secs;
    if subagent.context_budget_tokens.is_some() {
        subagent.context_overflow_action = Some(crate::agent::types::ContextOverflowAction::Error);
    }

    let persona_prompt = if subagent.sub_agent_def_id.as_deref()
        == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    {
        let scope = subagent
            .override_system_prompt
            .as_deref()
            .and_then(crate::agent::weles_governance::parse_weles_internal_override_payload)
            .map(|(scope, _, _)| scope)
            .unwrap_or_else(|| crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE.to_string());
        crate::agent::agent_identity::build_weles_persona_prompt(&scope)
    } else if let Some(def) = matched_def.as_ref().filter(|def| !def.builtin) {
        crate::agent::agent_identity::build_user_subagent_persona_prompt(def)
    } else if let Some(def) = matched_def.as_ref() {
        let resolved_scope = def.id.strip_suffix("_builtin").unwrap_or(def.id.as_str());
        build_spawned_persona_prompt(resolved_scope)
    } else {
        // Persona names must stay unique within a parent scope: two active
        // siblings sharing a persona made message_agent route sibling DMs into
        // one shared context-free internal-dm thread keyed by persona names.
        let siblings = active_sibling_subagent_tasks(agent, task_id, thread_id).await;
        let taken_persona_ids = active_sibling_persona_ids(&siblings).await;
        let persona = crate::agent::agent_identity::pick_unique_spawned_persona_seed(
            &subagent.id,
            taken_persona_ids.iter().map(String::as_str),
        )
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot assign a unique persona to subagent '{}': all {} worker personas are already active under this parent scope. Reuse an existing subagent (list_subagents) or let one finish before spawning another.",
                subagent.title,
                crate::agent::agent_identity::spawned_persona_count()
            )
        })?;
        build_spawned_persona_prompt(persona)
    };
    subagent.override_system_prompt = Some(match subagent.override_system_prompt.take() {
        Some(existing) if !existing.trim().is_empty() => {
            format!("{persona_prompt}\n\n{existing}")
        }
        _ => persona_prompt.clone(),
    });
    let reserved_thread_id = reserve_subagent_thread_id(agent).await;
    subagent.thread_id = Some(reserved_thread_id.clone());
    let default_provider_id = agent.config.read().await.provider.clone();
    let reserved_thread_provider = subagent
        .override_provider
        .clone()
        .or_else(|| Some(default_provider_id));
    let reserved_thread_profile = ThreadExecutionProfile {
        provider: reserved_thread_provider,
        model: Some(effective_provider_config.model.clone()),
        reasoning_effort: (!effective_provider_config.reasoning_effort.trim().is_empty())
            .then(|| effective_provider_config.reasoning_effort.clone()),
        context_window_tokens: Some(effective_provider_config.context_window_tokens),
    };
    let reserved_thread_target = matched_def
        .as_ref()
        .map(|def| def.id.as_str())
        .map(str::to_string)
        .or_else(|| {
            crate::agent::agent_identity::extract_persona_id(
                subagent.override_system_prompt.as_deref(),
            )
        });
    agent
        .set_thread_identity_from_task(&reserved_thread_id, &subagent)
        .await;
    seed_reserved_subagent_thread(
        agent,
        &reserved_thread_id,
        &title,
        reserved_thread_target.as_deref(),
        reserved_thread_profile,
    )
    .await;
    if let Some(client_surface) = inherited_client_surface {
        agent
            .set_thread_client_surface(&reserved_thread_id, client_surface)
            .await;
    }
    if let Some(parent_task_id) = task_id {
        agent
            .register_subagent_collaboration(parent_task_id, &subagent)
            .await;
    }
    {
        let mut tasks = agent.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|t| t.id == subagent.id) {
            *existing = subagent.clone();
        }
    }
    {
        let mut trusted = agent.trusted_weles_tasks.write().await;
        trusted.insert(subagent.id.clone());
    }
    agent.persist_tasks().await;
    agent.emit_task_update(&subagent, Some("Reserved child thread".into()));

    let lane_suffix = allocated_lane_summary
        .map(|value| format!("\nDedicated lane: {value}"))
        .unwrap_or_default();
    let persona_suffix = extract_persona_name(subagent.override_system_prompt.as_deref())
        .map(|name| format!("\nAssigned persona: {name}"))
        .unwrap_or_default();
    let def_suffix = subagent
        .sub_agent_def_id
        .as_ref()
        .map(|id| format!("\nMatched sub-agent definition: {id}"))
        .unwrap_or_default();
    let depth_suffix = format!(
        "\nDelegation depth: {}/{}",
        derived_limits.child_depth, derived_limits.max_depth
    );
    let budget_suffix = format!(
        "\nBudget: {} output tokens, {}s",
        derived_limits.context_budget_tokens.unwrap_or(0),
        derived_limits.max_duration_secs.unwrap_or(0)
    );
    let thread_suffix = format!("\nReserved thread: {reserved_thread_id}");
    Ok(format!(
        "Spawned subagent {} with runtime {}.{}{}{}{thread_suffix}{budget_suffix}{def_suffix}\nDo not busy-wait on child status. Use `list_subagents` only for occasional snapshots; if no other useful work remains, send a progress update and stop so zorai can resume you when the child reports back.",
        subagent.id, runtime, lane_suffix, persona_suffix, depth_suffix
    ))
}

pub(crate) async fn execute_fetch_authenticated_providers(agent: &AgentEngine) -> Result<String> {
    let authenticated = agent
        .get_provider_auth_states()
        .await
        .into_iter()
        .filter(|state| state.authenticated)
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&authenticated)
        .map_err(|error| anyhow::anyhow!("failed to serialize authenticated providers: {error}"))
}

pub(crate) async fn execute_list_providers(agent: &AgentEngine) -> Result<String> {
    let providers = agent.get_provider_auth_states().await;
    serde_json::to_string_pretty(&providers)
        .map_err(|error| anyhow::anyhow!("failed to serialize providers: {error}"))
}

pub(crate) async fn execute_fetch_provider_models(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let provider_id = args
        .get("provider")
        .or_else(|| args.get("provider_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'provider' argument"))?;

    let provider_config = resolve_authenticated_provider_config(agent, provider_id).await?;
    let models = crate::agent::llm_client::fetch_models(
        provider_id,
        &provider_config.base_url,
        &provider_config.api_key,
        None,
    )
    .await
    .map_err(|error| {
        anyhow::anyhow!(
            "failed to fetch models for provider '{}': {}. Check `list_providers` and try `list_models` again after fixing auth/base URL.",
            provider_id,
            error
        )
    })?;

    serde_json::to_string_pretty(&models)
        .map_err(|error| anyhow::anyhow!("failed to serialize provider models: {error}"))
}

pub(crate) async fn execute_list_models(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    execute_fetch_provider_models(args, agent).await
}

pub(crate) async fn execute_list_agents(agent: &AgentEngine) -> Result<String> {
    let config = agent.get_config().await;
    let mut rows = vec![
        list_agents_target_row(
            zorai_protocol::AGENT_HANDLE_SVAROG,
            MAIN_AGENT_NAME,
            "main",
            serde_json::json!(config.provider),
            serde_json::json!(config.model),
            true,
        ),
        list_agents_target_row(
            CONCIERGE_AGENT_ID,
            CONCIERGE_AGENT_NAME,
            "concierge",
            serde_json::json!(config
                .concierge
                .provider
                .clone()
                .unwrap_or_else(|| config.provider.clone())),
            serde_json::json!(config
                .concierge
                .model
                .clone()
                .unwrap_or_else(|| config.model.clone())),
            true,
        ),
        list_agents_target_row(
            crate::agent::agent_identity::WELES_AGENT_ID,
            crate::agent::agent_identity::WELES_AGENT_NAME,
            "builtin",
            serde_json::json!(config
                .builtin_sub_agents
                .weles
                .provider
                .clone()
                .unwrap_or_else(|| config.provider.clone())),
            serde_json::json!(config
                .builtin_sub_agents
                .weles
                .model
                .clone()
                .unwrap_or_else(|| config.model.clone())),
            true,
        ),
        list_agents_target_row(
            crate::agent::agent_identity::SWAROZYC_AGENT_ID,
            crate::agent::agent_identity::SWAROZYC_AGENT_NAME,
            "builtin",
            serde_json::json!(config.builtin_sub_agents.swarozyc.provider),
            serde_json::json!(config.builtin_sub_agents.swarozyc.model),
            true,
        ),
        list_agents_target_row(
            crate::agent::agent_identity::RADOGOST_AGENT_ID,
            crate::agent::agent_identity::RADOGOST_AGENT_NAME,
            "builtin",
            serde_json::json!(config.builtin_sub_agents.radogost.provider),
            serde_json::json!(config.builtin_sub_agents.radogost.model),
            true,
        ),
        list_agents_target_row(
            crate::agent::agent_identity::DOMOWOJ_AGENT_ID,
            crate::agent::agent_identity::DOMOWOJ_AGENT_NAME,
            "builtin",
            serde_json::json!(config.builtin_sub_agents.domowoj.provider),
            serde_json::json!(config.builtin_sub_agents.domowoj.model),
            true,
        ),
        list_agents_target_row(
            crate::agent::agent_identity::SWIETOWIT_AGENT_ID,
            crate::agent::agent_identity::SWIETOWIT_AGENT_NAME,
            "builtin",
            serde_json::json!(config.builtin_sub_agents.swietowit.provider),
            serde_json::json!(config.builtin_sub_agents.swietowit.model),
            true,
        ),
        list_agents_target_row(
            crate::agent::agent_identity::PERUN_AGENT_ID,
            crate::agent::agent_identity::PERUN_AGENT_NAME,
            "builtin",
            serde_json::json!(config.builtin_sub_agents.perun.provider),
            serde_json::json!(config.builtin_sub_agents.perun.model),
            true,
        ),
        list_agents_target_row(
            crate::agent::agent_identity::MOKOSH_AGENT_ID,
            crate::agent::agent_identity::MOKOSH_AGENT_NAME,
            "builtin",
            serde_json::json!(config.builtin_sub_agents.mokosh.provider),
            serde_json::json!(config.builtin_sub_agents.mokosh.model),
            true,
        ),
        list_agents_target_row(
            crate::agent::agent_identity::DAZHBOG_AGENT_ID,
            crate::agent::agent_identity::DAZHBOG_AGENT_NAME,
            "builtin",
            serde_json::json!(config.builtin_sub_agents.dazhbog.provider),
            serde_json::json!(config.builtin_sub_agents.dazhbog.model),
            true,
        ),
        list_agents_target_row(
            crate::agent::agent_identity::ROD_AGENT_ID,
            crate::agent::agent_identity::ROD_AGENT_NAME,
            "builtin",
            serde_json::json!(config.builtin_sub_agents.rod.provider),
            serde_json::json!(config.builtin_sub_agents.rod.model),
            true,
        ),
    ];

    for sub_agent in agent.list_sub_agents().await {
        if sub_agent.id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID {
            continue;
        }
        let mut row = list_agents_target_row(
            &sub_agent.id,
            &sub_agent.name,
            if sub_agent.builtin {
                "builtin"
            } else {
                "subagent"
            },
            serde_json::json!(sub_agent.provider),
            serde_json::json!(sub_agent.model),
            sub_agent.enabled,
        );
        row["spawnable"] = serde_json::Value::Bool(
            sub_agent.is_spawnable()
                && !crate::agent::agent_identity::is_lead_only_persona(&sub_agent.id)
                && !crate::agent::agent_identity::is_lead_only_persona(&sub_agent.name),
        );
        if let Some(role) = sub_agent
            .role
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            row["role"] = serde_json::Value::String(role.to_string());
        }
        rows.push(row);
    }

    serde_json::to_string_pretty(&rows)
        .map_err(|error| anyhow::anyhow!("failed to serialize agent targets: {error}"))
}

fn list_agents_target_row(
    agent: &str,
    name: &str,
    kind: &str,
    provider: serde_json::Value,
    model: serde_json::Value,
    switchable: bool,
) -> serde_json::Value {
    serde_json::json!({
        "agent": agent,
        "name": name,
        "kind": kind,
        "provider": provider,
        "model": model,
        "switchable": switchable,
        "spawnable": false
    })
}

pub(crate) async fn execute_list_participants(
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let thread_id = thread_id.trim();
    if thread_id.is_empty() {
        anyhow::bail!("list_participants requires a thread context");
    }
    if crate::agent::agent_identity::is_internal_dm_thread(thread_id)
        || crate::agent::agent_identity::is_participant_playground_thread(thread_id)
        || crate::agent::agent_identity::is_goal_run_thread(thread_id)
        || crate::agent::is_internal_handoff_thread(thread_id)
    {
        anyhow::bail!("list_participants is only available on visible operator threads");
    }

    let rows = agent
        .list_thread_participants(thread_id)
        .await
        .into_iter()
        .map(|participant| {
            serde_json::json!({
                "agent": participant.agent_id,
                "name": participant.agent_name,
                "instruction": participant.instruction,
                "status": match participant.status {
                    crate::agent::ThreadParticipantStatus::Active => "active",
                    crate::agent::ThreadParticipantStatus::Inactive => "inactive",
                },
                "created_at": participant.created_at,
                "updated_at": participant.updated_at,
                "deactivated_at": participant.deactivated_at,
                "last_contribution_at": participant.last_contribution_at,
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string_pretty(&rows)
        .map_err(|error| anyhow::anyhow!("failed to serialize thread participants: {error}"))
}

pub(crate) async fn execute_switch_model(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    if current_agent_scope_id() != MAIN_AGENT_ID {
        anyhow::bail!("`switch_model` is only available to svarog");
    }

    let target_agent = args
        .get("agent")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'agent' argument"))?;
    let provider_id = args
        .get("provider")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'provider' argument"))?;
    let model = args
        .get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'model' argument"))?;

    agent
        .switch_agent_provider_model_json(target_agent, provider_id, model)
        .await?;

    Ok(format!(
        "Updated agent '{}' to use provider '{}' with model '{}'.",
        target_agent, provider_id, model
    ))
}

async fn validate_spawn_provider_override(
    agent: &AgentEngine,
    provider_id: &str,
    model_override: Option<&str>,
) -> Result<()> {
    let provider_config = resolve_authenticated_provider_config(agent, provider_id).await?;
    let Some(model_override) = model_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    let models = crate::agent::llm_client::fetch_models(
        provider_id,
        &provider_config.base_url,
        &provider_config.api_key,
        None,
    )
    .await
    .map_err(|error| {
        anyhow::anyhow!(
            "failed to validate model '{}' for provider '{}': {}. Use `list_models` to inspect the provider's available models first.",
            model_override,
            provider_id,
            error
        )
    })?;

    if !models.is_empty() && !models.iter().any(|model| model.id == model_override) {
        anyhow::bail!(
            "model '{}' is not available for authenticated provider '{}'. Use `list_models` to choose one of the returned models.",
            model_override,
            provider_id
        );
    }

    Ok(())
}

async fn resolve_authenticated_provider_config(
    agent: &AgentEngine,
    provider_id: &str,
) -> Result<crate::agent::types::ProviderConfig> {
    let auth_state = agent
        .get_provider_auth_states()
        .await
        .into_iter()
        .find(|state| state.provider_id == provider_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unknown provider '{}'. Use `list_providers` to inspect available authenticated providers.",
                provider_id
            )
        })?;
    if !auth_state.authenticated {
        anyhow::bail!(
            "provider '{}' is not authenticated. Use `list_providers` to inspect which providers are ready before spawning a subagent.",
            provider_id
        );
    }

    let config = agent.get_config().await;
    agent
        .resolve_sub_agent_provider_config(&config, provider_id)
        .map_err(|error| {
            anyhow::anyhow!(
                "failed to resolve provider '{}': {}. Use `list_providers` to verify the provider configuration.",
                provider_id,
                error
            )
        })
}

pub(in crate::agent) async fn finalize_synchronous_weles_review_task(
    agent: &AgentEngine,
    task: &crate::agent::types::AgentTask,
    outcome: Result<&SendMessageOutcome, &anyhow::Error>,
) {
    let now = now_millis();
    let (status, summary) = match outcome {
        Ok(outcome) => (
            crate::agent::types::TaskStatus::Completed,
            agent
                .latest_assistant_message_text(&outcome.thread_id)
                .await
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "WELES governance review completed".to_string()),
        ),
        Err(error) => (
            crate::agent::types::TaskStatus::Failed,
            format!("WELES governance review failed: {error}"),
        ),
    };

    let mut updated = task.clone();
    updated.status = status;
    updated.progress = 100;
    updated.completed_at = Some(now);
    updated.lane_id = None;
    updated.next_retry_at = None;
    updated.blocked_reason = None;
    updated.result = Some(summary.clone());
    if status == crate::agent::types::TaskStatus::Failed {
        updated.error = Some(summary.clone());
        updated.last_error = Some(summary.clone());
    } else {
        updated.error = None;
        updated.last_error = None;
    }
    if let Some(contract) = updated.completion_contract.as_mut() {
        contract.satisfy_requirement(
            crate::agent::types::SUBAGENT_REPORT_REQUIREMENT_DESCRIPTION,
            summary.clone(),
        );
        contract.terminal_status = Some(status);
    }
    updated.logs.push(make_task_log_entry(
        updated.retry_count,
        if status == crate::agent::types::TaskStatus::Completed {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        "weles_governance",
        if status == crate::agent::types::TaskStatus::Completed {
            "synchronous WELES governance review completed"
        } else {
            "synchronous WELES governance review failed"
        },
        Some(summary),
    ));

    {
        let mut tasks = agent.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|entry| entry.id == updated.id) {
            *existing = updated.clone();
        }
    }
    if let Err(error) = agent.history.upsert_agent_task(&updated).await {
        tracing::warn!(task_id = %updated.id, "failed to persist synchronous WELES terminal state: {error}");
    }
    agent.trusted_weles_tasks.write().await.remove(&updated.id);
    agent.persist_tasks().await;
    agent.emit_task_update(
        &updated,
        Some(if status == crate::agent::types::TaskStatus::Completed {
            "WELES governance review completed".to_string()
        } else {
            "WELES governance review failed".to_string()
        }),
    );
}

pub(in crate::agent) async fn spawn_weles_internal_subagent(
    agent: &AgentEngine,
    thread_id: &str,
    parent_task_id: Option<&str>,
    scope: &str,
    tool_name: &str,
    tool_args: &serde_json::Value,
    security_level: SecurityLevel,
    suspicion_reasons: &[String],
) -> Result<crate::agent::types::AgentTask> {
    if !crate::agent::agent_identity::is_weles_internal_scope(scope) {
        anyhow::bail!("invalid WELES internal scope: {scope}");
    }

    let title = "WELES".to_string();
    let description = match scope {
        crate::agent::agent_identity::WELES_VITALITY_SCOPE => {
            "Internal vitality/self-health review".to_string()
        }
        _ => format!("Internal governance review for {tool_name}"),
    };
    let task_snapshot = if let Some(current_task_id) = parent_task_id {
        find_task_for_spawn(agent, current_task_id).await
    } else {
        None
    };
    if scope == crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE
        && matches!(security_level, SecurityLevel::Yolo)
    {
        anyhow::bail!("YOLO mode disables WELES tool-call supervision");
    }
    let effective_sub_agents = agent.list_sub_agents().await;
    let def = effective_sub_agents
        .iter()
        .find(|sa| sa.id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing daemon-owned WELES definition"))?;
    let task_health_signals =
        crate::agent::weles_governance::build_task_health_signals(task_snapshot.as_ref());

    let inspection_context = serde_json::json!({
        "tool_name": tool_name,
        "tool_args": tool_args,
        "security_level": match security_level {
            SecurityLevel::Highest => "highest",
            SecurityLevel::Moderate => "moderate",
            SecurityLevel::Lowest => "lowest",
            SecurityLevel::Yolo => "yolo",
        },
        "suspicion_reasons": suspicion_reasons,
        "task_health_signals": task_health_signals,
    });
    let internal_payload = crate::agent::weles_governance::build_weles_internal_override_payload(
        scope,
        &inspection_context,
    )
    .ok_or_else(|| anyhow::anyhow!("failed to build WELES internal payload"))?;
    let mut subagent = agent
        .enqueue_task(
            title,
            description,
            "high",
            None,
            task_snapshot
                .as_ref()
                .and_then(|task| task.session_id.clone()),
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;
    // Governance is an out-of-band control-plane operation. It must never
    // become a goal child, inherit a goal step, or participate in goal
    // progress/completion accounting. Keep the parent thread only for
    // routing the internal review conversation.
    subagent.goal_run_title = None;
    subagent.goal_step_id = None;
    subagent.goal_step_title = None;
    subagent.parent_task_id = None;
    subagent.parent_thread_id = Some(thread_id.to_string());
    subagent.status = crate::agent::types::TaskStatus::InProgress;
    subagent.started_at = Some(now_millis());
    subagent.notify_on_complete = false;
    subagent.override_provider = Some(def.provider.clone());
    subagent.override_model = Some(def.model.clone());
    subagent.override_api_transport = def.api_transport;
    subagent.sub_agent_def_id = Some(def.id.clone());
    subagent.override_system_prompt = Some(format!(
        "{}\n\n{}",
        crate::agent::agent_identity::build_weles_persona_prompt(scope),
        internal_payload
    ));
    if def.tool_whitelist.is_some() {
        subagent.tool_whitelist = def.tool_whitelist.clone();
    }
    if def.tool_blacklist.is_some() {
        subagent.tool_blacklist = def.tool_blacklist.clone();
    }
    if def.context_budget_tokens.is_some() {
        subagent.context_budget_tokens = def.context_budget_tokens;
    }
    if def.max_duration_secs.is_some() {
        subagent.max_duration_secs = def.max_duration_secs;
    }
    if def.supervisor_config.is_some() {
        subagent.supervisor_config = def.supervisor_config.clone();
    }

    {
        let mut tasks = agent.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|t| t.id == subagent.id) {
            *existing = subagent.clone();
        }
    }
    {
        let mut trusted = agent.trusted_weles_tasks.write().await;
        trusted.insert(subagent.id.clone());
    }
    agent.persist_tasks().await;

    Ok(subagent)
}

pub(crate) async fn execute_route_to_specialist(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let task_description = args
        .get("task_description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'task_description' argument"))?
        .to_string();
    let capability_tags: Vec<String> = args
        .get("capability_tags")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if capability_tags.is_empty() {
        anyhow::bail!("'capability_tags' must be a non-empty array of strings");
    }
    let acceptance_criteria = args
        .get("acceptance_criteria")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("non_empty")
        .to_string();
    let current_depth: u8 = args
        .get("current_depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;

    match agent
        .route_handoff(
            &task_description,
            &capability_tags,
            task_id,
            None,
            thread_id,
            &acceptance_criteria,
            current_depth,
        )
        .await
    {
        Ok(result) => {
            let response = serde_json::json!({
                "status": "dispatched",
                "task_id": result.task_id,
                "specialist_name": result.specialist_name,
                "specialist_profile_id": result.specialist_profile_id,
                "handoff_log_id": result.handoff_log_id,
                "context_bundle_tokens": result.context_bundle_tokens,
                "routing_method": result.routing_method.as_str(),
                "routing_score": result.routing_score,
                "fallback_used": result.fallback_used,
                "routing_rationale": result.routing_rationale,
                "specialization_diagnostics": result.specialization_diagnostics,
            });
            Ok(serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(e) => Ok(format!("Handoff failed: {e}")),
    }
}

pub(crate) async fn execute_run_divergent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let mode = args
        .get("mode")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("divergent");
    if !matches!(mode, "divergent" | "debate") {
        anyhow::bail!("invalid 'mode' argument: {mode}");
    }

    let problem_statement = args
        .get("problem_statement")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'problem_statement' argument"))?
        .to_string();

    let custom_framings = args
        .get("custom_framings")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let label = item.get("label")?.as_str()?.trim().to_string();
                    let prompt = item
                        .get("system_prompt_override")?
                        .as_str()?
                        .trim()
                        .to_string();
                    if label.is_empty() || prompt.is_empty() {
                        return None;
                    }
                    Some(super::super::handoff::divergent::Framing {
                        label,
                        system_prompt_override: prompt,
                        task_id: None,
                        contribution_id: None,
                    })
                })
                .collect::<Vec<_>>()
        })
        .filter(|v| v.len() >= 2);

    let goal_run_id = match task_id {
        Some(task_id) => {
            let tasks = agent.tasks.lock().await;
            tasks
                .iter()
                .find(|task| task.id == task_id)
                .and_then(|task| task.goal_run_id.clone())
        }
        None => None,
    };

    if mode == "debate" {
        return match agent
            .start_debate_session(
                &problem_statement,
                custom_framings,
                thread_id,
                goal_run_id.as_deref(),
            )
            .await
        {
            Ok(session_id) => {
                let response = serde_json::json!({
                    "status": "started",
                    "session_id": session_id,
                    "mode": "debate",
                    "topic": problem_statement,
                    "message": "Debate session started via run_divergent(mode=debate). Use get_debate_session with this session_id to retrieve debate state and verdict."
                });
                Ok(serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{}".to_string()))
            }
            Err(e) => Ok(format!("Debate session failed: {e}")),
        };
    }

    match agent
        .start_divergent_session(
            &problem_statement,
            custom_framings,
            thread_id,
            goal_run_id.as_deref(),
        )
        .await
    {
        Ok(session_id) => {
            let response = serde_json::json!({
                "status": "started",
                "session_id": session_id,
                "mode": "divergent",
                "problem_statement": problem_statement,
                "message": "Divergent session started. Parallel framings are being processed. Use get_divergent_session with this session_id to retrieve progress, tensions, and mediator output."
            });
            Ok(serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(e) => Ok(format!("Divergent session failed: {e}")),
    }
}

pub(crate) async fn execute_get_divergent_session(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'session_id' argument"))?;

    match agent.get_divergent_session(session_id).await {
        Ok(payload) => {
            Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(error) => Ok(format!("Failed to fetch divergent session: {error}")),
    }
}

pub(crate) async fn execute_run_debate(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let topic = args
        .get("topic")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'topic' argument"))?
        .to_string();

    let custom_framings = args
        .get("custom_framings")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let label = item.get("label")?.as_str()?.trim().to_string();
                    let prompt = item
                        .get("system_prompt_override")?
                        .as_str()?
                        .trim()
                        .to_string();
                    if label.is_empty() || prompt.is_empty() {
                        return None;
                    }
                    Some(super::super::handoff::divergent::Framing {
                        label,
                        system_prompt_override: prompt,
                        task_id: None,
                        contribution_id: None,
                    })
                })
                .collect::<Vec<_>>()
        })
        .filter(|v| v.len() >= 2);

    let goal_run_id = match task_id {
        Some(task_id) => {
            let tasks = agent.tasks.lock().await;
            tasks
                .iter()
                .find(|task| task.id == task_id)
                .and_then(|task| task.goal_run_id.clone())
        }
        None => None,
    };

    match agent
        .start_debate_session(&topic, custom_framings, thread_id, goal_run_id.as_deref())
        .await
    {
        Ok(session_id) => {
            let response = serde_json::json!({
                "status": "started",
                "session_id": session_id,
                "topic": topic,
                "message": "Debate session started. Use get_debate_session with this session_id to retrieve the debate state and verdict as it progresses."
            });
            Ok(serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(e) => Ok(format!("Debate session failed: {e}")),
    }
}

pub(crate) async fn execute_get_debate_session(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'session_id' argument"))?;

    match agent.get_debate_session_payload(session_id).await {
        Ok(payload) => {
            Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(error) => Ok(format!("Failed to fetch debate session: {error}")),
    }
}

pub(crate) async fn execute_get_critique_session(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'session_id' argument"))?;

    match agent.get_critique_session_payload(session_id).await {
        Ok(payload) => {
            Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(error) => Ok(format!("Failed to fetch critique session: {error}")),
    }
}

pub(crate) async fn execute_lookup_emergent_protocol(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let token = args
        .get("token")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'token' argument"))?;

    let record_usage = args
        .get("record_usage")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let payload = if record_usage {
        let success = args
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let fallback_reason = args
            .get("fallback_reason")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned);
        let execution_time_ms = args.get("execution_time_ms").and_then(|v| v.as_u64());
        agent
            .record_protocol_registry_usage(
                thread_id,
                token,
                success,
                fallback_reason,
                execution_time_ms,
            )
            .await?
            .map(serde_json::to_value)
            .transpose()?
    } else {
        agent
            .lookup_thread_protocol_registry_entry(thread_id, token)
            .await?
            .map(serde_json::to_value)
            .transpose()?
    };

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "thread_id": thread_id,
        "token": token,
        "entry": payload,
    }))
    .unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_list_emergent_protocol_proposals(
    _args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let payload = agent.list_thread_protocol_proposals(thread_id).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_respond_emergent_protocol_proposal(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let candidate_id = args
        .get("candidate_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'candidate_id' argument"))?;
    let accept = args
        .get("accept")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| anyhow::anyhow!("missing 'accept' argument"))?;

    let payload = agent
        .respond_to_protocol_proposal(thread_id, candidate_id, accept)
        .await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_reload_emergent_protocol_registry(
    _args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let payload = agent.reload_thread_protocol_registry(thread_id).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_decode_emergent_protocol(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let token = args
        .get("token")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'token' argument"))?;
    let current_role = args.get("current_role").and_then(|v| v.as_str());
    let target_role = args.get("target_role").and_then(|v| v.as_str());
    let normalized_pattern = args.get("normalized_pattern").and_then(|v| v.as_str());

    let payload = agent
        .decode_thread_protocol_token(
            thread_id,
            token,
            current_role,
            target_role,
            normalized_pattern,
        )
        .await?
        .map(serde_json::to_value)
        .transpose()?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "thread_id": thread_id,
        "token": token,
        "decode": payload,
    }))
    .unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_get_emergent_protocol_usage_log(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let protocol_id = args
        .get("protocol_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'protocol_id' argument"))?;

    let payload = agent.get_protocol_usage_log_payload(protocol_id).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_append_debate_argument(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'session_id' argument"))?;
    let role = match args
        .get("role")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .ok_or_else(|| anyhow::anyhow!("missing 'role' argument"))?
    {
        "proponent" => crate::agent::debate::types::RoleKind::Proponent,
        "skeptic" => crate::agent::debate::types::RoleKind::Skeptic,
        "synthesizer" => crate::agent::debate::types::RoleKind::Synthesizer,
        other => anyhow::bail!("invalid 'role' argument: {other}"),
    };
    let agent_id = args
        .get("agent_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'agent_id' argument"))?
        .to_string();
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?
        .to_string();
    let evidence_refs = args
        .get("evidence_refs")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let responds_to = args
        .get("responds_to")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);

    let session = agent
        .get_persisted_debate_session(session_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown debate session: {session_id}"))?;
    let argument = crate::agent::debate::types::Argument {
        id: format!("arg_{}", uuid::Uuid::new_v4()),
        round: session.current_round,
        role,
        agent_id,
        content,
        evidence_refs,
        responds_to,
        timestamp_ms: crate::agent::debate::protocol::now_millis(),
    };

    agent.append_debate_argument(session_id, argument).await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "status": "appended",
        "session_id": session_id,
    }))
    .unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_advance_debate_round(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'session_id' argument"))?;

    match agent.advance_debate_round(session_id).await {
        Ok(session) => Ok(serde_json::to_string_pretty(&serde_json::json!({
            "status": "advanced",
            "session_id": session.id,
            "current_round": session.current_round,
            "roles": session.roles,
        }))
        .unwrap_or_else(|_| "{}".to_string())),
        Err(error) => Ok(format!("Failed to advance debate round: {error}")),
    }
}

pub(crate) async fn execute_complete_debate_session(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'session_id' argument"))?;

    match agent.complete_debate_session(session_id).await {
        Ok(payload) => {
            Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(error) => Ok(format!("Failed to complete debate session: {error}")),
    }
}

pub(crate) async fn execute_handoff_thread_agent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<(String, Option<ToolPendingApproval>)> {
    if thread_id.trim().is_empty() {
        anyhow::bail!("handoff_thread_agent requires a thread context");
    }

    let action = args
        .get("action")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'action' argument"))?;
    let reason = args
        .get("reason")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'reason' argument"))?;
    let summary = args
        .get("summary")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'summary' argument"))?;
    let requested_by = match args
        .get("requested_by")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("agent")
    {
        "user" => crate::agent::ThreadHandoffRequestedBy::User,
        "agent" => crate::agent::ThreadHandoffRequestedBy::Agent,
        other => anyhow::bail!("invalid 'requested_by' argument: {other}"),
    };

    let request = crate::agent::build_thread_handoff_activation(
        thread_id,
        action,
        args.get("target_agent_id").and_then(|value| value.as_str()),
        requested_by,
        reason,
        summary,
    )?;

    let requires_approval = matches!(
        (request.kind, request.requested_by),
        (
            crate::agent::ThreadHandoffKind::Push,
            crate::agent::ThreadHandoffRequestedBy::Agent
        )
    ) && !matches!(
        agent.config.read().await.managed_execution.security_level,
        SecurityLevel::Yolo
    );

    if requires_approval {
        let pending_approval = agent.thread_handoff_pending_approval(&request, "medium")?;
        agent
            .queue_thread_handoff_approval(&request, &pending_approval)
            .await?;
        let target_name = request
            .target_agent_id
            .as_deref()
            .map(canonical_agent_name)
            .unwrap_or(MAIN_AGENT_NAME);
        return Ok((
            format!("Queued operator approval to hand off this thread to {target_name}."),
            Some(pending_approval),
        ));
    }

    let event = agent
        .apply_thread_handoff_activation(&request, None)
        .await?;
    Ok((
        format!(
            "Thread handoff complete: {} -> {}.",
            canonical_agent_name(&event.from_agent_id),
            canonical_agent_name(&event.to_agent_id)
        ),
        None,
    ))
}

async fn execute_message_agent_visible_thread_continuation(
    agent: &AgentEngine,
    thread_id: &str,
    sender: &str,
    resolved_target_id: &str,
    resolved_target_name: &str,
    message: &str,
    preferred_session_hint: Option<String>,
) -> Result<serde_json::Value> {
    let payload = agent
        .build_internal_delegate_payload(Some(thread_id), message, true)
        .await;
    let continuation_prompt = agent
        .build_visible_thread_continuation_prompt(thread_id, sender, resolved_target_id, message)
        .await;
    agent
        .enqueue_visible_thread_continuation(
            thread_id,
            crate::agent::DeferredVisibleThreadContinuation {
                agent_id: resolved_target_id.to_string(),
                task_id: None,
                preferred_session_hint: preferred_session_hint.clone(),
                llm_user_content: continuation_prompt,
                queued_at_ms: 0,
                force_compaction: false,
                rerun_participant_observers_after_turn: true,
                internal_delegate_sender: Some(sender.to_string()),
                internal_delegate_message: Some(payload),
            },
        )
        .await;
    Ok(serde_json::json!({
        "target": resolved_target_name,
        "thread_id": crate::agent::agent_identity::internal_dm_thread_id(
            sender,
            resolved_target_id,
        ),
        "response": "Visible-thread continuation queued; internal discussion will run after the current turn finishes.",
        "upstream_message": serde_json::Value::Null,
        "visible_thread_continuation_requested": true,
    }))
}

async fn execute_message_agent_internal_dm(
    agent: &AgentEngine,
    sender: &str,
    resolved_target_id: &str,
    resolved_target_name: &str,
    message: &str,
    preferred_session_hint: Option<&str>,
    originator_thread_id: &str,
    originator_task_id: Option<&str>,
) -> Result<serde_json::Value> {
    let thread_id = agent
        .enqueue_internal_agent_message(
            sender,
            resolved_target_id,
            message,
            preferred_session_hint,
            originator_thread_id,
            originator_task_id,
        )
        .await?;
    Ok(serde_json::json!({
        "target": resolved_target_name,
        "thread_id": thread_id,
        "delivered": true,
        "response": "Internal DM delivered asynchronously. Continue other work; this thread resumes when the recipient replies.",
        "upstream_message": serde_json::Value::Null,
        "visible_thread_continuation_requested": false,
    }))
}

pub(crate) async fn execute_message_agent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    preferred_session_id: Option<SessionId>,
) -> Result<String> {
    let target = args
        .get("target")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'target' argument"))?;
    let message = args
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'message' argument"))?;
    let requested_visible_thread_continuation = args
        .get("request_visible_thread_continuation")
        .and_then(|value| value.as_bool());

    let sender = if let Some(current_task_id) = task_id {
        let task = find_task_for_spawn(agent, current_task_id).await;
        sender_name_for_task(task.as_ref())
    } else {
        canonical_agent_name(&current_agent_scope_id()).to_string()
    };
    let (resolved_target_id, resolved_target_name) =
        agent.resolve_thread_participant_target(target).await?;

    if canonical_agent_id(&sender) == canonical_agent_id(&resolved_target_id) {
        anyhow::bail!("message_agent cannot target the current active responder");
    }

    // Sibling subagent routing: when the caller is a spawned subagent task
    // (or the main agent coordinating its children), a persona-name target
    // must resolve to the ACTIVE sibling task in the same parent scope —
    // never to the global persona registry, which used to route the DM into
    // a context-free internal-dm thread under unrelated provider/model
    // settings while a perfectly good reserved task thread existed.
    let caller_task_source_subagent = if let Some(current_task_id) = task_id {
        let tasks = subagent_tasks_snapshot(agent).await;
        tasks
            .iter()
            .any(|task| task.id == current_task_id && task.source == "subagent")
    } else {
        false
    };
    if caller_task_source_subagent || !is_global_service_target(&resolved_target_id) {
        let siblings = active_sibling_subagent_tasks(agent, task_id, thread_id).await;
        let matches: Vec<&AgentTask> = siblings
            .iter()
            .filter(|task| sibling_task_matches_target(task, target, &resolved_target_id))
            .collect();
        match matches.len() {
            1 => {
                let sibling = matches[0];
                let sibling_thread_id = sibling.thread_id.clone().ok_or_else(|| {
                    anyhow::anyhow!(
                        "sibling subagent task {} has no reserved thread",
                        sibling.id
                    )
                })?;
                let sender_name = crate::agent::agent_identity::canonical_agent_name(&sender);
                let continuation_prompt = format!(
                    "Internal DM from sibling subagent {sender_name} (task context). This is asynchronous mailbox delivery to your task thread, not a new operator request.\n\n{}\n\nIntegrate this into your assigned work and continue. Do not take over the sender's task; reply via message_agent only if the sender explicitly asked for an answer.",
                    message
                );
                let agent_id = agent
                    .agent_scope_id_for_turn(
                        Some(sibling_thread_id.as_str()),
                        Some(sibling.id.as_str()),
                    )
                    .await;
                agent
                    .enqueue_visible_thread_continuation(
                        &sibling_thread_id,
                        crate::agent::DeferredVisibleThreadContinuation {
                            agent_id,
                            task_id: Some(sibling.id.clone()),
                            preferred_session_hint: preferred_session_id
                                .as_ref()
                                .map(|value| value.to_string()),
                            llm_user_content: continuation_prompt,
                            queued_at_ms: 0,
                            force_compaction: false,
                            rerun_participant_observers_after_turn: false,
                            internal_delegate_sender: None,
                            internal_delegate_message: None,
                        },
                    )
                    .await;
                if agent
                    .thread_is_idle_for_subagent_wakeup(&sibling_thread_id)
                    .await
                {
                    let _ = agent.stop_stream(&sibling_thread_id).await;
                }
                if let Err(error) = agent
                    .flush_deferred_visible_thread_continuations(&sibling_thread_id)
                    .await
                {
                    tracing::warn!(
                        thread_id = %sibling_thread_id,
                        sibling_task_id = %sibling.id,
                        %error,
                        "failed to flush sibling continuation after internal DM"
                    );
                }
                return Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "target": resolved_target_name,
                    "sibling_task_id": sibling.id,
                    "thread_id": sibling_thread_id,
                    "delivered": true,
                    "response": "Internal DM delivered to the sibling subagent's dedicated task thread; it will resume there with full work context.",
                    "upstream_message": serde_json::Value::Null,
                    "visible_thread_continuation_requested": false,
                }))
                .unwrap_or_else(|_| "{}".to_string()));
            }
            0 => {
                if caller_task_source_subagent && !is_global_service_target(&resolved_target_id) {
                    anyhow::bail!(
                        "no active sibling subagent named '{}' exists in this parent scope. Spawned subagents may only internal-DM their active siblings, the parent (svarog), the concierge (rarog), or weles. Use report_subagent_outcome, ask_parent, or ask the parent to relay instead of messaging the global persona directly."
                    ,
                        resolved_target_name
                    );
                }
            }
            _ => {
                let candidates = matches
                    .iter()
                    .map(|task| format!("{} ({})", task.id, task.title))
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!(
                    "multiple active sibling subagents match target '{}': {candidates}. Target the specific task id instead.",
                    resolved_target_name
                );
            }
        }
    }
    let visible_operator_thread = !thread_id.trim().is_empty()
        && !crate::agent::agent_identity::is_internal_dm_thread(thread_id)
        && !crate::agent::agent_identity::is_participant_playground_thread(thread_id)
        && !crate::agent::agent_identity::is_goal_run_thread(thread_id)
        && !crate::agent::is_internal_handoff_thread(thread_id);
    if requested_visible_thread_continuation == Some(true) && !visible_operator_thread {
        anyhow::bail!(
            "request_visible_thread_continuation requires a visible operator thread, not an internal thread"
        );
    }
    let defaults_to_visible_thread_continuation = requested_visible_thread_continuation.is_none()
        && visible_operator_thread
        && agent
            .list_thread_participants(thread_id)
            .await
            .iter()
            .any(|participant| {
                participant.status == crate::agent::ThreadParticipantStatus::Active
                    && participant
                        .agent_id
                        .eq_ignore_ascii_case(&resolved_target_id)
            });
    let request_visible_thread_continuation =
        requested_visible_thread_continuation.unwrap_or(defaults_to_visible_thread_continuation);

    let preferred_session_hint = preferred_session_id.as_ref().map(|value| value.to_string());
    let result = if request_visible_thread_continuation {
        Box::pin(execute_message_agent_visible_thread_continuation(
            agent,
            thread_id,
            &sender,
            &resolved_target_id,
            &resolved_target_name,
            message,
            preferred_session_hint.clone(),
        ))
        .await?
    } else {
        Box::pin(execute_message_agent_internal_dm(
            agent,
            &sender,
            &resolved_target_id,
            &resolved_target_name,
            message,
            preferred_session_hint.as_deref(),
            thread_id,
            task_id,
        ))
        .await?
    };
    Ok(serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string()))
}
