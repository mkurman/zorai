use super::analysis::classify_stalled_turn;
use super::types::{StalledTurnClass, ThreadStallObservation, TurnEvidence};
use super::*;
use crate::agent::liveness::stuck_detection::{DetectionSnapshot, StuckDetector};
use crate::agent::tool_executor::task_is_awaiting_parent;
use crate::agent::types::StuckReason;
use crate::history::{GoalRunThreadRef, SubagentMetrics};

impl AgentEngine {
    /// A thread is *hibernated* when it deliberately ended its turn waiting
    /// for an external event: a running background operation (registered in
    /// `operation_wakeups`) or a scheduled timer wakeup (`timer_wakeups`).
    /// Hibernation is a designed pause, not a stall: the runtime will resume
    /// the thread when the operation completes or the wakeup fires.
    /// Stalled-turn recovery must never fire for these threads — doing so
    /// requeues work the thread intentionally deferred and the worker gets
    /// re-prompted with the full task description again and again.
    pub(in crate::agent) async fn thread_is_hibernated(&self, thread_id: &str) -> bool {
        {
            let wakeups = self.operation_wakeups.lock().await;
            if wakeups.values().any(|wakeup| wakeup.thread_id == thread_id) {
                return true;
            }
        }
        let wakeups = self.timer_wakeups.lock().await;
        wakeups.values().any(|wakeup| wakeup.thread_id == thread_id)
    }

