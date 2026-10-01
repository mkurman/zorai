use super::*;

pub(crate) fn make_task_log_entry(
    attempt: u32,
    level: TaskLogLevel,
    phase: &str,
    message: &str,
    details: Option<String>,
) -> AgentTaskLogEntry {
    AgentTaskLogEntry {
        id: format!("tasklog_{}", uuid::Uuid::new_v4()),
        timestamp: now_millis(),
        level,
        phase: phase.to_string(),
        message: message.to_string(),
        details,
        attempt,
    }
}

pub(in crate::agent) fn refresh_task_queue_state(
    tasks: &mut VecDeque<AgentTask>,
    now: u64,
    sessions: &[zorai_protocol::SessionInfo],
    config: &AgentConfig,
) -> Vec<AgentTask> {
    const MAX_CONCURRENT_SUBAGENTS_PER_PARENT: usize = 4;
    let max_weles_reviews = crate::agent::config::resolve_weles_max_concurrent_reviews(
        &config.builtin_sub_agents.weles,
    );
    let mut changed = Vec::new();
    for task in tasks.iter_mut() {
        if task.status == TaskStatus::AwaitingApproval && task.awaiting_approval_id.is_none() {
            task.status = TaskStatus::Queued;
            task.started_at = None;
            task.blocked_reason = None;
            task.logs.push(make_task_log_entry(
                task.retry_count,
                TaskLogLevel::Warn,
                "approval",
                "stale approval state cleared; task returned to queue",
                None,
            ));
            changed.push(task.clone());
        }
    }
    let completed: HashSet<String> = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .map(|task| task.id.clone())
        .collect();
    let occupied_lanes = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::AwaitingApproval
            )
        })
        .map(current_task_lane_key)
        .collect::<HashSet<_>>();
    let occupied_workspaces = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::AwaitingApproval
            )
        })
        .filter(|task| task_enforces_workspace_lock(task))
        .filter_map(|task| task_workspace_key(task, sessions))
        .collect::<HashSet<_>>();
    let active_subagents_by_parent = tasks
        .iter()
        .filter(|task| {
            task.status == TaskStatus::InProgress || task.status == TaskStatus::AwaitingApproval
        })
        .filter_map(subagent_parent_key)
        .fold(HashMap::<String, usize>::new(), |mut counts, parent_key| {
            *counts.entry(parent_key).or_insert(0) += 1;
            counts
        });
    let active_child_subagents_by_parent = tasks
        .iter()
        .filter(|task| task.source == "subagent" && !is_task_terminal_status(task.status))
        .filter_map(|task| {
            task.parent_task_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(|value| (value.to_string(), task.id.clone()))
        })
        .fold(
            HashMap::<String, Vec<String>>::new(),
            |mut grouped, (parent_id, child_id)| {
                grouped.entry(parent_id).or_default().push(child_id);
                grouped
            },
        );
    for task in tasks.iter_mut() {
        let unresolved = task
            .dependencies
            .iter()
            .filter(|dependency| !completed.contains(*dependency))
            .cloned()
            .collect::<Vec<_>>();
        let waiting_for_subagents = task
            .blocked_reason
            .as_deref()
            .map(|reason| reason.starts_with("waiting for subagents:"))
            .unwrap_or(false);

        if matches!(task.status, TaskStatus::Queued | TaskStatus::Blocked) {
            if crate::agent::tool_executor::task_is_awaiting_parent(task) {
                continue;
            }
            if let Some(active_children) = active_child_subagents_by_parent.get(&task.id) {
                let reason = format!("waiting for subagents: {}", active_children.join(", "));
                if task.status != TaskStatus::Blocked
                    || task.blocked_reason.as_deref() != Some(reason.as_str())
                {
                    task.status = TaskStatus::Blocked;
                    task.blocked_reason = Some(reason.clone());
                    task.progress = task.progress.max(90);
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "subagent",
                        "parent task waiting for child subagents",
                        Some(reason),
                    ));
                    changed.push(task.clone());
                }
                continue;
            } else if task.status == TaskStatus::Blocked && waiting_for_subagents {
                task.status = TaskStatus::Queued;
                task.blocked_reason = None;
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Info,
                    "subagent",
                    "all child subagents reached terminal state; parent task re-queued",
                    None,
                ));
                changed.push(task.clone());
            }

            if !unresolved.is_empty() {
                let reason = format!("waiting for dependencies: {}", unresolved.join(", "));
                if task.status != TaskStatus::Blocked
                    || task.blocked_reason.as_deref() != Some(reason.as_str())
                {
                    task.status = TaskStatus::Blocked;
                    task.blocked_reason = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "queue",
                        "task blocked on dependencies",
                        Some(reason),
                    ));
                    changed.push(task.clone());
                }
                continue;
            }

            if let Some(scheduled_at) = task.scheduled_at.filter(|deadline| *deadline > now) {
                let reason = describe_scheduled_time(scheduled_at);
                if task.status != TaskStatus::Blocked
                    || task.blocked_reason.as_deref() != Some(reason.as_str())
                {
                    task.status = TaskStatus::Blocked;
                    task.blocked_reason = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "schedule",
                        "task waiting for scheduled time",
                        Some(reason),
                    ));
                    changed.push(task.clone());
                }
                continue;
            }

            let resource_reason =
                if dispatch_lane_for_task(task, &occupied_lanes, max_weles_reviews).is_none() {
                    Some(format!(
                        "waiting for lane availability: {}",
                        task_lane_wait_key(task)
                    ))
                } else if let Some(parent_key) = subagent_parent_key(task) {
                    if active_subagents_by_parent
                        .get(&parent_key)
                        .copied()
                        .unwrap_or(0)
                        >= MAX_CONCURRENT_SUBAGENTS_PER_PARENT
                    {
                        Some(format!(
                            "waiting for subagent slot: {} active children for {}",
                            MAX_CONCURRENT_SUBAGENTS_PER_PARENT, parent_key
                        ))
                    } else {
                        None
                    }
                } else if let Some(workspace_key) = task_workspace_key(task, sessions) {
                    if task_enforces_workspace_lock(task)
                        && occupied_workspaces.contains(&workspace_key)
                    {
                        Some(format!(
                            "waiting for workspace lock: {}",
                            workspace_key.replace("workspace:", "")
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };

            if let Some(reason) = resource_reason {
                if task.status != TaskStatus::Blocked
                    || task.blocked_reason.as_deref() != Some(reason.as_str())
                {
                    task.status = TaskStatus::Blocked;
                    task.blocked_reason = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "queue",
                        "task blocked on lane or workspace availability",
                        Some(reason),
                    ));
                    changed.push(task.clone());
                }
                continue;
            }

            if task.status == TaskStatus::Blocked {
                if !blocked_reason_clears_when_resources_free(task) {
                    continue;
                }
                task.status = TaskStatus::Queued;
                task.blocked_reason = None;
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Info,
                    "queue",
                    "task gate cleared; task returned to queue",
                    None,
                ));
                changed.push(task.clone());
            }
        }

        if task.status == TaskStatus::FailedAnalyzing
            && task
                .next_retry_at
                .map(|deadline| deadline <= now)
                .unwrap_or(true)
        {
            task.status = TaskStatus::Queued;
            task.next_retry_at = None;
            task.blocked_reason = None;
            task.logs.push(make_task_log_entry(
                task.retry_count,
                TaskLogLevel::Info,
                "analysis",
                "retry backoff elapsed; task returned to queue",
                None,
            ));
            changed.push(task.clone());
        }
    }

    changed
}

