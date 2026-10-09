use super::*;
use crate::agent::parse_goal_supervisor_verdict;
pub(crate) async fn execute_list_subagents(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    fn is_descendant_of(task: &AgentTask, ancestor_task_id: &str, all_tasks: &[AgentTask]) -> bool {
        let mut current_parent_id = task.parent_task_id.as_deref();
        while let Some(parent_id) = current_parent_id {
            if parent_id == ancestor_task_id {
                return true;
            }
            current_parent_id = all_tasks
                .iter()
                .find(|candidate| candidate.id == parent_id)
                .and_then(|parent| parent.parent_task_id.as_deref());
        }
        false
    }

    let current_task = if let Some(task_id) = task_id {
        agent
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
    } else {
        None
    };
    let fallback_parent_task_id = current_task.as_ref().and_then(|task| {
        task.parent_task_id
            .clone()
            .or_else(|| Some(task.id.clone()))
    });

    let status_filter = args
        .get("status")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase());
    let parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or(fallback_parent_task_id);
    let parent_thread_id = args
        .get("parent_thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(thread_id.to_string()));
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(20);

    let all_tasks = if let Some(parent_thread_id) = parent_thread_id.as_deref() {
        let direct_parent_thread_status = if parent_task_id.is_none() {
            status_filter.as_deref()
        } else {
            None
        };
        agent
            .list_parent_thread_subagent_tasks(parent_thread_id, direct_parent_thread_status, true)
            .await
    } else {
        agent
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
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
            .await
    };

    let mut subagents = all_tasks
        .clone()
        .into_iter()
        .filter(|task| {
            if task.source != "subagent" {
                return false;
            }
            if let Some(parent_task_id) = parent_task_id.as_deref() {
                return is_descendant_of(task, parent_task_id, &all_tasks);
            }

            parent_thread_id
                .as_deref()
                .map(|value| task.parent_thread_id.as_deref() == Some(value))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    if let Some(status_filter) = status_filter {
        subagents.retain(|task| {
            serde_json::to_value(task.status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .map(|value| value == status_filter)
                .unwrap_or(false)
        });
    }

    subagents.truncate(limit);
    let mut payload = Vec::with_capacity(subagents.len());
    for task in subagents {
        let depth = compute_task_delegation_depth(&task, &all_tasks);
        let max_depth = parse_subagent_containment_scope(task.containment_scope.as_deref())
            .map(|(_, max_depth)| max_depth)
            .unwrap_or_else(|| effective_subagent_max_depth(&task, &all_tasks));
        let metrics = agent
            .history
            .get_subagent_metrics(&task.id)
            .await
            .ok()
            .flatten();
        let tool_call_limit = extract_tool_call_limit(task.termination_conditions.as_deref());

        let tokens_remaining_fraction = match (task.context_budget_tokens, metrics.as_ref()) {
            (Some(max_tokens), Some(metrics)) if max_tokens > 0 => {
                let consumed = metrics.tokens_consumed.max(0) as u64;
                let remaining = max_tokens as u64 - consumed.min(max_tokens as u64);
                Some(remaining as f64 / max_tokens as f64)
            }
            (Some(_), None) => Some(1.0),
            _ => None,
        };
        let time_remaining_fraction = match task.max_duration_secs {
            Some(max_duration_secs) if max_duration_secs > 0 => {
                let started_at = task.started_at.unwrap_or(task.created_at);
                let elapsed_secs = crate::agent::now_millis().saturating_sub(started_at) / 1000;
                let remaining = max_duration_secs.saturating_sub(elapsed_secs);
                Some(remaining as f64 / max_duration_secs as f64)
            }
            _ => None,
        };
        let tool_calls_remaining = match (tool_call_limit, metrics.as_ref()) {
            (Some(limit), Some(metrics)) => {
                let limit: u32 = limit;
                let used = (metrics.tool_calls_total.max(0) as i64).min(u32::MAX as i64) as u32;
                Some::<u32>(limit.saturating_sub(used))
            }
            (Some(limit), None) => Some::<u32>(limit),
            _ => None,
        };

        let mut exhausted_limits = Vec::new();
        if tokens_remaining_fraction.is_some_and(|value| value <= 0.0) {
            exhausted_limits.push("tokens");
        }
        if time_remaining_fraction.is_some_and(|value| value <= 0.0) {
            exhausted_limits.push("time");
        }
        if tool_calls_remaining == Some(0) {
            exhausted_limits.push("tool_calls");
        }
        let budget_exhausted = !exhausted_limits.is_empty();

        let effective_status = if budget_exhausted {
            "budget_exhausted".to_string()
        } else {
            serde_json::to_value(task.status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "unknown".to_string())
        };

        let mut value = serde_json::to_value(&task).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = value.as_object_mut() {
            obj.insert("depth".to_string(), serde_json::json!(depth));
            obj.insert("max_depth".to_string(), serde_json::json!(max_depth));
            obj.insert(
                "effective_status".to_string(),
                serde_json::json!(effective_status),
            );
            obj.insert(
                "budget_remaining".to_string(),
                serde_json::json!({
                    "tokens_pct": tokens_remaining_fraction,
                    "time_pct": time_remaining_fraction,
                    "tool_calls_remaining": tool_calls_remaining,
                }),
            );
            obj.insert(
                "budget_exhausted".to_string(),
                serde_json::json!(budget_exhausted),
            );
            obj.insert(
                "exhausted_limits".to_string(),
                serde_json::json!(exhausted_limits),
            );
        }
        if let Ok(handle) = ensure_task_slug(agent, &task).await {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("handle".to_string(), serde_json::json!(handle));
            }
        }
        payload.push(value);
    }
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_string()))
}