    pub(super) async fn collect_stalled_turn_observations(&self) -> Vec<ThreadStallObservation> {
        let active_streams = {
            let streams = self.stream_cancellations.lock().await;
            streams
                .iter()
                .map(|(thread_id, entry)| (thread_id.clone(), entry.clone()))
                .collect::<Vec<_>>()
        };
        let active_stream_ids = active_streams
            .iter()
            .map(|(thread_id, _)| thread_id.clone())
            .collect::<HashSet<_>>();
        let threads = self.threads.read().await.clone();
        let task_thread_ids = threads.keys().cloned().collect::<Vec<_>>();
        let mut goal_runs = match self
            .history
            .list_goal_run_thread_refs_for_thread_ids(&task_thread_ids)
            .await
        {
            Ok(goal_runs) => VecDeque::from(goal_runs),
            Err(error) => {
                tracing::warn!(
                    "failed to query persisted goal runs for stalled-turn scan: {error}"
                );
                VecDeque::new()
            }
        };
        let mut seen_goal_ids = goal_runs
            .iter()
            .map(|goal_run| goal_run.id.clone())
            .collect::<HashSet<_>>();
        {
            let live_goal_runs = self.goal_runs.lock().await;
            for goal_run in live_goal_runs.iter() {
                let goal_run_ref = GoalRunThreadRef::from(goal_run);
                if !task_thread_ids
                    .iter()
                    .any(|thread_id| goal_run_matches_thread_ref(&goal_run_ref, thread_id))
                {
                    continue;
                }
                if seen_goal_ids.insert(goal_run_ref.id.clone()) {
                    goal_runs.push_back(goal_run_ref);
                }
            }
        }
        let now = now_millis();
        let recent_window_ms = self.stalled_turn_activity_window_ms().await;
        if recent_window_ms == 0 {
            return Vec::new();
        }
        let recent_cutoff = now.saturating_sub(recent_window_ms);
        let active_task_thread_ids = if task_thread_ids.is_empty() {
            Vec::new()
        } else {
            self.list_tasks_filtered_without_logs(&crate::history::AgentTaskListQuery {
                statuses: active_stalled_turn_task_statuses(),
                thread_ids: task_thread_ids,
                ..Default::default()
            })
            .await
            .into_iter()
            .filter_map(|task| task.thread_id)
            .map(|thread_id| thread_id.trim().to_string())
            .collect::<Vec<_>>()
        };
        let scan_thread_ids = threads
            .values()
            .filter(|thread| latest_thread_activity_at(thread) >= recent_cutoff)
            .map(|thread| thread.id.clone())
            .chain(active_stream_ids.iter().cloned())
            .chain(active_task_thread_ids)
            .filter(|thread_id| threads.contains_key(thread_id))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let tasks = if scan_thread_ids.is_empty() {
            VecDeque::new()
        } else {
            self.list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: stalled_turn_task_statuses(),
                source: None,
                thread_id: None,
                thread_ids: scan_thread_ids.clone(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: false,
                order_by_recent_activity_desc: true,
                limit: None,
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await
            .into_iter()
            // Children blocked on an open ask_parent are waiting on their
            // parent by design, not stalled — exempt them from recovery.
            .filter(|task| !task_is_awaiting_parent(task))
            .collect::<VecDeque<_>>()
        };
        let subagent_runtime = self.subagent_runtime.read().await.clone();
        let pending_operator_question_thread_ids =
            self.pending_operator_question_thread_ids().await;
        let unanswered_tool_call_thread_ids = match self
            .history
            .thread_ids_with_unanswered_tool_calls(&scan_thread_ids)
            .await
        {
            Ok(thread_ids) => Some(thread_ids.into_iter().collect::<HashSet<_>>()),
            Err(error) => {
                tracing::warn!(
                    "failed to query persisted unanswered tool call thread ids for stalled-turn scan; falling back to live thread scans: {error}"
                );
                None
            }
        };

        // Hibernated threads deliberately parked on a background operation or
        // a scheduled wakeup are not stalled — the runtime already owns their
        // resume path. Filter them out before any classification runs.
        let mut hibernated_thread_ids = HashSet::new();
        for thread_id in threads.keys() {
            if self.thread_is_hibernated(thread_id).await {
                hibernated_thread_ids.insert(thread_id.clone());
            }
        }

        let mut observations = threads
            .values()
            .filter(|thread| !active_stream_ids.contains(&thread.id))
            .filter(|thread| !hibernated_thread_ids.contains(&thread.id))
            .filter(|thread| !pending_operator_question_thread_ids.contains(&thread.id))
            .filter(|thread| {
                let in_db = unanswered_tool_call_thread_ids
                    .as_ref()
                    .map(|thread_ids| thread_ids.contains(&thread.id))
                    .unwrap_or(false);
                !(in_db || thread_has_unanswered_tool_calls(thread))
            })
            .filter(|thread| latest_thread_activity_at(thread) >= recent_cutoff)
            .filter_map(|thread| latest_stalled_turn_observation(thread, &tasks, &goal_runs))
            .collect::<Vec<_>>();

        observations.extend(active_streams.into_iter().filter_map(|(thread_id, entry)| {
            if pending_operator_question_thread_ids.contains(&thread_id) {
                return None;
            }
            if now.saturating_sub(entry.last_progress_at) < super::runtime::INITIAL_GRACE_DELAY_MS
                || matches!(entry.last_progress_kind, StreamProgressKind::Started)
            {
                return None;
            }
            let thread = threads.get(&thread_id)?;
            if terminal_work_owns_thread(thread.id.as_str(), &tasks, &goal_runs)
                && !active_work_owns_thread(thread.id.as_str(), &tasks, &goal_runs)
            {
                return None;
            }
            let in_db_unanswered = unanswered_tool_call_thread_ids
                .as_ref()
                .map(|thread_ids| thread_ids.contains(thread.id.as_str()))
                .unwrap_or(false);
            if in_db_unanswered || thread_has_unanswered_tool_calls(thread) {
                return None;
            }
            if latest_stream_activity_at(thread, &entry) < recent_cutoff {
                return None;
            }
            idle_stream_stall_observation(thread, &tasks, &goal_runs, &entry)
        }));

        let observed_ids = observations
            .iter()
            .map(|observation| observation.thread_id.clone())
            .collect::<HashSet<_>>();
        observations.extend(
            self.runtime_stall_observations(
                &threads,
                &tasks,
                &goal_runs,
                &subagent_runtime,
                now,
                recent_window_ms,
                &observed_ids,
                &pending_operator_question_thread_ids,
                unanswered_tool_call_thread_ids.as_ref(),
            )
            .await,
        );

        observations
    }

    async fn runtime_stall_observations(
        &self,
        threads: &HashMap<String, AgentThread>,
        tasks: &VecDeque<AgentTask>,
        goal_runs: &VecDeque<GoalRunThreadRef>,
        subagent_runtime: &HashMap<String, SubagentRuntimeStats>,
        now_ms: u64,
        recent_window_ms: u64,
        observed_ids: &HashSet<String>,
        pending_operator_question_thread_ids: &HashSet<String>,
        unanswered_tool_call_thread_ids: Option<&HashSet<String>>,
    ) -> Vec<ThreadStallObservation> {
        let mut observations = Vec::new();
        let mut seen_thread_ids = observed_ids.clone();
        let now_secs = now_ms / 1000;

        for task in tasks.iter().rev() {
            if !matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::AwaitingApproval
            ) {
                continue;
            }

            let Some(thread_id) = task.thread_id.as_deref() else {
                continue;
            };
            let thread_id = thread_id.trim();
            if thread_id.is_empty() || seen_thread_ids.contains(thread_id) {
                continue;
            }
            if pending_operator_question_thread_ids.contains(thread_id) {
                continue;
            }

            let Some(thread) = threads.get(thread_id) else {
                continue;
            };
            let in_db_unanswered = unanswered_tool_call_thread_ids
                .map(|thread_ids| thread_ids.contains(thread_id))
                .unwrap_or(false);
            if in_db_unanswered || thread_has_unanswered_tool_calls(thread) {
                continue;
            }

            let persisted_metrics = if subagent_runtime.contains_key(&task.id) {
                None
            } else {
                self.history
                    .get_subagent_metrics(&task.id)
                    .await
                    .ok()
                    .flatten()
            };
            let snapshot = build_task_detection_snapshot(
                task,
                thread,
                subagent_runtime,
                persisted_metrics.as_ref(),
            );
            let last_activity_at =
                latest_task_activity_at(task, thread, subagent_runtime, persisted_metrics.as_ref());
            if now_ms.saturating_sub(last_activity_at) > recent_window_ms {
                continue;
            }

            let mut detector = StuckDetector::default();
            if let Some(config) = task.supervisor_config.as_ref() {
                detector.no_progress_timeout_secs = config.stuck_timeout_secs;
            }

            let Some(analysis) = detector.analyze(&snapshot, now_secs) else {
                continue;
            };
            let class = match analysis.reason {
                StuckReason::ToolCallLoop => StalledTurnClass::ToolCallLoop,
                StuckReason::NoProgress => StalledTurnClass::NoProgress,
                _ => continue,
            };
            let class_label = match class {
                StalledTurnClass::ToolCallLoop => "tool_call_loop",
                StalledTurnClass::NoProgress => "no_progress",
                _ => unreachable!("runtime stall observations only emit runtime-derived classes"),
            };

            seen_thread_ids.insert(thread_id.to_string());
            observations.push(ThreadStallObservation {
                thread_id: thread_id.to_string(),
                last_message_id: format!("runtime:{}:{class_label}", task.id),
                last_message_at: last_activity_at,
                last_assistant_message: analysis.evidence,
                class,
                stream_progress_kind: None,
                task_id: Some(task.id.clone()),
                goal_run_id: goal_run_id_for_thread(thread_id, goal_runs),
            });
        }

        observations
    }

