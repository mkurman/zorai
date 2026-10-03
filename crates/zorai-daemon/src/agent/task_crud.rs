//! Task and goal run CRUD — create, list, cancel, control operations.

use super::*;

#[path = "task_crud/tasks.rs"]
mod tasks;

pub(crate) const GOAL_AUTONOMY_TOOL_BLACKLIST: &[&str] =
    &[zorai_protocol::tool_names::ASK_QUESTIONS];

pub(crate) fn merge_tool_blacklist(
    existing: Option<Vec<String>>,
    additions: &[&str],
) -> Option<Vec<String>> {
    let mut merged = existing.unwrap_or_default();
    for addition in additions {
        let trimmed = addition.trim();
        if trimmed.is_empty() || merged.iter().any(|entry| entry == trimmed) {
            continue;
        }
        merged.push(trimmed.to_string());
    }
    (!merged.is_empty()).then_some(merged)
}

pub(crate) fn enforce_goal_task_autonomy_tool_blacklist(task: &mut AgentTask) {
    if task.goal_run_id.is_some() {
        task.tool_blacklist =
            merge_tool_blacklist(task.tool_blacklist.take(), GOAL_AUTONOMY_TOOL_BLACKLIST);
    }
}

fn push_unique_string(values: &mut Vec<String>, value: &str) -> bool {
    if value.is_empty() || values.iter().any(|existing| existing == value) {
        return false;
    }
    values.push(value.to_string());
    true
}

fn declared_goal_task_ids(goal_run: &GoalRun) -> Vec<String> {
    let mut task_ids = Vec::new();
    if let Some(task_id) = goal_run.active_task_id.as_deref() {
        push_unique_string(&mut task_ids, task_id);
    }
    for step in &goal_run.steps {
        if let Some(task_id) = step.task_id.as_deref() {
            push_unique_string(&mut task_ids, task_id);
        }
    }
    for task_id in &goal_run.child_task_ids {
        push_unique_string(&mut task_ids, task_id);
    }
    task_ids
}

fn declared_goal_thread_ids(goal_run: &GoalRun) -> Vec<String> {
    let mut thread_ids = Vec::new();
    for thread_id in goal_run
        .thread_id
        .iter()
        .chain(goal_run.root_thread_id.iter())
        .chain(goal_run.active_thread_id.iter())
    {
        push_unique_string(&mut thread_ids, thread_id);
    }
    for thread_id in &goal_run.execution_thread_ids {
        push_unique_string(&mut thread_ids, thread_id);
    }
    thread_ids
}

fn release_task_after_supervised_acknowledgment(
    task: &mut AgentTask,
    ack_id: Option<String>,
) -> bool {
    if task.status != TaskStatus::AwaitingApproval {
        return false;
    }

    task.status = TaskStatus::Queued;
    task.started_at = None;
    task.awaiting_approval_id = None;
    task.blocked_reason = None;
    task.logs.push(make_task_log_entry(
        task.retry_count,
        TaskLogLevel::Info,
        "autonomy_acknowledgment",
        "supervised acknowledgment received; task released to queue",
        ack_id,
    ));
    task.progress = task.progress.max(5);
    true
}

#[derive(Debug, Clone, Copy)]
struct GoalRunDetailWindow {
    loaded_step_start: usize,
    loaded_step_end: usize,
    total_step_count: usize,
    loaded_event_start: usize,
    loaded_event_end: usize,
    total_event_count: usize,
}

fn goal_run_wire_json(goal_run: &GoalRun, window: GoalRunDetailWindow) -> Option<String> {
    let mut value = serde_json::to_value(goal_run).ok()?;
    let object = value.as_object_mut()?;
    object.insert(
        "loaded_step_start".to_string(),
        serde_json::Value::from(window.loaded_step_start),
    );
    object.insert(
        "loaded_step_end".to_string(),
        serde_json::Value::from(window.loaded_step_end),
    );
    object.insert(
        "total_step_count".to_string(),
        serde_json::Value::from(window.total_step_count),
    );
    object.insert(
        "loaded_event_start".to_string(),
        serde_json::Value::from(window.loaded_event_start),
    );
    object.insert(
        "loaded_event_end".to_string(),
        serde_json::Value::from(window.loaded_event_end),
    );
    object.insert(
        "total_event_count".to_string(),
        serde_json::Value::from(window.total_event_count),
    );
    serde_json::to_string(&Some(value)).ok()
}

fn goal_run_detail_frame_fits_ipc_with_window(
    goal_run: &GoalRun,
    window: GoalRunDetailWindow,
) -> bool {
    let Some(goal_run_json) = goal_run_wire_json(goal_run, window) else {
        return false;
    };
    zorai_protocol::daemon_message_fits_ipc(&zorai_protocol::DaemonMessage::AgentGoalRunDetail {
        goal_run_json,
    })
}

fn task_detail_frame_fits_ipc(task: &AgentTask) -> bool {
    let Ok(task_json) = serde_json::to_string(task) else {
        return false;
    };

    zorai_protocol::daemon_message_fits_ipc(&zorai_protocol::DaemonMessage::AgentTaskEnqueued {
        task_json,
    })
}

fn task_list_frame_fits_ipc(tasks: &[AgentTask]) -> bool {
    let Ok(tasks_json) = serde_json::to_string(tasks) else {
        return false;
    };

    zorai_protocol::daemon_message_fits_ipc(&zorai_protocol::DaemonMessage::AgentTaskList {
        tasks_json,
    })
}

fn goal_run_list_frame_fits_ipc(goal_runs: &[GoalRun]) -> bool {
    let Ok(goal_runs_json) = serde_json::to_string(goal_runs) else {
        return false;
    };

    zorai_protocol::daemon_message_fits_ipc(&zorai_protocol::DaemonMessage::AgentGoalRunList {
        goal_runs_json,
    })
}

fn todo_detail_frame_fits_ipc(thread_id: &str, todos: &[TodoItem]) -> bool {
    let Ok(todos_json) = serde_json::to_string(todos) else {
        return false;
    };

    zorai_protocol::daemon_message_fits_ipc(&zorai_protocol::DaemonMessage::AgentTodoDetail {
        thread_id: thread_id.to_string(),
        todos_json,
    })
}

fn todo_list_frame_fits_ipc(todos_by_thread: &HashMap<String, Vec<TodoItem>>) -> bool {
    let Ok(todos_json) = serde_json::to_string(todos_by_thread) else {
        return false;
    };

    zorai_protocol::daemon_message_fits_ipc(&zorai_protocol::DaemonMessage::AgentTodoList {
        todos_json,
    })
}

fn work_context_detail_frame_fits_ipc(thread_id: &str, context: &ThreadWorkContext) -> bool {
    let Ok(context_json) = serde_json::to_string(context) else {
        return false;
    };

    zorai_protocol::daemon_message_fits_ipc(
        &zorai_protocol::DaemonMessage::AgentWorkContextDetail {
            thread_id: thread_id.to_string(),
            context_json,
        },
    )
}

fn cap_todos_for_ipc(thread_id: &str, todos: Vec<TodoItem>) -> (Vec<TodoItem>, bool) {
    if todo_detail_frame_fits_ipc(thread_id, &todos) {
        return (todos, false);
    }

    let mut low = 0usize;
    let mut high = todos.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if todo_detail_frame_fits_ipc(thread_id, &todos[mid..]) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    (todos.into_iter().skip(low).collect(), true)
}

fn cap_todo_list_for_ipc(
    todos_by_thread: HashMap<String, Vec<TodoItem>>,
) -> (HashMap<String, Vec<TodoItem>>, bool) {
    let mut truncated = false;
    let mut entries = Vec::with_capacity(todos_by_thread.len());
    for (thread_id, todos) in todos_by_thread {
        let (todos, todos_truncated) = cap_todos_for_ipc(&thread_id, todos);
        truncated |= todos_truncated;
        entries.push((thread_id, todos));
    }

    entries.sort_by(|(a_thread_id, a_todos), (b_thread_id, b_todos)| {
        let a_updated = a_todos
            .iter()
            .map(|todo| todo.updated_at.max(todo.created_at))
            .max()
            .unwrap_or(0);
        let b_updated = b_todos
            .iter()
            .map(|todo| todo.updated_at.max(todo.created_at))
            .max()
            .unwrap_or(0);
        b_updated
            .cmp(&a_updated)
            .then_with(|| a_thread_id.cmp(b_thread_id))
    });

    let to_map = |slice: &[(String, Vec<TodoItem>)]| {
        slice
            .iter()
            .cloned()
            .collect::<HashMap<String, Vec<TodoItem>>>()
    };

    let candidate = to_map(&entries);
    if todo_list_frame_fits_ipc(&candidate) {
        return (candidate, truncated);
    }

    let mut low = 0usize;
    let mut high = entries.len();
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        let candidate = to_map(&entries[..mid]);
        if todo_list_frame_fits_ipc(&candidate) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    (to_map(&entries[..low]), true)
}