pub(crate) async fn execute_broadcast_contribution(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let explicit_parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let task = if let Some(task_id) = task_id {
        Some(
            task_by_id_for_tool_scope(agent, task_id)
                .await
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?,
        )
    } else {
        None
    };
    let parent_task_id = explicit_parent_task_id
        .or_else(|| task.as_ref().and_then(|task| task.parent_task_id.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "broadcast_contribution requires a current task or explicit parent_task_id"
            )
        })?;
    let contributor_task_id = task
        .as_ref()
        .map(|task| task.id.clone())
        .unwrap_or_else(|| "operator".to_string());
    let topic = args
        .get("topic")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'topic' argument"))?;
    let position = args
        .get("position")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'position' argument"))?;
    let evidence = args
        .get("evidence")
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
    let confidence = args
        .get("confidence")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.6);
    let report = agent
        .record_collaboration_contribution(
            &parent_task_id,
            &contributor_task_id,
            topic,
            position,
            evidence,
            confidence,
        )
        .await?;
    agent
        .record_provenance_event(
            "collaboration_contribution",
            "subagent broadcast a collaboration contribution",
            serde_json::json!({
                "parent_task_id": parent_task_id,
                "task_id": contributor_task_id,
                "topic": topic,
                "position": position,
                "thread_id": thread_id,
            }),
            task.as_ref().and_then(|task| task.goal_run_id.as_deref()),
            task.as_ref().map(|task| task.id.as_str()),
            Some(thread_id),
            None,
            None,
        )
        .await;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_read_peer_memory(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let explicit_parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let task = if let Some(task_id) = task_id {
        Some(
            task_by_id_for_tool_scope(agent, task_id)
                .await
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?,
        )
    } else {
        None
    };
    let parent_task_id = explicit_parent_task_id
        .or_else(|| task.as_ref().and_then(|task| task.parent_task_id.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!("read_peer_memory requires a current task or explicit parent_task_id")
        })?;
    let requester_task_id = task
        .as_ref()
        .map(|task| task.id.as_str())
        .unwrap_or("operator");
    let report = agent
        .collaboration_peer_memory_json(&parent_task_id, requester_task_id)
        .await?;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_vote_on_disagreement(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let task_id =
        task_id.ok_or_else(|| anyhow::anyhow!("vote_on_disagreement requires a current task"))?;
    let task = task_by_id_for_tool_scope(agent, task_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?;
    let parent_task_id = task.parent_task_id.clone().ok_or_else(|| {
        anyhow::anyhow!("vote_on_disagreement is only available inside subagents")
    })?;
    let disagreement_id = args
        .get("disagreement_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'disagreement_id' argument"))?;
    let position = args
        .get("position")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'position' argument"))?;
    let confidence = args.get("confidence").and_then(|value| value.as_f64());
    let report = agent
        .vote_on_collaboration_disagreement(
            &parent_task_id,
            disagreement_id,
            task_id,
            position,
            confidence,
        )
        .await?;
    agent
        .record_provenance_event(
            "collaboration_vote",
            "subagent voted on a disagreement",
            serde_json::json!({
                "parent_task_id": parent_task_id,
                "task_id": task_id,
                "disagreement_id": disagreement_id,
                "position": position,
                "thread_id": thread_id,
            }),
            task.goal_run_id.as_deref(),
            Some(task_id),
            Some(thread_id),
            None,
            None,
        )
        .await;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_dispatch_via_bid_protocol(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'parent_task_id' argument"))?;
    let bids = args
        .get("bids")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing 'bids' argument"))?
        .iter()
        .map(|bid| {
            let task_id = bid
                .get("task_id")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("each bid requires 'task_id'"))?
                .to_string();
            let confidence = bid
                .get("confidence")
                .and_then(|value| value.as_f64())
                .ok_or_else(|| anyhow::anyhow!("each bid requires numeric 'confidence'"))?;
            let availability = match bid
                .get("availability")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref()
            {
                Some("available") => crate::agent::collaboration::BidAvailability::Available,
                Some("busy") => crate::agent::collaboration::BidAvailability::Busy,
                Some("unavailable") => crate::agent::collaboration::BidAvailability::Unavailable,
                _ => anyhow::bail!(
                    "each bid requires availability in [available, busy, unavailable]"
                ),
            };
            Ok(crate::agent::collaboration::DispatchBidRequest {
                task_id,
                confidence,
                availability,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let report = agent
        .dispatch_via_bid_protocol(parent_task_id, &bids)
        .await?;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_list_collaboration_sessions(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let fallback_parent = if let Some(task_id) = task_id {
        task_by_id_for_tool_scope(agent, task_id)
            .await
            .and_then(|task| task.parent_task_id.or_else(|| Some(task.id)))
    } else {
        None
    };
    let parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or(fallback_parent);
    let report = agent
        .collaboration_sessions_json(parent_task_id.as_deref())
        .await?;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "[]".to_string()))
}

pub(crate) async fn execute_enqueue_task(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let description = args
        .get("description")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'description' argument"))?
        .trim()
        .to_string();
    if description.is_empty() {
        anyhow::bail!("'description' must not be empty");
    }

    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let title = args
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_task_title(&description, command.as_deref()));
    let priority = args
        .get("priority")
        .and_then(|value| value.as_str())
        .unwrap_or("normal");
    let session = args
        .get("session")
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
    let scheduled_at = parse_scheduled_at(args)?;

    let task = agent
        .enqueue_task(
            title,
            description,
            priority,
            command,
            session,
            dependencies,
            scheduled_at,
            "agent",
            None,
            None,
            None,
            None,
        )
        .await;

    Ok(serde_json::to_string_pretty(&task).unwrap_or_else(|_| format!("queued task {}", task.id)))
}

pub(crate) async fn execute_start_goal_run(
    args: &serde_json::Value,
    agent: &AgentEngine,
    current_thread_id: &str,
    current_session_id: Option<SessionId>,
) -> Result<String> {
    let goal = args
        .get("goal")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'goal' argument"))?
        .to_string();
    let title = args
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let thread_id = args
        .get("thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(current_thread_id.to_string()));
    let session_id = args
        .get("session_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| current_session_id.map(|value| value.to_string()));
    let priority = args
        .get("priority")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let autonomy_level = args
        .get("autonomy_level")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let requires_approval = args
        .get("requires_approval")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let launch_assignments = parse_goal_launch_assignments(args)?;

    let goal_run = agent
        .start_goal_run_with_surface_and_approval_policy(
            goal,
            title,
            thread_id,
            session_id,
            priority,
            None,
            autonomy_level,
            None,
            requires_approval,
            launch_assignments,
        )
        .await;
    let handle = ensure_goal_slug(
        agent,
        &goal_run.id,
        &goal_run.title,
        goal_run.thread_id.as_deref(),
    )
    .await
    .unwrap_or_else(|_| goal_run.id.clone());
    let mut value = serde_json::to_value(&goal_run).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert("handle".to_string(), serde_json::json!(handle));
    }

    Ok(serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()))
}

fn parse_goal_launch_assignments(
    args: &serde_json::Value,
) -> Result<Option<Vec<crate::agent::types::GoalAgentAssignment>>> {
    let Some(raw) = args.get("launch_assignments") else {
        return Ok(None);
    };
    let assignments = raw
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("'launch_assignments' must be an array"))?;
    if assignments.is_empty() {
        return Ok(None);
    }

    assignments
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let role_id = required_assignment_string(value, index, "role_id")?;
            let provider = required_assignment_string(value, index, "provider")?;
            let model = required_assignment_string(value, index, "model")?;
            let reasoning_effort = value
                .get("reasoning_effort")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let enabled = value
                .get("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            let inherit_from_main = value
                .get("inherit_from_main")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            Ok(crate::agent::types::GoalAgentAssignment {
                role_id,
                enabled,
                provider,
                model,
                reasoning_effort,
                inherit_from_main,
            })
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn required_assignment_string(
    value: &serde_json::Value,
    index: usize,
    field: &str,
) -> Result<String> {
    value
        .get(field)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("launch_assignments[{index}].{field} must be a non-empty string")
        })
}

pub(crate) async fn execute_list_tasks(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let status_filter = args
        .get("status")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize);

    let tasks = agent
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: status_filter,
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
            limit,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await;

    let mut items = Vec::with_capacity(tasks.len());
    for task in tasks {
        let mut value = serde_json::to_value(&task).unwrap_or_else(|_| serde_json::json!({}));
        if let Ok(handle) = ensure_task_slug(agent, &task).await {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("handle".to_string(), serde_json::json!(handle));
            }
        }
        items.push(value);
    }

    Ok(serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string()))
}