pub(in crate::agent) fn select_ready_task_indices(
    tasks: &VecDeque<AgentTask>,
    sessions: &[zorai_protocol::SessionInfo],
    goal_run_statuses: &HashMap<String, GoalRunStatus>,
    config: &AgentConfig,
) -> Vec<(usize, String)> {
    const MAX_CONCURRENT_SUBAGENTS_PER_PARENT: usize = 4;
    let max_weles_reviews = crate::agent::config::resolve_weles_max_concurrent_reviews(
        &config.builtin_sub_agents.weles,
    );
    let mut occupied_lanes = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::AwaitingApproval
            )
        })
        .map(current_task_lane_key)
        .collect::<HashSet<_>>();
    let mut occupied_workspaces = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::AwaitingApproval
            )
        })
        .filter(|task| task_enforces_workspace_lock(task))
        .filter_map(|task| task_workspace_key(task, sessions))
        .collect::<HashSet<_>>();
    let mut active_subagents_by_parent = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::AwaitingApproval
            )
        })
        .filter_map(subagent_parent_key)
        .fold(HashMap::<String, usize>::new(), |mut counts, parent_key| {
            *counts.entry(parent_key).or_insert(0) += 1;
            counts
        });

    let mut queued = tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| task.status == TaskStatus::Queued)
        .collect::<Vec<_>>();
    queued.sort_by_key(|(_, task)| (task_priority_rank(task.priority), task.created_at));

    let mut selected = Vec::new();
    for (index, task) in queued {
        if let Some(goal_run_id) = task.goal_run_id.as_deref() {
            match goal_run_statuses.get(goal_run_id) {
                Some(status) if goal_run_status_blocks_dispatch(*status) => continue,
                None => continue,
                Some(_) => {}
            }
        }

        let Some(lane) = dispatch_lane_for_task(task, &occupied_lanes, max_weles_reviews) else {
            continue;
        };
        let workspace = if task_enforces_workspace_lock(task) {
            task_workspace_key(task, sessions)
        } else {
            None
        };
        let parent_key = subagent_parent_key(task);
        if let Some(parent_key) = parent_key.as_deref() {
            if active_subagents_by_parent
                .get(parent_key)
                .copied()
                .unwrap_or(0)
                >= MAX_CONCURRENT_SUBAGENTS_PER_PARENT
            {
                continue;
            }
        }
        let lane_available = occupied_lanes.insert(lane.clone());
        let workspace_available = workspace
            .as_ref()
            .map(|key| occupied_workspaces.insert(key.clone()))
            .unwrap_or(true);
        if lane_available && workspace_available {
            if let Some(parent_key) = parent_key {
                *active_subagents_by_parent.entry(parent_key).or_insert(0) += 1;
            }
            selected.push((index, lane));
            continue;
        }

        if lane_available {
            occupied_lanes.remove(lane.as_str());
        }
    }

    selected
}