    async fn stalled_turn_activity_window_ms(&self) -> u64 {
        let window_hours = self
            .config
            .read()
            .await
            .participant_observer_restore_window_hours;
        (window_hours as u64).saturating_mul(60 * 60 * 1000)
    }
}

fn active_stalled_turn_task_statuses() -> Vec<String> {
    [
        TaskStatus::InProgress,
        TaskStatus::AwaitingApproval,
        TaskStatus::Blocked,
    ]
    .into_iter()
    .filter_map(|status| {
        serde_json::to_value(status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
    })
    .collect()
}

fn stalled_turn_task_statuses() -> Vec<String> {
    [
        TaskStatus::Queued,
        TaskStatus::InProgress,
        TaskStatus::AwaitingApproval,
        TaskStatus::Blocked,
        TaskStatus::FailedAnalyzing,
        TaskStatus::BudgetExceeded,
        TaskStatus::Completed,
        TaskStatus::Failed,
        TaskStatus::Cancelled,
    ]
    .into_iter()
    .filter_map(|status| {
        serde_json::to_value(status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
    })
    .collect()
}

fn latest_stalled_turn_observation(
    thread: &AgentThread,
    tasks: &VecDeque<AgentTask>,
    goal_runs: &VecDeque<GoalRunThreadRef>,
) -> Option<ThreadStallObservation> {
    if terminal_work_owns_thread(thread.id.as_str(), tasks, goal_runs)
        && !active_work_owns_thread(thread.id.as_str(), tasks, goal_runs)
    {
        return None;
    }

    let mut trailing = thread
        .messages
        .iter()
        .rev()
        .skip_while(|message| matches!(message.role, MessageRole::System));
    let last_message = trailing.next()?;
    if last_message.role != MessageRole::Assistant || last_message.tool_calls.is_some() {
        return None;
    }

    let preceded_by_tool_result = trailing
        .find(|message| !matches!(message.role, MessageRole::System))
        .map(|message| message.role == MessageRole::Tool)
        .unwrap_or(false);

    if last_message.content.trim().is_empty() && !preceded_by_tool_result {
        return None;
    }

    let evidence = TurnEvidence {
        last_assistant_message: last_message.content.clone(),
        preceded_by_tool_result,
        new_tool_call_followed: false,
        new_substantive_assistant_message_followed: false,
        task_or_goal_progressed: goal_progressed_after(
            thread.id.as_str(),
            last_message.timestamp,
            tasks,
            goal_runs,
        ),
        user_replied: false,
    };
    let class = classify_stalled_turn(&evidence)?;

    Some(ThreadStallObservation {
        thread_id: thread.id.clone(),
        last_message_id: last_message.id.clone(),
        last_message_at: last_message.timestamp,
        last_assistant_message: last_message.content.clone(),
        class,
        stream_progress_kind: None,
        task_id: active_task_id_for_thread(thread.id.as_str(), tasks),
        goal_run_id: goal_run_id_for_thread(thread.id.as_str(), goal_runs),
    })
}

fn idle_stream_stall_observation(
    thread: &AgentThread,
    tasks: &VecDeque<AgentTask>,
    goal_runs: &VecDeque<GoalRunThreadRef>,
    stream: &StreamCancellationEntry,
) -> Option<ThreadStallObservation> {
    Some(ThreadStallObservation {
        thread_id: thread.id.clone(),
        last_message_id: format!(
            "stream:{}:{}:{:?}",
            stream.generation, stream.last_progress_at, stream.last_progress_kind
        ),
        last_message_at: stream.last_progress_at,
        last_assistant_message: if stream.last_progress_excerpt.trim().is_empty() {
            "Stream went idle before completion.".to_string()
        } else {
            stream.last_progress_excerpt.clone()
        },
        class: StalledTurnClass::ActiveStreamIdle,
        stream_progress_kind: Some(stream.last_progress_kind),
        task_id: active_task_id_for_thread(thread.id.as_str(), tasks),
        goal_run_id: goal_run_id_for_thread(thread.id.as_str(), goal_runs),
    })
}

fn active_task_id_for_thread(thread_id: &str, tasks: &VecDeque<AgentTask>) -> Option<String> {
    tasks
        .iter()
        .rev()
        .find(|task| {
            task.thread_id.as_deref() == Some(thread_id)
                && matches!(
                    task.status,
                    TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::AwaitingApproval
                )
        })
        .map(|task| task.id.clone())
}

fn goal_run_id_for_thread(
    thread_id: &str,
    goal_runs: &VecDeque<GoalRunThreadRef>,
) -> Option<String> {
    goal_runs
        .iter()
        .find(|goal_run| goal_run_matches_thread_ref(goal_run, thread_id))
        .map(|goal_run| goal_run.id.clone())
}

fn active_work_owns_thread(
    thread_id: &str,
    tasks: &VecDeque<AgentTask>,
    goal_runs: &VecDeque<GoalRunThreadRef>,
) -> bool {
    tasks.iter().any(|task| {
        task.thread_id.as_deref() == Some(thread_id)
            && matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::AwaitingApproval
            )
    }) || goal_runs.iter().any(|goal_run| {
        goal_run_matches_thread_ref(goal_run, thread_id)
            && !goal_run_status_is_terminal(goal_run.status)
    })
}

pub(super) fn thread_has_unanswered_tool_calls(thread: &AgentThread) -> bool {
    let messages = &thread.messages;
    let mut index = 0usize;

    while index < messages.len() {
        let message = &messages[index];
        let Some(tool_calls) = message.tool_calls.as_ref() else {
            index += 1;
            continue;
        };
        if message.role != MessageRole::Assistant || tool_calls.is_empty() {
            index += 1;
            continue;
        }

        let expected_ids = tool_calls
            .iter()
            .map(|tool_call| tool_call.id.as_str())
            .collect::<HashSet<_>>();
        let mut matched_ids = HashSet::new();
        let mut result_index = index + 1;
        while result_index < messages.len() && messages[result_index].role == MessageRole::Tool {
            if let Some(tool_call_id) = messages[result_index].tool_call_id.as_deref() {
                if expected_ids.contains(tool_call_id) {
                    matched_ids.insert(tool_call_id);
                }
            }
            result_index += 1;
        }

        if matched_ids.len() != expected_ids.len() {
            return true;
        }
        index = result_index;
    }

    false
}

fn terminal_work_owns_thread(
    thread_id: &str,
    tasks: &VecDeque<AgentTask>,
    goal_runs: &VecDeque<GoalRunThreadRef>,
) -> bool {
    tasks.iter().any(|task| {
        task.thread_id.as_deref() == Some(thread_id)
            && crate::agent::task_scheduler::is_task_terminal_status(task.status)
    }) || goal_runs.iter().any(|goal_run| {
        goal_run_matches_thread_ref(goal_run, thread_id)
            && goal_run_status_is_terminal(goal_run.status)
    })
}

fn goal_run_matches_thread_ref(goal_run: &GoalRunThreadRef, thread_id: &str) -> bool {
    goal_run.thread_id.as_deref() == Some(thread_id)
        || goal_run.root_thread_id.as_deref() == Some(thread_id)
        || goal_run.active_thread_id.as_deref() == Some(thread_id)
        || goal_run
            .execution_thread_ids
            .iter()
            .any(|candidate| candidate == thread_id)
}

fn goal_run_status_is_terminal(status: GoalRunStatus) -> bool {
    matches!(
        status,
        GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled
    )
}

fn goal_progressed_after(
    thread_id: &str,
    message_timestamp: u64,
    tasks: &VecDeque<AgentTask>,
    goal_runs: &VecDeque<GoalRunThreadRef>,
) -> bool {
    tasks.iter().any(|task| {
        task.thread_id.as_deref() == Some(thread_id)
            && task_progressed_after(task, message_timestamp)
    }) || goal_runs.iter().any(|goal_run| {
        goal_run.thread_id.as_deref() == Some(thread_id) && goal_run.updated_at > message_timestamp
    })
}

fn task_progressed_after(task: &AgentTask, message_timestamp: u64) -> bool {
    let execution_started = matches!(task.status, TaskStatus::InProgress)
        && task.started_at.is_some_and(|at| at > message_timestamp);
    let lifecycle_advanced = task.completed_at.is_some_and(|at| at > message_timestamp)
        || task.scheduled_at.is_some_and(|at| at > message_timestamp)
        || task.next_retry_at.is_some_and(|at| at > message_timestamp);
    let logged_progress = task
        .logs
        .iter()
        .any(|entry| entry.timestamp > message_timestamp);

    execution_started || lifecycle_advanced || logged_progress
}

fn latest_thread_activity_at(thread: &AgentThread) -> u64 {
    thread
        .messages
        .last()
        .map(|message| message.timestamp)
        .unwrap_or(thread.updated_at)
        .max(thread.updated_at)
}

fn latest_thread_message_at(thread: &AgentThread) -> u64 {
    thread
        .messages
        .last()
        .map(|message| message.timestamp)
        .unwrap_or(thread.updated_at)
}

fn latest_stream_activity_at(thread: &AgentThread, stream: &StreamCancellationEntry) -> u64 {
    latest_thread_activity_at(thread).max(stream.last_progress_at)
}

fn latest_runtime_activity_at(thread: &AgentThread, stats: &SubagentRuntimeStats) -> u64 {
    latest_thread_activity_at(thread)
        .max(stats.updated_at)
        .max(stats.last_tool_call_at.unwrap_or(0))
        .max(stats.last_progress_at.unwrap_or(0))
        .max(stats.started_at)
}

fn latest_task_activity_at(
    task: &AgentTask,
    thread: &AgentThread,
    subagent_runtime: &HashMap<String, SubagentRuntimeStats>,
    persisted_metrics: Option<&SubagentMetrics>,
) -> u64 {
    let base_task_activity = latest_thread_activity_at(thread)
        .max(task.started_at.unwrap_or(task.created_at))
        .max(task.created_at);

    let live_activity = subagent_runtime
        .get(&task.id)
        .map(|stats| latest_runtime_activity_at(thread, stats))
        .unwrap_or(0);

    let persisted_activity = persisted_metrics
        .map(|metrics| {
            latest_thread_activity_at(thread)
                .max(metrics.updated_at)
                .max(metrics.last_progress_at.unwrap_or(0))
                .max(task.started_at.unwrap_or(task.created_at))
        })
        .unwrap_or(0);

    base_task_activity
        .max(live_activity)
        .max(persisted_activity)
}

fn build_task_detection_snapshot(
    task: &AgentTask,
    thread: &AgentThread,
    subagent_runtime: &HashMap<String, SubagentRuntimeStats>,
    persisted_metrics: Option<&SubagentMetrics>,
) -> DetectionSnapshot {
    let live_stats = subagent_runtime.get(&task.id);
    let fallback_last_progress_at = latest_thread_message_at(thread);
    let total_tool_calls = live_stats
        .map(|stats| stats.tool_calls_total)
        .or_else(|| {
            persisted_metrics
                .map(|metrics| metrics.tool_calls_total.max(0).min(u32::MAX as i64) as u32)
        })
        .unwrap_or(0);
    let total_errors = live_stats
        .map(|stats| stats.tool_calls_failed)
        .or_else(|| {
            persisted_metrics
                .map(|metrics| metrics.tool_calls_failed.max(0).min(u32::MAX as i64) as u32)
        })
        .unwrap_or(0);
    let context_utilization_pct = live_stats
        .map(|stats| stats.context_utilization_pct)
        .or_else(|| {
            persisted_metrics.and_then(|metrics| {
                task.context_budget_tokens.and_then(|budget| {
                    if budget == 0 {
                        Some(0)
                    } else {
                        Some(
                            (((metrics.tokens_consumed.max(0) as u64) * 100) / budget as u64)
                                .min(100) as u32,
                        )
                    }
                })
            })
        })
        .unwrap_or(0);

    DetectionSnapshot {
        entity_id: task.id.clone(),
        entity_type: "task".to_string(),
        last_progress_at: if let Some(stats) = live_stats {
            stats
                .last_progress_at
                .or(Some(fallback_last_progress_at))
                .map(|timestamp| timestamp / 1000)
        } else {
            persisted_metrics
                .and_then(|metrics| metrics.last_progress_at.map(|ts| ts / 1000))
                .or(Some(fallback_last_progress_at / 1000))
        },
        started_at: live_stats
            .map(|stats| stats.started_at / 1000)
            .unwrap_or_else(|| task.started_at.unwrap_or(task.created_at) / 1000),
        max_duration_secs: live_stats
            .and_then(|stats| stats.max_duration_secs)
            .or(task.max_duration_secs),
        consecutive_errors: live_stats
            .map(|stats| stats.consecutive_errors)
            .unwrap_or(0),
        total_errors,
        total_tool_calls,
        recent_tool_names: live_stats
            .map(|stats| stats.recent_tool_names.iter().cloned().collect())
            .unwrap_or_default(),
        context_utilization_pct,
    }
}