pub(crate) async fn execute_list_goal_runs(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(20)
        .clamp(1, 100) as usize;
    let offset = args
        .get("offset")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    let (goal_runs, total) = agent.list_goal_runs_paginated_for_tool(limit, offset).await;
    let mut items = Vec::with_capacity(goal_runs.len());
    for goal_run in goal_runs {
        let handle = ensure_goal_slug(
            agent,
            &goal_run.id,
            &goal_run.title,
            goal_run.thread_id.as_deref(),
        )
        .await
        .unwrap_or_else(|_| goal_run.id.clone());
        let mut value = serde_json::to_value(&goal_run).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = value.as_object_mut() {
            obj.insert("handle".to_string(), serde_json::json!(handle));
        }
        items.push(value);
    }
    let returned = items.len();
    let next_offset = offset.saturating_add(returned);
    let has_more = next_offset < total;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "total": total,
        "limit": limit,
        "offset": offset,
        "returned": returned,
        "has_more": has_more,
        "next_offset": if has_more { Some(next_offset) } else { None },
        "items": items,
    }))
    .unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_request_goal_review(
    args: &serde_json::Value,
    agent: &AgentEngine,
    _thread_id: &str,
    current_task_id: Option<&str>,
) -> Result<String> {
    let report = args
        .get("report")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing non-empty 'report' argument"))?;
    let task_id = current_task_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("request_goal_review can only be called by the goal worker")
        })?;
    let task = task_by_id_for_tool_scope(agent, task_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?;
    let goal_run_id = task
        .goal_run_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("request_goal_review requires an active goal worker"))?;
    if let Some(provided) = args
        .get("goal_run_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let provided = resolve_goal_id(agent, provided, Some(_thread_id)).await?;
        if provided != goal_run_id {
            anyhow::bail!(
                "goal_run_id mismatch: worker is on '{goal_run_id}' but tool received '{provided}'"
            );
        }
    }
    let updated = agent
        .request_goal_review(goal_run_id, task_id, report)
        .await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "ok": true,
        "state": "awaiting_review",
        "goal_run_id": updated.id,
        "message": "Worker is blocked until the owner supervisor verdicts."
    }))
    .unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_submit_goal_review(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    current_task_id: Option<&str>,
) -> Result<String> {
    let goal_run_id = args
        .get("goal_run_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'goal_run_id' argument"))?;
    let resolved_goal_run_id = resolve_goal_id(agent, goal_run_id, Some(thread_id)).await?;
    let goal_run_id = resolved_goal_run_id.as_str();
    let verdict = parse_goal_supervisor_verdict(
        args.get("verdict")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    )?;
    let explanation = args
        .get("explanation")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if let Some(task_id) = current_task_id {
        if let Some(task) = task_by_id_for_tool_scope(agent, task_id).await {
            if task.goal_run_id.as_deref() == Some(goal_run_id) && task.source == "goal_run" {
                anyhow::bail!("submit_goal_review can only be used by the owner supervisor");
            }
        }
    }
    let updated = agent
        .submit_goal_review(goal_run_id, verdict, explanation, Some(thread_id))
        .await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "ok": true,
        "verdict": verdict.as_str(),
        "goal_run_id": updated.id,
        "status": updated.status.as_label(),
    }))
    .unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_list_triggers(
    _args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    agent.ensure_default_event_triggers().await?;
    let payload = agent.list_event_triggers_json().await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_string()))
}