pub(in crate::agent) fn task_lane_key(task: &AgentTask) -> String {
    if is_weles_review_task(task) {
        return "weles".to_string();
    }
    // Goal workers and spawned children must not wait on the interactive
    // session lane. Sharing that lane left them queued behind the lead turn.
    if task.source == "goal_run" || subagent_parent_key(task).is_some() {
        return format!("daemon-subagent:{}", task.id);
    }
    if let Some(session_id) = task
        .session_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return format!("session:{session_id}");
    }
    "daemon-main".to_string()
}

pub(in crate::agent) fn current_task_lane_key(task: &AgentTask) -> String {
    task.lane_id.clone().unwrap_or_else(|| task_lane_key(task))
}

fn is_weles_review_task(task: &AgentTask) -> bool {
    task.sub_agent_def_id.as_deref()
        == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
}

fn task_lane_wait_key(task: &AgentTask) -> String {
    if is_weles_review_task(task) {
        "weles".to_string()
    } else {
        task_lane_key(task)
    }
}

fn dispatch_lane_for_task(
    task: &AgentTask,
    occupied_lanes: &HashSet<String>,
    max_weles_reviews: usize,
) -> Option<String> {
    if is_weles_review_task(task) {
        for slot in 0..max_weles_reviews.max(1) {
            let lane = format!("weles:{slot}");
            if !occupied_lanes.contains(&lane) {
                return Some(lane);
            }
        }
        None
    } else {
        let lane = task_lane_key(task);
        if occupied_lanes.contains(&lane) {
            None
        } else {
            Some(lane)
        }
    }
}