fn cap_work_context_for_ipc(
    thread_id: &str,
    context: ThreadWorkContext,
) -> (ThreadWorkContext, bool) {
    if work_context_detail_frame_fits_ipc(thread_id, &context) {
        return (context, false);
    }

    let entries = context.entries;
    let mut low = 0usize;
    let mut high = entries.len();
    while low < high {
        let mid = low + (high - low) / 2;
        let candidate = ThreadWorkContext {
            thread_id: thread_id.to_string(),
            entries: entries[mid..].to_vec(),
        };
        if work_context_detail_frame_fits_ipc(thread_id, &candidate) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    (
        ThreadWorkContext {
            thread_id: thread_id.to_string(),
            entries: entries.into_iter().skip(low).collect(),
        },
        true,
    )
}

fn task_with_log_slice(task: &AgentTask, start_idx: usize) -> AgentTask {
    let mut candidate = task.clone();
    if start_idx >= candidate.logs.len() {
        candidate.logs.clear();
        return candidate;
    }
    candidate.logs = candidate.logs[start_idx..].to_vec();
    candidate
}

fn cap_task_for_ipc(task: AgentTask) -> Option<(AgentTask, bool)> {
    if task_detail_frame_fits_ipc(&task) {
        return Some((task, false));
    }

    let mut low = 0usize;
    let mut high = task.logs.len();
    while low < high {
        let mid = low + (high - low) / 2;
        let candidate = task_with_log_slice(&task, mid);
        if task_detail_frame_fits_ipc(&candidate) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    let candidate = task_with_log_slice(&task, low);
    if task_detail_frame_fits_ipc(&candidate) {
        return Some((candidate, low > 0));
    }

    None
}

fn cap_task_list_for_ipc(tasks: Vec<AgentTask>) -> (Vec<AgentTask>, bool) {
    if task_list_frame_fits_ipc(&tasks) {
        return (tasks, false);
    }

    let mut truncated = false;
    let mut capped = Vec::with_capacity(tasks.len());
    for task in tasks {
        if let Some((task, task_truncated)) = cap_task_for_ipc(task) {
            truncated |= task_truncated;
            capped.push(task);
        } else {
            truncated = true;
        }
    }

    if task_list_frame_fits_ipc(&capped) {
        return (capped, truncated);
    }

    let mut low = 0usize;
    let mut high = capped.len();
    while low < high {
        let mid = low + (high - low + 1) / 2;
        if task_list_frame_fits_ipc(&capped[..mid]) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    (capped.into_iter().take(low).collect(), true)
}

fn goal_run_with_step_slice(goal_run: &GoalRun, start_idx: usize) -> GoalRun {
    let mut candidate = goal_run.clone();
    let current_step_represented = goal_run.current_step_index >= start_idx;
    if start_idx >= candidate.steps.len() {
        candidate.steps.clear();
        candidate.current_step_index = 0;
        candidate.current_step_title = None;
        candidate.current_step_kind = None;
        candidate.active_task_id = None;
        candidate.current_step_owner_profile = None;
        return candidate;
    }

    candidate.steps = candidate.steps[start_idx..].to_vec();
    let current_idx = goal_run
        .current_step_index
        .saturating_sub(start_idx)
        .min(candidate.steps.len().saturating_sub(1));
    candidate.current_step_index = current_idx;
    candidate.current_step_title = candidate
        .steps
        .get(current_idx)
        .map(|step| step.title.clone());
    candidate.current_step_kind = candidate
        .steps
        .get(current_idx)
        .map(|step| step.kind.clone());
    candidate.active_task_id = candidate
        .steps
        .get(current_idx)
        .and_then(|step| step.task_id.clone());
    if !current_step_represented {
        candidate.current_step_owner_profile = None;
    }
    candidate
}

fn goal_run_with_window(
    goal_run: &GoalRun,
    step_start: usize,
    step_end: usize,
    event_start: usize,
    event_end: usize,
) -> (GoalRun, GoalRunDetailWindow) {
    let mut candidate = goal_run.clone();
    let current_step_represented =
        goal_run.current_step_index >= step_start && goal_run.current_step_index < step_end;

    let step_start = step_start.min(goal_run.steps.len());
    let step_end = step_end.clamp(step_start, goal_run.steps.len());
    candidate.steps = goal_run.steps[step_start..step_end].to_vec();
    if candidate.steps.is_empty() {
        candidate.current_step_index = 0;
        candidate.current_step_title = None;
        candidate.current_step_kind = None;
        candidate.active_task_id = None;
        candidate.current_step_owner_profile = None;
    } else {
        let current_idx = goal_run
            .current_step_index
            .saturating_sub(step_start)
            .min(candidate.steps.len().saturating_sub(1));
        candidate.current_step_index = current_idx;
        candidate.current_step_title = candidate
            .steps
            .get(current_idx)
            .map(|step| step.title.clone());
        candidate.current_step_kind = candidate
            .steps
            .get(current_idx)
            .map(|step| step.kind.clone());
        candidate.active_task_id = candidate
            .steps
            .get(current_idx)
            .and_then(|step| step.task_id.clone());
        if !current_step_represented {
            candidate.current_step_owner_profile = None;
        }
    }

    let event_start = event_start.min(goal_run.events.len());
    let event_end = event_end.clamp(event_start, goal_run.events.len());
    candidate.events = goal_run.events[event_start..event_end].to_vec();

    (
        candidate,
        GoalRunDetailWindow {
            loaded_step_start: step_start,
            loaded_step_end: step_end,
            total_step_count: goal_run.steps.len(),
            loaded_event_start: event_start,
            loaded_event_end: event_end,
            total_event_count: goal_run.events.len(),
        },
    )
}

fn goal_run_stripped_summary(goal_run: &GoalRun) -> GoalRun {
    let mut candidate = goal_run.clone();
    candidate.events.clear();
    candidate.steps.clear();
    candidate.current_step_index = 0;
    candidate.current_step_title = None;
    candidate.current_step_kind = None;
    candidate.active_task_id = None;
    candidate.current_step_owner_profile = None;
    candidate.plan_summary = None;
    candidate.reflection_summary = None;
    candidate.memory_updates.clear();
    candidate.last_error = None;
    candidate.failure_cause = None;
    candidate
}

fn cap_goal_run_for_ipc(goal_run: GoalRun) -> Option<(GoalRun, GoalRunDetailWindow, bool)> {
    let full_window = GoalRunDetailWindow {
        loaded_step_start: 0,
        loaded_step_end: goal_run.steps.len(),
        total_step_count: goal_run.steps.len(),
        loaded_event_start: 0,
        loaded_event_end: goal_run.events.len(),
        total_event_count: goal_run.events.len(),
    };
    if goal_run_detail_frame_fits_ipc_with_window(&goal_run, full_window) {
        return Some((goal_run, full_window, false));
    }

    let mut candidate = goal_run.clone();
    let mut window = full_window;
    if !candidate.events.is_empty() {
        let mut low = 0usize;
        let mut high = candidate.events.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let mut trial = candidate.clone();
            trial.events = trial.events[mid..].to_vec();
            let trial_window = GoalRunDetailWindow {
                loaded_event_start: mid,
                ..window
            };
            if goal_run_detail_frame_fits_ipc_with_window(&trial, trial_window) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        candidate.events = candidate.events[low..].to_vec();
        window.loaded_event_start = low;
    }
    if goal_run_detail_frame_fits_ipc_with_window(&candidate, window) {
        return Some((candidate, window, true));
    }

    if !candidate.steps.is_empty() {
        let max_prefix_drop = goal_run
            .current_step_index
            .min(goal_run.steps.len().saturating_sub(1));
        let mut low = 0usize;
        let mut high = max_prefix_drop;
        while low < high {
            let mid = low + (high - low) / 2;
            let trial = goal_run_with_step_slice(&candidate, mid);
            let trial_window = GoalRunDetailWindow {
                loaded_step_start: mid,
                ..window
            };
            if goal_run_detail_frame_fits_ipc_with_window(&trial, trial_window) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }

        candidate = goal_run_with_step_slice(&candidate, low);
        window.loaded_step_start = low;
        if goal_run_detail_frame_fits_ipc_with_window(&candidate, window) {
            return Some((candidate, window, true));
        }

        candidate = goal_run_with_step_slice(
            &candidate,
            candidate
                .current_step_index
                .min(candidate.steps.len().saturating_sub(1)),
        );
        window.loaded_step_start = goal_run
            .current_step_index
            .min(goal_run.steps.len().saturating_sub(1));
        if goal_run_detail_frame_fits_ipc_with_window(&candidate, window) {
            return Some((candidate, window, true));
        }
    }

    candidate = goal_run_stripped_summary(&candidate);
    window.loaded_step_start = goal_run.steps.len();
    window.loaded_step_end = goal_run.steps.len();
    window.loaded_event_start = goal_run.events.len();
    window.loaded_event_end = goal_run.events.len();
    if goal_run_detail_frame_fits_ipc_with_window(&candidate, window) {
        return Some((candidate, window, true));
    }

    None
}

fn cap_goal_run_window_for_ipc(
    goal_run: &GoalRun,
    step_offset: Option<usize>,
    step_limit: Option<usize>,
    event_offset: Option<usize>,
    event_limit: Option<usize>,
) -> Option<(GoalRun, GoalRunDetailWindow, bool)> {
    let step_start = step_offset.unwrap_or(0).min(goal_run.steps.len());
    let step_end = step_limit
        .map(|limit| step_start.saturating_add(limit).min(goal_run.steps.len()))
        .unwrap_or(goal_run.steps.len());
    let event_start = event_offset.unwrap_or(0).min(goal_run.events.len());
    let event_end = event_limit
        .map(|limit| event_start.saturating_add(limit).min(goal_run.events.len()))
        .unwrap_or(goal_run.events.len());

    let (candidate, window) =
        goal_run_with_window(goal_run, step_start, step_end, event_start, event_end);
    if goal_run_detail_frame_fits_ipc_with_window(&candidate, window) {
        return Some((candidate, window, false));
    }

    let mut best_window = window;
    let mut truncated = false;

    if best_window.loaded_event_start < best_window.loaded_event_end {
        let mut low = best_window.loaded_event_start;
        let mut high = best_window.loaded_event_end;
        while low < high {
            let mid = low + (high - low) / 2;
            let (trial, trial_window) = goal_run_with_window(
                goal_run,
                best_window.loaded_step_start,
                best_window.loaded_step_end,
                mid,
                best_window.loaded_event_end,
            );
            if goal_run_detail_frame_fits_ipc_with_window(&trial, trial_window) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        let (trial, trial_window) = goal_run_with_window(
            goal_run,
            best_window.loaded_step_start,
            best_window.loaded_step_end,
            low,
            best_window.loaded_event_end,
        );
        best_window = trial_window;
        truncated = best_window.loaded_event_start > event_start;
        if goal_run_detail_frame_fits_ipc_with_window(&trial, best_window) {
            return Some((trial, best_window, truncated));
        }
    }

    if best_window.loaded_step_start < best_window.loaded_step_end {
        let mut low = best_window.loaded_step_start;
        let mut high = best_window.loaded_step_end;
        while low < high {
            let mid = low + (high - low) / 2;
            let (trial, trial_window) = goal_run_with_window(
                goal_run,
                mid,
                best_window.loaded_step_end,
                best_window.loaded_event_start,
                best_window.loaded_event_end,
            );
            if goal_run_detail_frame_fits_ipc_with_window(&trial, trial_window) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        let (trial, trial_window) = goal_run_with_window(
            goal_run,
            low,
            best_window.loaded_step_end,
            best_window.loaded_event_start,
            best_window.loaded_event_end,
        );
        best_window = trial_window;
        truncated |= best_window.loaded_step_start > step_start;
        if goal_run_detail_frame_fits_ipc_with_window(&trial, best_window) {
            return Some((trial, best_window, truncated));
        }
    }

    None
}

fn goal_run_thread_context_message(
    goal_run: &GoalRun,
    goal_handle: &str,
    source_thread_id: Option<&str>,
) -> String {
    let source_line = source_thread_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("- Source thread: {value}"))
        .unwrap_or_else(|| "- Source thread: direct goal launch".to_string());
    format!(
        "Dedicated goal thread initialized.\n\n\
         - Goal handle: {goal_handle}\n\
         - Title: {}\n\
         - Big picture: {}\n\
         {source_line}\n\
         This is the sole worker thread for the goal. Do the work here. Spawned helpers keep their \
         own threads. Pass `{goal_handle}` as goal_run_id when a tool asks for the goal. Completeness is decided only by the owner supervisor via `request_goal_review`.",
        goal_run.title, goal_run.goal
    )
}

impl AgentEngine {
    async fn reserve_unique_goal_run_id(&self) -> String {
        loop {
            let candidate = format!("goal_{}", Uuid::new_v4());
            let memory_conflict = {
                let goal_runs = self.goal_runs.lock().await;
                goal_runs.iter().any(|goal_run| goal_run.id == candidate)
            };
            if memory_conflict {
                continue;
            }

            match self.history.has_goal_run_id(&candidate).await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(
                        goal_run_id = %candidate,
                        %error,
                        "failed to check persisted goal-run id conflict"
                    );
                    if self
                        .history
                        .get_goal_run(&candidate)
                        .await
                        .ok()
                        .flatten()
                        .is_some()
                    {
                        continue;
                    }
                }
            }

            return candidate;
        }
    }

    async fn initialize_goal_run_thread(&self, goal_run: &GoalRun, source_thread_id: Option<&str>) {
        let Some(goal_thread_id) = goal_run.thread_id.as_deref() else {
            return;
        };
        self.ensure_thread_messages_loaded(goal_thread_id).await;

        let normalized_source_thread_id = source_thread_id
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != goal_thread_id)
            .map(ToOwned::to_owned);
        {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(goal_thread_id) else {
                return;
            };
            thread.upstream_thread_id = normalized_source_thread_id;
            thread.updated_at = now_millis();
        }
        self.persist_thread_by_id(goal_thread_id).await;
        let goal_handle = crate::agent::tool_executor::ensure_goal_slug(
            self,
            &goal_run.id,
            &goal_run.title,
            Some(goal_thread_id),
        )
        .await
        .unwrap_or_else(|_| goal_run.id.clone());
        self.append_system_thread_message(
            goal_thread_id,
            goal_run_thread_context_message(goal_run, &goal_handle, source_thread_id),
        )
        .await;
    }

    pub async fn restore_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<crate::agent::liveness::state_layers::RestoreOutcome> {
        let Some(state_json) = self.history.get_checkpoint(checkpoint_id).await? else {
            anyhow::bail!("checkpoint not found");
        };
        let checkpoint = crate::agent::liveness::checkpoint::checkpoint_load(&state_json)?;
        let goal_run_id = checkpoint.goal_run_id.clone();
        let restored_step_index = checkpoint.goal_run.current_step_index;
        let tasks_restored = checkpoint.tasks_snapshot.len();

        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(existing) = goal_runs
                .iter_mut()
                .find(|goal_run| goal_run.id == goal_run_id)
            {
                *existing = checkpoint.goal_run.clone();
            } else {
                goal_runs.push_back(checkpoint.goal_run.clone());
            }
        }

        {
            let mut tasks = self.tasks.lock().await;
            let retained = tasks
                .drain(..)
                .filter(|task| task.goal_run_id.as_deref() != Some(goal_run_id.as_str()))
                .collect::<VecDeque<_>>();
            *tasks = retained;
            tasks.extend(checkpoint.tasks_snapshot.clone());
        }

        if let Some(thread_id) = checkpoint.thread_id.clone() {
            self.thread_todos
                .write()
                .await
                .insert(thread_id.clone(), checkpoint.todos.clone());
            if let Some(work_context) = checkpoint.work_context.clone() {
                self.thread_work_contexts
                    .write()
                    .await
                    .insert(thread_id.clone(), work_context);
            }
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&thread_id) {
                thread.updated_at = now_millis();
                thread.messages.push(AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::System,
                    content: format!(
                        "Restored goal run {} from checkpoint {} at step {}.",
                        goal_run_id, checkpoint.id, restored_step_index
                    ),
                    content_blocks: Vec::new(),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    weles_review: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    cost: None,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    upstream_message: None,
                    provider_final_result: None,
                    author_agent_id: None,
                    author_agent_name: None,
                    reasoning: None,
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    tool_output_preview_path: None,
                    structural_refs: Vec::new(),
                    pinned_for_compaction: false,
                    timestamp: now_millis(),
                    feedback: None,
                });
            }
        }

        self.persist_goal_runs().await;
        self.persist_tasks().await;
        self.persist_todos().await;
        self.persist_work_context().await;
        self.persist_threads().await;

        let outcome = crate::agent::liveness::state_layers::RestoreOutcome {
            checkpoint_id: checkpoint.id,
            goal_run_id,
            restored_step_index,
            tasks_restored,
            context_restored: checkpoint.thread_id.is_some(),
        };

        let indicators_json = serde_json::json!({
            "restored_step_index": outcome.restored_step_index,
            "tasks_restored": outcome.tasks_restored,
            "context_restored": outcome.context_restored,
        })
        .to_string();
        let _ = self
            .history
            .insert_health_log(
                &format!("health_{}", Uuid::new_v4()),
                "goal_run",
                &outcome.goal_run_id,
                "healthy",
                Some(&indicators_json),
                Some("restore_checkpoint"),
                now_millis(),
            )
            .await;

        Ok(outcome)
    }

    pub async fn start_goal_run(
        &self,
        goal: String,
        title: Option<String>,
        thread_id: Option<String>,
        session_id: Option<String>,
        priority: Option<&str>,
        client_request_id: Option<String>,
        autonomy_level: Option<String>,
        launch_assignments: Option<Vec<GoalAgentAssignment>>,
    ) -> GoalRun {
        self.start_goal_run_with_surface(
            goal,
            title,
            thread_id,
            session_id,
            priority,
            client_request_id,
            autonomy_level,
            None,
            launch_assignments,
        )
        .await
    }

    pub async fn start_goal_run_with_surface(
        &self,
        goal: String,
        title: Option<String>,
        thread_id: Option<String>,
        session_id: Option<String>,
        priority: Option<&str>,
        client_request_id: Option<String>,
        autonomy_level: Option<String>,
        client_surface: Option<zorai_protocol::ClientSurface>,
        launch_assignments: Option<Vec<GoalAgentAssignment>>,
    ) -> GoalRun {
        self.start_goal_run_with_surface_and_approval_policy(
            goal,
            title,
            thread_id,
            session_id,
            priority,
            client_request_id,
            autonomy_level,
            client_surface,
            true,
            launch_assignments,
        )
        .await
    }

    pub async fn start_goal_run_with_surface_and_approval_policy(
        &self,
        goal: String,
        title: Option<String>,
        thread_id: Option<String>,
        session_id: Option<String>,
        priority: Option<&str>,
        client_request_id: Option<String>,
        autonomy_level: Option<String>,
        client_surface: Option<zorai_protocol::ClientSurface>,
        requires_approval: bool,
        launch_assignments: Option<Vec<GoalAgentAssignment>>,
    ) -> GoalRun {
        let normalized_goal_key = normalize_goal_key(&goal);
        let normalized_request_id = client_request_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        {
            let goal_runs = self.goal_runs.lock().await;
            if let Some(existing) = goal_runs
                .iter()
                .rev()
                .find(|existing| {
                    if matches!(
                        existing.status,
                        GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled
                    ) {
                        return false;
                    }
                    if existing.thread_id != thread_id || existing.session_id != session_id {
                        return false;
                    }
                    if normalize_goal_key(&existing.goal) != normalized_goal_key {
                        return false;
                    }
                    match (&normalized_request_id, &existing.client_request_id) {
                        (Some(request_id), Some(existing_request_id)) => {
                            existing_request_id == request_id
                        }
                        (Some(_), None) => false,
                        _ => true,
                    }
                })
                .cloned()
            {
                return self.project_goal_run(existing).await;
            }
        }
        match self
            .history
            .list_active_goal_runs_for_start_request(
                thread_id.clone(),
                session_id.clone(),
                normalized_request_id.clone(),
            )
            .await
        {
            Ok(candidates) => {
                for existing in candidates {
                    if normalize_goal_key(&existing.goal) == normalized_goal_key {
                        {
                            let mut goal_runs = self.goal_runs.lock().await;
                            if !goal_runs.iter().any(|goal_run| goal_run.id == existing.id) {
                                goal_runs.push_back(existing.clone());
                            }
                        }
                        return self.project_goal_run(existing).await;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(
                    "failed to query persisted active goal runs for de-duplication: {error}"
                );
            }
        }

        let normalized_title = title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| summarize_goal_title(&goal));
        let source_thread_id = thread_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let effective_client_surface = match client_surface {
            Some(client_surface) => Some(client_surface),
            None => match source_thread_id.as_deref() {
                Some(source_thread_id) => self.get_thread_client_surface(source_thread_id).await,
                None => None,
            },
        };
        let goal_run_id = self.reserve_unique_goal_run_id().await;
        let dedicated_thread_id = format!("goal:{goal_run_id}");
        let (created_thread_id, _) = self
            .get_or_create_thread(Some(&dedicated_thread_id), &normalized_title)
            .await;
        let goal_thread_id = Some(created_thread_id);
        let now = now_millis();
        if let (Some(thread_id), Some(client_surface)) =
            (goal_thread_id.as_deref(), effective_client_surface)
        {
            self.set_thread_client_surface(thread_id, client_surface)
                .await;
        }
        let adaptation_mode = {
            let model = self.operator_model.read().await;
            SatisfactionAdaptationMode::from_label(&model.operator_satisfaction.label)
        };
        let launch_assignment_snapshot = match launch_assignments {
            Some(assignments) if !assignments.is_empty() => {
                self.normalized_goal_launch_assignment_snapshot(assignments)
                    .await
            }
            _ => self.goal_launch_assignment_snapshot().await,
        };
        let goal_run = GoalRun {
            id: goal_run_id,
            title: normalized_title,
            goal,
            client_request_id: normalized_request_id,
            status: GoalRunStatus::Queued,
            priority: parse_priority_str(priority.unwrap_or("normal")),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            thread_id: goal_thread_id,
            root_thread_id: None,
            supervision_thread_id: source_thread_id.clone(),
            active_thread_id: None,
            execution_thread_ids: Vec::new(),
            session_id,
            current_step_index: 0,
            current_step_title: None,
            current_step_kind: None,
            launch_assignment_snapshot: launch_assignment_snapshot.clone(),
            runtime_assignment_list: launch_assignment_snapshot,
            planner_owner_profile: None,
            current_step_owner_profile: None,
            step_owner_overrides: std::collections::BTreeMap::new(),
            replan_count: 0,
            max_replans: adaptation_mode.max_goal_replans(2),
            plan_summary: None,
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            step_failure_history: Vec::new(),
            stopped_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            active_task_id: None,
            pending_review_report: None,
            duration_ms: None,
            child_task_ids: Vec::new(),
            child_task_count: 0,
            approval_count: 0,
            steps: Vec::new(),
            events: vec![make_goal_run_event("queue", "goal run created", None)],
            dossier: None,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            model_usage: Vec::new(),
            autonomy_level: autonomy_level
                .as_deref()
                .map(super::autonomy::AutonomyLevel::from_str_or_default)
                .unwrap_or_default(),
            authorship_tag: (!requires_approval).then_some(super::AuthorshipTag::Agent),
        };
        let mut goal_run = goal_run;
        let goal_thread_id = goal_run.thread_id.clone();
        super::goal_run_apply_thread_routing(&mut goal_run, goal_thread_id);
        if let Some(goal_thread_id) = goal_run.thread_id.as_deref() {
            self.set_thread_identity_metadata(
                goal_thread_id,
                ThreadIdentityMetadata::for_goal_thread(goal_thread_id, &goal_run.id),
            )
            .await;
        }
        self.initialize_goal_run_thread(&goal_run, source_thread_id.as_deref())
            .await;
        self.pin_goal_owner_thread(&goal_run).await;

        self.goal_runs.lock().await.push_back(goal_run.clone());
        if let Some(client_surface) = effective_client_surface {
            self.set_goal_run_client_surface(&goal_run.id, client_surface)
                .await;
        }
        self.persist_goal_runs().await;
        crate::governance::record_transition_audit(
            &self.history,
            crate::governance::TransitionKind::RunAdmission,
            crate::governance::TransitionAuditIds {
                goal_run_id: Some(goal_run.id.clone()),
                thread_id: goal_run.thread_id.clone(),
                ..Default::default()
            },
            serde_json::json!({
                "goal": goal_run.goal.clone(),
                "title": goal_run.title.clone(),
                "priority": format!("{:?}", goal_run.priority),
            }),
            "admitted",
        )
        .await;
        self.emit_goal_run_update(&goal_run, Some("Goal queued".into()));
        self.record_provenance_event(
            "goal_created",
            "goal run created",
            serde_json::json!({
                "goal_run_id": goal_run.id.clone(),
                "title": goal_run.title.clone(),
                "goal": goal_run.goal.clone(),
                "priority": format!("{:?}", goal_run.priority).to_lowercase(),
                "source_thread_id": source_thread_id,
            }),
            Some(goal_run.id.as_str()),
            None,
            goal_run.thread_id.as_deref(),
            None,
            None,
        )
        .await;
        if let Err(error) = self.record_goal_start_episode(&goal_run).await {
            tracing::warn!(
                goal_run_id = %goal_run.id,
                error = %error,
                "failed to record goal start episode"
            );
        }
        self.project_goal_run(goal_run).await
    }

    pub async fn list_goal_runs(&self) -> Vec<GoalRun> {
        self.list_goal_runs_paginated_capped_for_ipc(None, None)
            .await
            .0
    }

    pub(crate) async fn list_goal_runs_paginated_for_tool(
        &self,
        limit: usize,
        offset: usize,
    ) -> (Vec<GoalRun>, usize) {
        match self.history.list_goal_runs_page(limit, offset).await {
            Ok((goal_runs, total)) => {
                let mut page = Vec::with_capacity(goal_runs.len());
                for goal_run in goal_runs {
                    page.push(self.project_goal_run(goal_run).await);
                }
                (page, total)
            }
            Err(error) => {
                tracing::warn!("failed to list paged persisted goal runs: {error}");
                let goal_runs = self.goal_runs.lock().await;
                let mut items: Vec<GoalRun> = goal_runs.iter().cloned().collect();
                drop(goal_runs);
                let mut projected = Vec::with_capacity(items.len());
                for goal_run in items.drain(..) {
                    projected.push(self.project_goal_run(goal_run).await);
                }
                projected.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                let total = projected.len();
                let page = projected.into_iter().skip(offset).take(limit).collect();
                (page, total)
            }
        }
    }

    pub(crate) async fn list_goal_runs_paginated_capped_for_ipc(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> (Vec<GoalRun>, bool) {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(i64::MAX as usize);
        let projected = match self.history.list_goal_runs_page(limit, offset).await {
            Ok((goal_runs, total)) if total > 0 => self.project_goal_runs_batched(goal_runs).await,
            Ok(_) => {
                let goal_runs = self.goal_runs.lock().await;
                let items: Vec<GoalRun> = goal_runs.iter().cloned().collect();
                drop(goal_runs);
                let mut projected = self.project_goal_runs_batched(items).await;
                projected.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                projected.into_iter().skip(offset).take(limit).collect()
            }
            Err(error) => {
                tracing::warn!("failed to list paged persisted goal runs for IPC: {error}");
                let goal_runs = self.goal_runs.lock().await;
                let items: Vec<GoalRun> = goal_runs.iter().cloned().collect();
                drop(goal_runs);
                let mut projected = self.project_goal_runs_batched(items).await;
                projected.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                projected.into_iter().skip(offset).take(limit).collect()
            }
        };

        if goal_run_list_frame_fits_ipc(&projected) {
            return (projected, false);
        }

        let mut capped = Vec::with_capacity(projected.len());
        for goal_run in projected {
            if let Some((goal_run, _window, _goal_truncated)) = cap_goal_run_for_ipc(goal_run) {
                capped.push(goal_run);
            }
        }

        if goal_run_list_frame_fits_ipc(&capped) {
            return (capped, true);
        }

        let mut low = 0usize;
        let mut high = capped.len();
        while low < high {
            let mid = low + (high - low + 1) / 2;
            if goal_run_list_frame_fits_ipc(&capped[..mid]) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }

        (capped.into_iter().take(low).collect(), true)
    }

    pub async fn get_goal_run(&self, goal_run_id: &str) -> Option<GoalRun> {
        let goal_run = self
            .goal_runs
            .lock()
            .await
            .iter()
            .find(|goal_run| goal_run.id == goal_run_id)
            .cloned();
        if let Some(goal_run) = goal_run {
            return Some(self.project_goal_run(goal_run).await);
        }
        match self.history.get_goal_run(goal_run_id).await {
            Ok(Some(goal_run)) => Some(self.project_goal_run(goal_run).await),
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(goal_run_id, "failed to load persisted goal run: {error}");
                None
            }
        }
    }

    pub(crate) async fn get_goal_run_capped_for_ipc(
        &self,
        goal_run_id: &str,
    ) -> Option<(String, bool)> {
        let goal_run = self.get_goal_run(goal_run_id).await?;
        let (goal_run, window, truncated) = cap_goal_run_for_ipc(goal_run)?;
        Some((goal_run_wire_json(&goal_run, window)?, truncated))
    }

    pub(crate) async fn get_goal_run_page_capped_for_ipc(
        &self,
        goal_run_id: &str,
        step_offset: Option<usize>,
        step_limit: Option<usize>,
        event_offset: Option<usize>,
        event_limit: Option<usize>,
    ) -> Option<(String, bool)> {
        let goal_run = self.get_goal_run(goal_run_id).await?;
        let (goal_run, window, truncated) = cap_goal_run_window_for_ipc(
            &goal_run,
            step_offset,
            step_limit,
            event_offset,
            event_limit,
        )?;
        Some((goal_run_wire_json(&goal_run, window)?, truncated))
    }

    pub async fn list_todos(&self) -> HashMap<String, Vec<TodoItem>> {
        self.thread_todos.read().await.clone()
    }

    /// Full task list with logs; kept for tests that exercise log capping.
    /// IPC list endpoints use `list_tasks_metadata_only_capped_for_ipc`.
    #[cfg(test)]
    pub(crate) async fn list_tasks_capped_for_ipc(&self) -> (Vec<AgentTask>, bool) {
        cap_task_list_for_ipc(self.list_tasks().await)
    }

    /// Task list for the Electron `list-tasks` IPC poll: metadata only, no
    /// `logs` hydration. The persisted log table can hold hundreds of MB of
    /// historical tool output; hydrating it on every poll stalled the SQLite
    /// read pool for tens of seconds and starved every other IPC query
    /// (thread list, status, prompt queue) into client-side timeouts.
    pub(crate) async fn list_tasks_metadata_only_capped_for_ipc(&self) -> (Vec<AgentTask>, bool) {
        self.refresh_task_queue_state_for_filtered_list().await;
        let tasks = match self
            .history
            .list_agent_tasks_filtered_without_logs(&crate::history::AgentTaskListQuery::default())
            .await
        {
            Ok(mut tasks) => {
                for task in &mut tasks {
                    crate::agent::persistence::sanitize_task_for_external_view(task);
                }
                let in_memory = self.snapshot_tasks().await;
                if !in_memory.is_empty() {
                    let known_ids: std::collections::HashSet<String> =
                        tasks.iter().map(|task| task.id.clone()).collect();
                    for mut task in in_memory {
                        if !known_ids.contains(&task.id) {
                            task.logs.clear();
                            tasks.push(task);
                        }
                    }
                }
                tasks
            }
            Err(error) => {
                tracing::warn!("failed to list persisted tasks without logs: {error}");
                self.snapshot_tasks()
                    .await
                    .into_iter()
                    .map(|mut task| {
                        task.logs.clear();
                        task
                    })
                    .collect()
            }
        };
        cap_task_list_for_ipc(tasks)
    }

    pub(crate) async fn list_todos_capped_for_ipc(&self) -> (HashMap<String, Vec<TodoItem>>, bool) {
        cap_todo_list_for_ipc(self.list_todos().await)
    }

    pub async fn get_todos(&self, thread_id: &str) -> Vec<TodoItem> {
        self.thread_todos
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) async fn get_todos_capped_for_ipc(&self, thread_id: &str) -> (Vec<TodoItem>, bool) {
        cap_todos_for_ipc(thread_id, self.get_todos(thread_id).await)
    }

    pub async fn get_work_context(&self, thread_id: &str) -> ThreadWorkContext {
        self.refresh_thread_repo_context(thread_id).await;
        self.thread_work_contexts
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_else(|| ThreadWorkContext {
                thread_id: thread_id.to_string(),
                entries: Vec::new(),
            })
    }

    pub(crate) async fn get_work_context_capped_for_ipc(
        &self,
        thread_id: &str,
    ) -> (ThreadWorkContext, bool) {
        cap_work_context_for_ipc(thread_id, self.get_work_context(thread_id).await)
    }

    pub(super) async fn project_goal_run(&self, goal_run: GoalRun) -> GoalRun {
        let mut related_tasks = self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: None,
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: Some(goal_run.id.clone()),
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
        let mut task_ids = related_tasks
            .iter()
            .map(|task| task.id.clone())
            .collect::<std::collections::HashSet<_>>();
        for task in self
            .tasks
            .lock()
            .await
            .iter()
            .filter(|task| task.goal_run_id.as_deref() == Some(goal_run.id.as_str()))
        {
            if task_ids.insert(task.id.clone()) {
                related_tasks.push(task.clone());
            }
        }
        let mut projected = project_goal_run_snapshot(goal_run, &related_tasks, now_millis());
        self.attach_pending_review_report(&mut projected).await;
        projected
    }

    /// Projects a batch of goal_runs in O(1) SQL round-trips instead of
    /// O(N). Replaces the per-goal-run `project_goal_run().await` loop that
    /// the AgentListGoalRuns dispatch handler used to do — under load that
    /// blocked the connection's outer loop for hundreds of milliseconds and
    /// starved every other client message in the cascade.
    pub(super) async fn project_goal_runs_batched(&self, goal_runs: Vec<GoalRun>) -> Vec<GoalRun> {
        if goal_runs.is_empty() {
            return Vec::new();
        }
        let goal_run_ids: Vec<String> = goal_runs.iter().map(|g| g.id.clone()).collect();
        let mut persisted_by_goal = match self
            .history
            .list_agent_tasks_by_goal_run_ids(&goal_run_ids)
            .await
        {
            Ok(map) => map,
            Err(error) => {
                tracing::warn!(
                    %error,
                    "failed to batch-fetch tasks by goal_run_ids; goal-run page will be empty of related tasks"
                );
                std::collections::HashMap::new()
            }
        };
        let live_tasks_by_goal: std::collections::HashMap<String, Vec<AgentTask>> = {
            let live = self.tasks.lock().await;
            let mut map: std::collections::HashMap<String, Vec<AgentTask>> =
                std::collections::HashMap::new();
            for task in live.iter() {
                if let Some(id) = task.goal_run_id.clone() {
                    if goal_run_ids.iter().any(|candidate| candidate == &id) {
                        map.entry(id).or_default().push(task.clone());
                    }
                }
            }
            map
        };
        let now = now_millis();
        let mut projected = Vec::with_capacity(goal_runs.len());
        for goal_run in goal_runs {
            let mut related = persisted_by_goal.remove(&goal_run.id).unwrap_or_default();
            if let Some(live_tasks) = live_tasks_by_goal.get(&goal_run.id) {
                let mut seen: std::collections::HashSet<String> =
                    related.iter().map(|task| task.id.clone()).collect();
                for task in live_tasks {
                    if seen.insert(task.id.clone()) {
                        related.push(task.clone());
                    }
                }
            }
            let mut projected_run = project_goal_run_snapshot(goal_run, &related, now);
            self.attach_pending_review_report(&mut projected_run).await;
            projected.push(projected_run);
        }
        projected
    }

    async fn list_task_refs_for_goal_relation(
        &self,
        query: crate::history::AgentTaskListQuery,
    ) -> Vec<(String, Option<String>, Option<String>)> {
        match self.history.list_agent_task_refs_filtered(&query).await {
            Ok(refs) => refs,
            Err(error) => {
                tracing::warn!(
                    %error,
                    "failed to query lightweight task refs; falling back to hydrated task rows"
                );
                self.list_tasks_filtered(&query)
                    .await
                    .into_iter()
                    .map(|task| (task.id, task.thread_id, task.parent_thread_id))
                    .collect()
            }
        }
    }

    pub(in crate::agent) async fn goal_related_task_ids(&self, goal_run: &GoalRun) -> Vec<String> {
        let mut task_ids = declared_goal_task_ids(goal_run);
        let mut thread_ids = declared_goal_thread_ids(goal_run);

        loop {
            let mut changed = false;
            let mut related_task_refs = self
                .list_task_refs_for_goal_relation(crate::history::AgentTaskListQuery {
                    id: None,
                    status: None,
                    statuses: Vec::new(),
                    source: None,
                    thread_id: None,
                    thread_ids: Vec::new(),
                    goal_run_id: Some(goal_run.id.clone()),
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
            if !thread_ids.is_empty() {
                related_task_refs.extend(
                    self.list_task_refs_for_goal_relation(crate::history::AgentTaskListQuery {
                        id: None,
                        status: None,
                        statuses: Vec::new(),
                        source: None,
                        thread_id: None,
                        thread_ids: thread_ids.clone(),
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
                    .await,
                );
            }
            if !task_ids.is_empty() {
                related_task_refs.extend(
                    self.list_task_refs_for_goal_relation(crate::history::AgentTaskListQuery {
                        id: None,
                        ids: task_ids.clone(),
                        status: None,
                        statuses: Vec::new(),
                        source: None,
                        thread_id: None,
                        thread_ids: Vec::new(),
                        goal_run_id: None,
                        parent_task_id: None,
                        parent_task_ids: Vec::new(),
                        awaiting_approval_id: None,
                        supervisor_config_present: false,
                        exclude_terminal_statuses: false,
                        order_by_recent_activity_desc: false,
                        limit: None,
                    })
                    .await,
                );
                related_task_refs.extend(
                    self.list_task_refs_for_goal_relation(crate::history::AgentTaskListQuery {
                        id: None,
                        ids: Vec::new(),
                        status: None,
                        statuses: Vec::new(),
                        source: None,
                        thread_id: None,
                        thread_ids: Vec::new(),
                        goal_run_id: None,
                        parent_task_id: None,
                        parent_task_ids: task_ids.clone(),
                        awaiting_approval_id: None,
                        supervisor_config_present: false,
                        exclude_terminal_statuses: false,
                        order_by_recent_activity_desc: false,
                        limit: None,
                    })
                    .await,
                );
            }
            for (task_id, thread_id, parent_thread_id) in related_task_refs {
                changed |= push_unique_string(&mut task_ids, &task_id);
                if let Some(thread_id) = thread_id.as_deref() {
                    changed |= push_unique_string(&mut thread_ids, thread_id);
                }
                if let Some(parent_thread_id) = parent_thread_id.as_deref() {
                    changed |= push_unique_string(&mut thread_ids, parent_thread_id);
                }
            }
            if !changed {
                break;
            }
        }

        task_ids
    }

    pub(in crate::agent) async fn pause_goal_tasks(&self, goal_run: &GoalRun) {
        for task_id in self.goal_related_task_ids(goal_run).await {
            let task = self
                .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                    id: Some(task_id.clone()),
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
            let Some(mut task) = task else { continue };
            if !matches!(
                task.status,
                TaskStatus::Queued | TaskStatus::InProgress | TaskStatus::FailedAnalyzing
            ) {
                continue;
            }
            let previous =
                serde_json::to_string(&task.status).unwrap_or_else(|_| "\"queued\"".to_string());
            task.status = TaskStatus::Blocked;
            task.blocked_reason = Some(format!("goal_paused:{previous}"));
            task.lane_id = None;
            task.logs.push(make_task_log_entry(
                task.retry_count,
                TaskLogLevel::Info,
                "goal_pause",
                "task suspended because its goal was paused",
                None,
            ));
            if let Err(error) = self.history.upsert_agent_task(&task).await {
                tracing::warn!(task_id = %task.id, %error, "failed to persist paused goal task");
                continue;
            }
            let mut tasks = self.tasks.lock().await;
            if let Some(live) = tasks.iter_mut().find(|live| live.id == task.id) {
                *live = task;
            }
        }
    }

    async fn resume_goal_tasks(&self, goal_run: &GoalRun) {
        for task_id in self.goal_related_task_ids(goal_run).await {
            let Some(mut task) = self
                .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                    id: Some(task_id),
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
            else {
                continue;
            };
            let Some(reason) = task
                .blocked_reason
                .as_deref()
                .and_then(|value| value.strip_prefix("goal_paused:"))
            else {
                continue;
            };
            task.status = if reason.contains("in_progress") || reason.contains("failed_analyzing") {
                TaskStatus::Queued
            } else {
                serde_json::from_str(reason).unwrap_or(TaskStatus::Queued)
            };
            task.blocked_reason = None;
            task.logs.push(make_task_log_entry(
                task.retry_count,
                TaskLogLevel::Info,
                "goal_resume",
                "task restored after goal resume",
                None,
            ));
            if self.history.upsert_agent_task(&task).await.is_ok() {
                let mut tasks = self.tasks.lock().await;
                if let Some(live) = tasks.iter_mut().find(|live| live.id == task.id) {
                    *live = task;
                }
            }
        }
    }

    async fn goal_execution_thread_ids(
        &self,
        goal_run: &GoalRun,
    ) -> std::collections::HashSet<String> {
        let task_ids = self.goal_related_task_ids(goal_run).await;
        let mut thread_ids = declared_goal_thread_ids(goal_run)
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        if !task_ids.is_empty() {
            for (_, thread_id, parent_thread_id) in self
                .list_task_refs_for_goal_relation(crate::history::AgentTaskListQuery {
                    id: None,
                    ids: task_ids,
                    status: None,
                    statuses: Vec::new(),
                    source: None,
                    thread_id: None,
                    thread_ids: Vec::new(),
                    goal_run_id: None,
                    parent_task_id: None,
                    parent_task_ids: Vec::new(),
                    awaiting_approval_id: None,
                    supervisor_config_present: false,
                    exclude_terminal_statuses: false,
                    order_by_recent_activity_desc: false,
                    limit: None,
                })
                .await
            {
                if let Some(thread_id) = thread_id.filter(|value| !value.is_empty()) {
                    thread_ids.insert(thread_id);
                }
                if let Some(parent_thread_id) = parent_thread_id.filter(|value| !value.is_empty()) {
                    thread_ids.insert(parent_thread_id);
                }
            }
        }
        thread_ids
    }

    pub(in crate::agent) async fn quiesce_goal_execution_tree(
        &self,
        goal_run: &GoalRun,
        cancel: bool,
    ) {
        let thread_ids = self.goal_execution_thread_ids(goal_run).await;
        for thread_id in &thread_ids {
            let _ = self.stop_stream(thread_id).await;
        }
        if cancel {
            self.clear_deferred_visible_thread_continuations_for_threads(&thread_ids)
                .await;
            self.remove_operation_wakeups_for_threads(&thread_ids, true)
                .await;
            self.cancel_wakeups_for_threads(&thread_ids).await;
        }
        self.inflight_goal_runs.lock().await.remove(&goal_run.id);
    }

    pub async fn control_goal_run(
        &self,
        goal_run_id: &str,
        action: &str,
        step_index: Option<usize>,
        payload_json: Option<&str>,
    ) -> bool {
        if let Ok(verdict) = parse_goal_supervisor_verdict(action) {
            let explanation = payload_json
                .and_then(|value| {
                    serde_json::from_str::<serde_json::Value>(value)
                        .ok()
                        .and_then(|json| {
                            json.get("explanation")
                                .and_then(|value| value.as_str())
                                .map(ToOwned::to_owned)
                        })
                        .or_else(|| {
                            let trimmed = value.trim();
                            (!trimmed.is_empty() && !trimmed.starts_with('{'))
                                .then(|| trimmed.to_string())
                        })
                })
                .unwrap_or_default();
            return self
                .submit_goal_review(goal_run_id, verdict, &explanation, None)
                .await
                .is_ok();
        }

        let persisted_goal_run = self.get_goal_run(goal_run_id).await;
        let mut changed_goal: Option<GoalRun> = None;
        let mut tasks_to_cancel: Vec<String> = Vec::new();
        let mut task_to_release: Option<(String, Option<String>)> = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if !goal_runs.iter().any(|item| item.id == goal_run_id) {
                let Some(goal_run) = persisted_goal_run.clone() else {
                    return false;
                };
                goal_runs.push_back(goal_run);
            }
            let goal_run = goal_runs
                .iter_mut()
                .find(|item| item.id == goal_run_id)
                .expect("goal run should exist after live rehydration");

            match action {
                "pause" => {
                    if matches!(
                        goal_run.status,
                        GoalRunStatus::Queued
                            | GoalRunStatus::Planning
                            | GoalRunStatus::Running
                            | GoalRunStatus::AwaitingApproval
                            | GoalRunStatus::AwaitingReview
                    ) {
                        goal_run.status = GoalRunStatus::Paused;
                        goal_run.updated_at = now_millis();
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run paused",
                            None,
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "resume" => {
                    if matches!(
                        goal_run.status,
                        GoalRunStatus::Paused | GoalRunStatus::Blocked
                    ) {
                        let prior = goal_run.status;
                        goal_run.status = if goal_run.active_task_id.is_some() {
                            GoalRunStatus::Running
                        } else {
                            GoalRunStatus::Queued
                        };
                        goal_run.updated_at = now_millis();
                        let message = if prior == GoalRunStatus::Blocked {
                            "goal run unblocked"
                        } else {
                            "goal run resumed"
                        };
                        goal_run
                            .events
                            .push(make_goal_run_event("control", message, None));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "block" => {
                    // Governance-induced stall. Distinct from "pause"
                    // (operator) and "acknowledge" (resolves an approval).
                    // Reversible via "resume" — exits through
                    // TransitionKind::ResumeFromBlocked.
                    if matches!(
                        goal_run.status,
                        GoalRunStatus::Queued
                            | GoalRunStatus::Planning
                            | GoalRunStatus::Running
                            | GoalRunStatus::AwaitingApproval
                            | GoalRunStatus::Paused
                    ) {
                        goal_run.status = GoalRunStatus::Blocked;
                        goal_run.updated_at = now_millis();
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run blocked by governance gate",
                            None,
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "acknowledge" | "ack" => {
                    if goal_run.status == GoalRunStatus::AwaitingApproval {
                        let current_task_id = goal_run
                            .steps
                            .get(goal_run.current_step_index)
                            .and_then(|step| step.task_id.clone());
                        let current_ack = goal_run.awaiting_approval_id.clone();
                        let has_steps = !goal_run.steps.is_empty();
                        goal_run.status = if has_steps {
                            GoalRunStatus::Running
                        } else {
                            GoalRunStatus::Queued
                        };
                        goal_run.updated_at = now_millis();
                        goal_run.awaiting_approval_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "autonomy_acknowledgment",
                            "supervised acknowledgment received; approval gate cleared",
                            current_ack.clone(),
                        ));
                        changed_goal = Some(goal_run.clone());
                        task_to_release = current_task_id.map(|task_id| (task_id, current_ack));
                    }
                }
                "cancel" => {
                    if !matches!(
                        goal_run.status,
                        GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled
                    ) {
                        goal_run.status = GoalRunStatus::Cancelled;
                        goal_run.completed_at = Some(now_millis());
                        goal_run.updated_at = now_millis();
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run cancelled",
                            None,
                        ));
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        tasks_to_cancel = declared_goal_task_ids(goal_run);
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "stop" => {
                    if !matches!(
                        goal_run.status,
                        GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled
                    ) {
                        let stopped_at = now_millis();
                        goal_run.status = GoalRunStatus::Cancelled;
                        goal_run.completed_at = Some(stopped_at);
                        goal_run.updated_at = stopped_at;
                        goal_run.stopped_reason = Some("operator_stop".to_string());
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run stopped",
                            Some("operator_stop".to_string()),
                        ));
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        tasks_to_cancel = declared_goal_task_ids(goal_run);
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "contain" => {
                    if !goal_run.status.is_terminal() {
                        let now = now_millis();
                        goal_run.status = GoalRunStatus::Contained;
                        goal_run.completed_at = Some(now);
                        goal_run.updated_at = now;
                        goal_run.stopped_reason = Some("operator_contain".to_string());
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run contained for operator review",
                            None,
                        ));
                        tasks_to_cancel = declared_goal_task_ids(goal_run);
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "compensate" => {
                    if !goal_run.status.is_terminal() {
                        let now = now_millis();
                        goal_run.status = GoalRunStatus::Compensated;
                        goal_run.completed_at = Some(now);
                        goal_run.updated_at = now;
                        goal_run.stopped_reason = Some("operator_compensate".to_string());
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run effects compensated",
                            None,
                        ));
                        tasks_to_cancel = declared_goal_task_ids(goal_run);
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "compensate-partial" => {
                    if !goal_run.status.is_terminal() {
                        let now = now_millis();
                        goal_run.status = GoalRunStatus::PartiallyCompensated;
                        goal_run.completed_at = Some(now);
                        goal_run.updated_at = now;
                        goal_run.stopped_reason = Some("operator_partial_compensate".to_string());
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run effects partially compensated",
                            None,
                        ));
                        tasks_to_cancel = declared_goal_task_ids(goal_run);
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "break-glass" => {
                    if !goal_run.status.is_terminal() {
                        let now = now_millis();
                        goal_run.status = GoalRunStatus::BreakGlass;
                        goal_run.completed_at = Some(now);
                        goal_run.updated_at = now;
                        goal_run.stopped_reason = Some("operator_break_glass".to_string());
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run completed under break-glass override",
                            None,
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "update-role" | "update_role" => {
                    if goal_run.status.is_terminal() {
                        // No-op on terminal runs.
                    } else if let (Some(target_step), Some(payload_str)) =
                        (step_index, payload_json)
                    {
                        let parsed: Result<crate::agent::GoalRuntimeOwnerProfile, _> =
                            serde_json::from_str(payload_str);
                        if let Ok(new_profile) = parsed {
                            let total_steps = goal_run.steps.len();
                            let current_idx = goal_run.current_step_index;
                            if target_step == current_idx
                                && (total_steps == 0 || target_step < total_steps)
                            {
                                goal_run.current_step_owner_profile = Some(new_profile.clone());
                                goal_run.step_owner_overrides.remove(&target_step);
                                goal_run.updated_at = now_millis();
                                goal_run.events.push(make_goal_run_event(
                                    "control",
                                    "owner profile updated for active step",
                                    Some(format!(
                                        "step {} -> {} / {}",
                                        target_step, new_profile.provider, new_profile.model
                                    )),
                                ));
                                changed_goal = Some(goal_run.clone());
                            } else if target_step > current_idx
                                && (total_steps == 0 || target_step < total_steps)
                            {
                                goal_run
                                    .step_owner_overrides
                                    .insert(target_step, new_profile.clone());
                                goal_run.updated_at = now_millis();
                                goal_run.events.push(make_goal_run_event(
                                    "control",
                                    "owner profile override staged for future step",
                                    Some(format!(
                                        "step {} -> {} / {}",
                                        target_step, new_profile.provider, new_profile.model
                                    )),
                                ));
                                changed_goal = Some(goal_run.clone());
                            }
                            // target_step < current_idx is a no-op: past
                            // steps are immutable for the purpose of role
                            // assignment.
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some((task_id, ack_id)) = task_to_release {
            let mut released_live_task = false;
            let mut released_task = {
                let mut tasks = self.tasks.lock().await;
                tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .and_then(|task| {
                        if release_task_after_supervised_acknowledgment(task, ack_id.clone()) {
                            released_live_task = true;
                            Some(task.clone())
                        } else {
                            None
                        }
                    })
            };

            if released_task.is_none() {
                let persisted_task = self
                    .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                        id: Some(task_id.clone()),
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
                if let Some(mut task) = persisted_task {
                    if release_task_after_supervised_acknowledgment(&mut task, ack_id) {
                        if let Err(error) = self.history.upsert_agent_task(&task).await {
                            tracing::warn!(
                                task_id = %task.id,
                                "failed to persist supervised acknowledgment task release: {error}"
                            );
                        } else {
                            {
                                let mut tasks = self.tasks.lock().await;
                                if !tasks.iter().any(|entry| entry.id == task.id) {
                                    tasks.push_back(task.clone());
                                }
                            }
                            released_task = Some(task);
                        }
                    }
                }
            }

            if let Some(task) = released_task {
                if released_live_task {
                    self.persist_tasks().await;
                }
                self.emit_task_update(&task, Some(status_message(&task).into()));
            }
        }

        if matches!(
            action,
            "cancel" | "stop" | "contain" | "compensate" | "compensate-partial"
        ) {
            if let Some(goal_run) = changed_goal.as_ref() {
                for task_id in self.goal_related_task_ids(goal_run).await {
                    push_unique_string(&mut tasks_to_cancel, &task_id);
                }
            }
        }

        if let Some(goal_run) = changed_goal.as_ref() {
            if action == "pause" {
                self.pause_goal_tasks(goal_run).await;
                self.quiesce_goal_execution_tree(goal_run, false).await;
            } else if action == "resume" {
                self.resume_goal_tasks(goal_run).await;
            } else if matches!(
                action,
                "cancel" | "stop" | "contain" | "compensate" | "compensate-partial"
            ) {
                self.quiesce_goal_execution_tree(goal_run, true).await;
            }
        }

        for task_id in tasks_to_cancel {
            let _ = self.cancel_task(&task_id).await;
        }

        if let Some(goal_run) = changed_goal {
            self.persist_goal_runs().await;
            if goal_run.status.is_terminal() {
                self.cancel_goal_wakeups(&goal_run.id).await;
                self.unpin_goal_owner_thread(&goal_run).await;
            }
            // Settle skill consultations and causal traces for any operator
            // -initiated terminal transition that didn't run through the
            // normal finalization path (which handles Completed/Failed).
            let settle_outcome = match goal_run.status {
                GoalRunStatus::Cancelled => Some("cancelled"),
                GoalRunStatus::Contained => Some("contained"),
                GoalRunStatus::Compensated => Some("compensated"),
                GoalRunStatus::PartiallyCompensated => Some("partially_compensated"),
                GoalRunStatus::BreakGlass => Some("break_glass"),
                _ => None,
            };
            if let Some(outcome) = settle_outcome {
                self.settle_goal_skill_consultations(&goal_run, outcome)
                    .await;
                self.settle_goal_plan_causal_traces(&goal_run.id, outcome, None)
                    .await;
            }
            // Record the governance transition for control verbs that flip
            // goal-run state in policy-significant ways. `resume` is the
            // canonical ResumeFromBlocked site; `cancel`/`stop` are operator
            // -initiated FinalDisposition events (the LLM-completed and
            // failure-path FinalDispositions are recorded inside the planner
            // finalization functions).
            let audit_transition = match action {
                "resume" => Some(crate::governance::TransitionKind::ResumeFromBlocked),
                "cancel" | "stop" | "contain" | "compensate" | "compensate-partial"
                | "break-glass" => Some(crate::governance::TransitionKind::FinalDisposition),
                _ => None,
            };
            if let Some(transition) = audit_transition {
                crate::governance::record_transition_audit(
                    &self.history,
                    transition,
                    crate::governance::TransitionAuditIds {
                        goal_run_id: Some(goal_run.id.clone()),
                        thread_id: goal_run.thread_id.clone(),
                        ..Default::default()
                    },
                    serde_json::json!({
                        "action": action,
                        "status": goal_run_status_message(&goal_run),
                        "stopped_reason": goal_run.stopped_reason.clone(),
                    }),
                    action,
                )
                .await;
            }
            self.emit_goal_run_update(&goal_run, Some(goal_run_status_message(&goal_run).into()));
            return true;
        }

        false
    }

    pub async fn delete_goal_run(&self, goal_run_id: &str) -> bool {
        let live_removed_goal = {
            let mut goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .position(|goal_run| goal_run.id == goal_run_id)
                .map(|index| {
                    goal_runs
                        .remove(index)
                        .expect("validated goal run index should remove item")
                })
        };
        let removed_goal = match live_removed_goal {
            Some(goal_run) => goal_run,
            None => match self.history.get_goal_run(goal_run_id).await {
                Ok(Some(goal_run)) => goal_run,
                Ok(None) => return false,
                Err(error) => {
                    tracing::warn!(
                        goal_run_id = %goal_run_id,
                        %error,
                        "failed to load goal run for deletion"
                    );
                    return false;
                }
            },
        };

        let mut related_task_ids = {
            let mut tasks = self.tasks.lock().await;
            let mut removed_ids = Vec::new();
            tasks.retain(|task| {
                let should_remove = task.goal_run_id.as_deref() == Some(goal_run_id)
                    || removed_goal
                        .child_task_ids
                        .iter()
                        .any(|child_task_id| child_task_id == &task.id);
                if should_remove {
                    removed_ids.push(task.id.clone());
                }
                !should_remove
            });
            removed_ids
        };
        for (task_id, _, _) in self
            .list_task_refs_for_goal_relation(crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: None,
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: Some(goal_run_id.to_string()),
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
        {
            push_unique_string(&mut related_task_ids, &task_id);
        }
        for child_task_id in &removed_goal.child_task_ids {
            if self
                .history
                .has_agent_task_id(child_task_id)
                .await
                .unwrap_or(false)
            {
                push_unique_string(&mut related_task_ids, child_task_id);
            }
        }

        self.inflight_goal_runs.lock().await.remove(goal_run_id);
        self.cost_trackers.lock().await.remove(goal_run_id);
        self.cancel_goal_wakeups(goal_run_id).await;

        if let Err(error) = self.history.delete_goal_run(goal_run_id).await {
            tracing::warn!(goal_run_id = %goal_run_id, %error, "failed to delete goal run history");
        }
        if let Err(error) = self
            .history
            .delete_checkpoints_for_goal_run(goal_run_id)
            .await
        {
            tracing::warn!(goal_run_id = %goal_run_id, %error, "failed to delete goal checkpoints");
        }
        for task_id in related_task_ids {
            if let Err(error) = self.history.delete_agent_task(&task_id).await {
                tracing::warn!(task_id = %task_id, %error, "failed to delete goal task history");
            }
        }

        true
    }
}

#[cfg(test)]
#[path = "tests/task_crud.rs"]
mod tests;