pub(crate) async fn execute_create_routine(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let payload = agent.create_routine_from_args(args).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_list_routines(
    _args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let payload = agent.list_routines_json().await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_string()))
}

pub(crate) async fn execute_get_routine(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let routine_id = args
        .get("routine_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'routine_id' argument"))?;
    let payload = agent.get_routine_json(routine_id).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_preview_routine(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let routine_id = args
        .get("routine_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'routine_id' argument"))?;
    let fire_count = args
        .get("fire_count")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(3);
    let payload = agent.preview_routine_json(routine_id, fire_count).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_update_routine(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let payload = agent.update_routine_from_args(args).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_run_routine_now(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let routine_id = args
        .get("routine_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'routine_id' argument"))?;
    let payload = agent.run_routine_now_json(routine_id).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_list_routine_history(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let routine_id = args
        .get("routine_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'routine_id' argument"))?;
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(10);
    let payload = agent.list_routine_history_json(routine_id, limit).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_rerun_routine(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let run_id = args
        .get("run_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'run_id' argument"))?;
    let payload = agent.rerun_routine_run_json(run_id).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_pause_routine(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let routine_id = args
        .get("routine_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'routine_id' argument"))?;
    let payload = agent.pause_routine_json(routine_id).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_resume_routine(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let routine_id = args
        .get("routine_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'routine_id' argument"))?;
    let payload = agent.resume_routine_json(routine_id).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_delete_routine(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let routine_id = args
        .get("routine_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'routine_id' argument"))?;
    let payload = agent.delete_routine_json(routine_id).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_run_workflow_pack(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let execution = agent
        .run_workflow_pack_json(args, Some(thread_id), task_id)
        .await?;
    Ok((
        serde_json::to_string_pretty(&execution.payload).unwrap_or_else(|_| "{}".to_string()),
        execution.pending_approval,
    ))
}

pub(crate) async fn execute_whatsapp_link_start(
    _args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let started = agent.whatsapp_link.start_if_idle().await?;
    let snapshot = agent.whatsapp_link.status_snapshot().await;
    Ok(serde_json::json!({
        "ok": true,
        "started": started,
        "state": snapshot.state,
        "phone": snapshot.phone,
        "last_error": snapshot.last_error,
    })
    .to_string())
}

pub(crate) async fn execute_whatsapp_link_stop(
    _args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    agent
        .whatsapp_link
        .stop(Some("operator_cancelled".to_string()))
        .await?;
    let snapshot = agent.whatsapp_link.status_snapshot().await;
    Ok(serde_json::json!({
        "ok": true,
        "state": snapshot.state,
        "phone": snapshot.phone,
        "last_error": snapshot.last_error,
    })
    .to_string())
}

pub(crate) async fn execute_whatsapp_link_reset(
    _args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    agent.whatsapp_link.reset().await?;
    crate::agent::clear_persisted_provider_state(
        &agent.history,
        crate::agent::WHATSAPP_LINK_PROVIDER_ID,
    )
    .await?;
    let native_store_path = crate::agent::whatsapp_native_store_path(&agent.data_dir);
    if native_store_path.exists() {
        tokio::fs::remove_file(&native_store_path).await?;
    }
    let snapshot = agent.whatsapp_link.status_snapshot().await;
    Ok(serde_json::json!({
        "ok": true,
        "state": snapshot.state,
        "phone": snapshot.phone,
        "last_error": snapshot.last_error,
        "message": "reset",
    })
    .to_string())
}

pub(crate) async fn execute_whatsapp_link_status(
    _args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let snapshot = agent.whatsapp_link.status_snapshot().await;
    Ok(serde_json::json!({
        "state": snapshot.state,
        "phone": snapshot.phone,
        "last_error": snapshot.last_error,
    })
    .to_string())
}

pub(crate) async fn execute_ingest_webhook_event(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    agent.ensure_default_event_triggers().await?;
    let payload = agent.ingest_webhook_event_json(args).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_add_trigger(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let payload = agent.add_event_trigger_from_args(args).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_list_trigger_fire_history(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let trigger_id = args
        .get("trigger_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let status = args
        .get("status")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(20);

    let rows = agent
        .history
        .list_trigger_fire_history(trigger_id, status, limit)
        .await?;

    let payload: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "trigger_id": row.trigger_id,
                "event_family": row.event_family,
                "event_kind": row.event_kind,
                "status": row.status,
                "fired_at_ms": row.fired_at_ms,
                "completed_at_ms": row.completed_at_ms,
                "retry_count": row.retry_count,
                "error_message": row.error_message,
                "created_task_id": row.created_task_id,
                "notice_id": row.notice_id,
                "payload_json": row.payload_json,
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_string()))
}

pub(crate) async fn execute_get_cost_summary(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let window = args
        .get("window")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let payload = agent.get_cost_summary_json(window).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_list_browser_profiles(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let health_filter = args
        .get("health_state")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let workspace_filter = args
        .get("workspace_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let rows = agent
        .list_browser_profiles_with_current_health_filtered(health_filter, workspace_filter)
        .await?;

    let filtered: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "profile_id": row.profile_id,
                "label": row.label,
                "profile_dir": row.profile_dir,
                "browser_kind": row.browser_kind,
                "workspace_id": row.workspace_id,
                "health_state": row.health_state,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
                "last_used_at": row.last_used_at,
                "last_auth_success_at": row.last_auth_success_at,
                "last_auth_failure_at": row.last_auth_failure_at,
                "last_auth_failure_reason": row.last_auth_failure_reason,
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&filtered).unwrap_or_else(|_| "[]".to_string()))
}

pub(crate) async fn execute_create_browser_profile(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let profile_id = args
        .get("profile_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'profile_id' argument"))?;

    let label = args
        .get("label")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'label' argument"))?;

    let profile_dir = args
        .get("profile_dir")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'profile_dir' argument"))?;

    let browser_kind = args
        .get("browser_kind")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let workspace_id = args
        .get("workspace_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let now = crate::agent::now_millis();
    let profile = crate::agent::types::BrowserProfile {
        profile_id: profile_id.to_string(),
        label: label.to_string(),
        profile_dir: profile_dir.to_string(),
        browser_kind,
        workspace_id,
        health_state: crate::agent::types::BrowserProfileHealth::Healthy,
        created_at: now,
        updated_at: now,
        last_used_at: None,
        last_auth_success_at: None,
        last_auth_failure_at: None,
        last_auth_failure_reason: None,
    };

    agent.history.upsert_browser_profile(&profile).await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "profile_id": profile.profile_id,
        "label": profile.label,
        "profile_dir": profile.profile_dir,
        "browser_kind": profile.browser_kind,
        "workspace_id": profile.workspace_id,
        "health_state": profile.health_state.as_str(),
        "created_at": profile.created_at,
        "updated_at": profile.updated_at,
    }))
    .unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_update_browser_profile_health(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let profile_id = args
        .get("profile_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'profile_id' argument"))?;

    let health_state = args
        .get("health_state")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'health_state' argument"))?;

    let health = crate::agent::types::BrowserProfileHealth::from_str(health_state)
        .ok_or_else(|| anyhow::anyhow!("invalid 'health_state': {health_state}"))?;

    let now = crate::agent::now_millis();

    let last_auth_success_at = args
        .get("last_auth_success_at")
        .and_then(|value| value.as_u64());

    let last_auth_failure_at = args
        .get("last_auth_failure_at")
        .and_then(|value| value.as_u64());

    let last_auth_failure_reason = args
        .get("last_auth_failure_reason")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let mut row = agent
        .history
        .get_browser_profile(profile_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("browser profile not found: {profile_id}"))?;

    row.health_state = health.as_str().to_string();
    row.updated_at = now;
    if last_auth_success_at.is_some() {
        row.last_auth_success_at = last_auth_success_at;
    }
    if last_auth_failure_at.is_some() {
        row.last_auth_failure_at = last_auth_failure_at;
    }
    if last_auth_failure_reason.is_some() {
        row.last_auth_failure_reason = last_auth_failure_reason;
    }

    let profile = crate::agent::types::BrowserProfile {
        profile_id: row.profile_id.clone(),
        label: row.label.clone(),
        profile_dir: row.profile_dir.clone(),
        browser_kind: row.browser_kind.clone(),
        workspace_id: row.workspace_id.clone(),
        health_state: health,
        created_at: row.created_at,
        updated_at: row.updated_at,
        last_used_at: row.last_used_at,
        last_auth_success_at: row.last_auth_success_at,
        last_auth_failure_at: row.last_auth_failure_at,
        last_auth_failure_reason: row.last_auth_failure_reason.clone(),
    };

    agent.history.upsert_browser_profile(&profile).await?;

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "profile_id": profile.profile_id,
        "label": profile.label,
        "health_state": profile.health_state.as_str(),
        "updated_at": profile.updated_at,
        "last_auth_success_at": profile.last_auth_success_at,
        "last_auth_failure_at": profile.last_auth_failure_at,
        "last_auth_failure_reason": profile.last_auth_failure_reason,
    }))
    .unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_show_dreams(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(10);
    let payload = agent.show_dreams_payload(limit).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_show_harness_state(
    args: &serde_json::Value,
    agent: &AgentEngine,
    current_thread_id: &str,
    current_task_id: Option<&str>,
) -> Result<String> {
    let requested_thread_id = args
        .get("thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(current_thread_id.to_string()));
    let requested_task_id = args
        .get("task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| current_task_id.map(ToOwned::to_owned));
    let requested_goal_run_id = args
        .get("goal_run_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(5);

    let resolved_task = if let Some(task_id) = requested_task_id.as_deref() {
        Some(
            agent
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
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?,
        )
    } else {
        None
    };
    let goal_run_id = requested_goal_run_id.or_else(|| {
        resolved_task
            .as_ref()
            .and_then(|task| task.goal_run_id.clone())
    });
    let task_id = resolved_task
        .as_ref()
        .map(|task| task.id.clone())
        .or(requested_task_id);

    let projection = crate::agent::harness::load_harness_state_projection(
        &agent.history,
        requested_thread_id.as_deref(),
        goal_run_id.as_deref(),
        task_id.as_deref(),
    )
    .await?;
    let payload = crate::agent::harness::build_harness_state_payload(
        &projection,
        requested_thread_id.as_deref(),
        goal_run_id.as_deref(),
        task_id.as_deref(),
        limit,
    );
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_show_import_report(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let runtime = args
        .get("runtime")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(20);

    let payload = agent.show_import_report_json(runtime, limit).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_import_external_runtime(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let runtime = args
        .get("runtime")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'runtime' argument"))?;
    let config_path = args
        .get("config_path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let dry_run = args
        .get("dry_run")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let conflict_policy = args
        .get("conflict_policy")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("stage_for_review")
        .parse::<ExternalRuntimeConflictPolicy>()
        .map_err(anyhow::Error::msg)?;

    let payload = agent
        .import_external_runtime_json(runtime, config_path, dry_run, conflict_policy)
        .await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_preview_shadow_run(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let runtime = args
        .get("runtime")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'runtime' argument"))?;

    let payload = agent.preview_shadow_run_json(runtime).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_get_todos(
    args: &serde_json::Value,
    agent: &AgentEngine,
    current_thread_id: &str,
    current_task_id: Option<&str>,
) -> Result<String> {
    let requested_task_id = args
        .get("task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let resolved_task = if let Some(task_id) = requested_task_id.or(current_task_id) {
        Some(
            task_by_id_for_tool_scope(agent, task_id)
                .await
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?,
        )
    } else {
        None
    };
    let thread_id = args
        .get("thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            resolved_task
                .as_ref()
                .and_then(|task| task.thread_id.as_deref())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            let trimmed = current_thread_id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .ok_or_else(|| anyhow::anyhow!("missing 'thread_id' argument"))?;
    let items = agent.get_todos(&thread_id).await;

    Ok(serde_json::json!({
        "thread_id": thread_id,
        "task_id": resolved_task.as_ref().map(|task| task.id.as_str()),
        "goal_run_id": resolved_task.as_ref().and_then(|task| task.goal_run_id.as_deref()),
        "items": items,
    })
    .to_string())
}

pub(crate) fn goal_run_worker_may_cancel_target(caller: &AgentTask, target: &AgentTask) -> bool {
    if caller.source != "goal_run" {
        return true;
    }
    caller.id == target.id || target.parent_task_id.as_deref() == Some(caller.id.as_str())
}

async fn ensure_goal_run_cancel_allowed(
    agent: &AgentEngine,
    caller_task_id: Option<&str>,
    target_task_id: &str,
) -> Result<()> {
    let Some(caller_id) = caller_task_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let Some(caller) = task_by_id_for_tool_scope(agent, caller_id).await else {
        return Ok(());
    };
    if caller.source != "goal_run" {
        return Ok(());
    }
    let Some(target) = task_by_id_for_tool_scope(agent, target_task_id).await else {
        return Ok(());
    };
    if goal_run_worker_may_cancel_target(&caller, &target) {
        return Ok(());
    }
    let mut current = target.parent_task_id.clone();
    while let Some(parent_id) = current {
        if parent_id == caller.id {
            return Ok(());
        }
        current = task_by_id_for_tool_scope(agent, &parent_id)
            .await
            .and_then(|task| task.parent_task_id);
    }
    anyhow::bail!(
        "goal-run workers may only cancel themselves or descendant tasks, not sibling goal-step tasks"
    )
}

pub(crate) async fn execute_cancel_task(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    caller_task_id: Option<&str>,
) -> Result<String> {
    let requested = args
        .get("task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'task_id' argument"))?;
    let task_id = match resolve_child_reference(agent, requested, thread_id, caller_task_id).await {
        Ok(task) => task.id,
        Err(error) if error.to_string().contains("not found") => {
            lookup_operation_id(agent, thread_id, requested)
                .await?
                .unwrap_or_else(|| requested.to_string())
        }
        Err(error) => return Err(error),
    };
    let task_id = task_id.as_str();
    ensure_goal_run_cancel_allowed(agent, caller_task_id, task_id).await?;
    let cancelled = agent.cancel_task(task_id).await;
    if cancelled {
        return Ok(serde_json::json!({
            "task_id": task_id,
            "cancelled": true,
        })
        .to_string());
    }

    if cancel_headless_operation(task_id) {
        return Ok(serde_json::json!({
            "task_id": task_id,
            "cancelled": true,
            "kind": "background_operation",
            "detail": "no task with this id; killed the background operation process instead",
        })
        .to_string());
    }

    if let Ok(Some(status)) = agent
        .session_manager
        .get_background_task_status(task_id)
        .await
    {
        match status.state {
            crate::session_manager::BackgroundTaskState::Queued
            | crate::session_manager::BackgroundTaskState::Running => {
                if agent
                    .session_manager
                    .cancel_queued_managed_command(task_id)
                    .await
                {
                    return Ok(serde_json::json!({
                        "task_id": task_id,
                        "cancelled": true,
                        "kind": "managed_command",
                        "detail": "removed the queued command before it started",
                    })
                    .to_string());
                }
                if let Some(session_id) = status
                    .session_id
                    .as_deref()
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
                {
                    let _ = agent.session_manager.write_input(session_id, &[3]).await;
                    return Ok(serde_json::json!({
                        "task_id": task_id,
                        "cancelled": true,
                        "kind": "managed_command",
                        "detail": "sent interrupt to the terminal session running this command",
                    })
                    .to_string());
                }
            }
            crate::session_manager::BackgroundTaskState::Completed
            | crate::session_manager::BackgroundTaskState::Failed => {
                return Ok(serde_json::json!({
                    "task_id": task_id,
                    "cancelled": false,
                    "kind": "managed_command",
                    "detail": "operation already finished",
                })
                .to_string());
            }
        }
    }

    if let Some(snapshot) = crate::server::operation_registry().snapshot(task_id) {
        let detail = match snapshot.state {
            zorai_protocol::OperationLifecycleState::Completed
            | zorai_protocol::OperationLifecycleState::Failed => "operation already finished",
            _ => "operation exists but its kind does not support cancellation",
        };
        return Ok(serde_json::json!({
            "task_id": task_id,
            "cancelled": false,
            "kind": snapshot.kind,
            "detail": detail,
        })
        .to_string());
    }

    Ok(serde_json::json!({
        "task_id": task_id,
        "cancelled": false,
        "detail": "no task or operation found with this id",
    })
    .to_string())
}

pub(crate) async fn execute_schedule_wakeup(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    if thread_id.trim().is_empty() {
        return Err(anyhow::anyhow!("schedule_wakeup requires an active thread"));
    }
    let delay = args
        .get("delay")
        .and_then(|value| value.as_u64())
        .filter(|value| *value >= 1)
        .ok_or_else(|| anyhow::anyhow!("missing or invalid 'delay' (integer >= 1)"))?;
    let unit = args
        .get("unit")
        .and_then(|value| value.as_str())
        .unwrap_or("minutes")
        .trim()
        .to_ascii_lowercase();
    let unit_ms: u64 = match unit.as_str() {
        "second" | "seconds" | "sec" | "s" => 1_000,
        "minute" | "minutes" | "min" | "m" => 60_000,
        "hour" | "hours" | "hr" | "h" => 3_600_000,
        other => {
            return Err(anyhow::anyhow!(
                "unsupported unit '{other}' (use seconds, minutes, or hours)"
            ))
        }
    };
    let delay_ms = delay.saturating_mul(unit_ms);
    let repetitions = args
        .get("repetitions")
        .and_then(|value| value.as_u64())
        .unwrap_or(1);
    let wakeup_kind = args
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or("generic")
        .trim();
    let resolved_goal_run_id = match args
        .get("goal_run_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(goal_run_id) => Some(resolve_goal_id(agent, goal_run_id, Some(thread_id)).await?),
        None => None,
    };
    let goal_run_id = resolved_goal_run_id.as_deref();
    if wakeup_kind == "goal_supervision" {
        if goal_run_id.is_none() {
            return Err(anyhow::anyhow!("goal supervision requires 'goal_run_id'"));
        }
        if repetitions != 1 {
            return Err(anyhow::anyhow!(
                "goal supervision must schedule exactly one wakeup; let the triggered agent reassess before scheduling another"
            ));
        }
    }
    let message = args
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Scheduled wakeup — continue with what you intended to check.");

    let wakeup = agent
        .schedule_wakeup_with_context(
            thread_id,
            delay_ms,
            repetitions,
            message,
            wakeup_kind,
            goal_run_id,
        )
        .await?;

    let repetitions_value = if repetitions == 0 {
        serde_json::Value::String("infinite".to_string())
    } else {
        serde_json::Value::from(repetitions)
    };
    Ok(serde_json::json!({
        "wakeup_id": wakeup.id,
        "fires_in_ms": delay_ms,
        "next_fire_at_ms": wakeup.next_fire_at,
        "repetitions": repetitions_value,
        "kind": wakeup.wakeup_kind,
        "goal_run_id": wakeup.goal_run_id,
        "note": "Cancel with cancel_wakeup using this wakeup_id. Fires within ~30s of the scheduled time.",
    })
    .to_string())
}

pub(crate) async fn execute_cancel_wakeup(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let wakeup_id = args
        .get("wakeup_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'wakeup_id' argument"))?;
    let cancelled = agent.cancel_wakeup(wakeup_id).await;
    Ok(serde_json::json!({
        "wakeup_id": wakeup_id,
        "cancelled": cancelled,
    })
    .to_string())
}

#[cfg(test)]
mod cancel_scope_tests {
    use super::goal_run_worker_may_cancel_target;
    use crate::agent::types::{AgentTask, TaskPriority, TaskStatus};

    fn task(id: &str, source: &str, parent: Option<&str>) -> AgentTask {
        AgentTask {
            id: id.to_string(),
            title: id.to_string(),
            description: String::new(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: source.to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: parent.map(ToString::to_string),
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
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        }
    }

    #[test]
    fn goal_run_worker_can_cancel_self_and_children_but_not_siblings() {
        let caller = task("step-4", "goal_run", None);
        assert!(goal_run_worker_may_cancel_target(
            &caller,
            &task("step-4", "goal_run", None)
        ));
        assert!(goal_run_worker_may_cancel_target(
            &caller,
            &task("child", "subagent", Some("step-4"))
        ));
        assert!(!goal_run_worker_may_cancel_target(
            &caller,
            &task("step-5", "goal_run", None)
        ));
    }

    #[test]
    fn non_goal_run_callers_are_not_restricted() {
        let caller = task("user-task", "user", None);
        assert!(goal_run_worker_may_cancel_target(
            &caller,
            &task("other", "goal_run", None)
        ));
    }
}