pub(in crate::agent) fn is_task_terminal_status(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Completed
            | TaskStatus::BudgetExceeded
            | TaskStatus::Failed
            | TaskStatus::Cancelled
    )
}

fn goal_run_status_blocks_dispatch(status: GoalRunStatus) -> bool {
    status.is_terminal()
        || matches!(
            status,
            GoalRunStatus::AwaitingApproval
                | GoalRunStatus::AwaitingReview
                | GoalRunStatus::Paused
                | GoalRunStatus::Blocked
                | GoalRunStatus::Planning
        )
}

fn blocked_reason_clears_when_resources_free(task: &AgentTask) -> bool {
    match task.blocked_reason.as_deref() {
        Some(reason)
            if reason.starts_with("waiting for lane availability:")
                || reason.starts_with("waiting for workspace lock:")
                || reason.starts_with("waiting for subagent slot:")
                || reason.starts_with("waiting for dependencies:")
                || reason.starts_with("scheduled for ") =>
        {
            true
        }
        Some(reason) if reason.starts_with("completion contract remains open") => {
            task.completion_contract.as_ref().is_some_and(|contract| {
                contract.open_completion_attempts < OPEN_COMPLETION_CONTRACT_ATTEMPT_LIMIT
            })
        }
        _ => false,
    }
}

pub(in crate::agent) fn task_enforces_workspace_lock(task: &AgentTask) -> bool {
    task.parent_task_id.is_none() && task.parent_thread_id.is_none()
}

pub(in crate::agent) fn subagent_parent_key(task: &AgentTask) -> Option<String> {
    task.parent_task_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("task:{value}"))
        .or_else(|| {
            task.parent_thread_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("thread:{value}"))
        })
}

pub(in crate::agent) fn task_workspace_key(
    task: &AgentTask,
    sessions: &[zorai_protocol::SessionInfo],
) -> Option<String> {
    let session_hint = task.session_id.as_deref()?.trim();
    if session_hint.is_empty() {
        return None;
    }

    sessions
        .iter()
        .find(|session| {
            let session_id = session.id.to_string();
            session_id == session_hint || session_id.contains(session_hint)
        })
        .and_then(|session| session.workspace_id.as_ref())
        .map(|workspace_id| format!("workspace:{workspace_id}"))
}

pub(in crate::agent) fn task_priority_rank(priority: TaskPriority) -> u8 {
    match priority {
        TaskPriority::Urgent => 0,
        TaskPriority::High => 1,
        TaskPriority::Normal => 2,
        TaskPriority::Low => 3,
    }
}

pub(in crate::agent) fn compute_task_backoff_ms(base_delay_ms: u64, retry_count: u32) -> u64 {
    let multiplier = 2u64.saturating_pow(retry_count.saturating_sub(1));
    base_delay_ms.saturating_mul(multiplier).min(5 * 60 * 1000)
}

pub(in crate::agent) fn describe_scheduled_time(timestamp_ms: u64) -> String {
    let system_time = std::time::UNIX_EPOCH + std::time::Duration::from_millis(timestamp_ms);
    format!(
        "scheduled for {}",
        humantime::format_rfc3339_seconds(system_time)
    )
}

pub(in crate::agent) fn status_message(task: &AgentTask) -> &'static str {
    match task.status {
        TaskStatus::Queued => "Task queued",
        TaskStatus::InProgress => "Task in progress",
        TaskStatus::AwaitingApproval => "Task awaiting approval",
        TaskStatus::Blocked => "Task blocked",
        TaskStatus::FailedAnalyzing => "Task analyzing failure",
        TaskStatus::BudgetExceeded => "Task budget exceeded",
        TaskStatus::Completed => "Task completed",
        TaskStatus::Failed => "Task failed",
        TaskStatus::Cancelled => "Task cancelled",
    }
}
