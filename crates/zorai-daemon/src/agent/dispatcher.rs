//! Task and goal dispatching — background execution scheduling.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoverableGoalPauseReason {
    ProviderCredits,
    RateLimit,
    TemporaryProvider,
    Transport,
    AuthConfiguration,
}

impl RecoverableGoalPauseReason {
    fn notification_kind(self) -> &'static str {
        match self {
            Self::ProviderCredits => "goal_provider_credits",
            Self::RateLimit => "goal_provider_rate_limit",
            Self::TemporaryProvider => "goal_provider_temporary",
            Self::Transport => "goal_provider_transport",
            Self::AuthConfiguration => "goal_provider_auth",
        }
    }

    fn operator_label(self) -> &'static str {
        match self {
            Self::ProviderCredits => "provider credits or billing need attention",
            Self::RateLimit => "provider rate limit",
            Self::TemporaryProvider => "temporary provider outage",
            Self::Transport => "network transport problem",
            Self::AuthConfiguration => "provider authentication or API key",
        }
    }
}

fn recoverable_goal_pause_reason(message: &str) -> Option<RecoverableGoalPauseReason> {
    if let Some(structured) = crate::agent::llm_client::parse_structured_upstream_failure(message) {
        let diagnostic_text = structured.diagnostics.to_string().to_ascii_lowercase();
        let summary = structured.summary.to_ascii_lowercase();
        let combined = format!("{summary}\n{diagnostic_text}");
        if mentions_provider_credit_problem(&combined) {
            return Some(RecoverableGoalPauseReason::ProviderCredits);
        }
        return match structured.class.as_str() {
            "rate_limit" => Some(RecoverableGoalPauseReason::RateLimit),
            "temporary_upstream" => Some(RecoverableGoalPauseReason::TemporaryProvider),
            "transient_transport" => Some(RecoverableGoalPauseReason::Transport),
            "auth_configuration" => Some(RecoverableGoalPauseReason::AuthConfiguration),
            _ => None,
        };
    }

    let lower = message.to_ascii_lowercase();
    if mentions_provider_credit_problem(&lower) {
        return Some(RecoverableGoalPauseReason::ProviderCredits);
    }
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("too many requests")
    {
        return Some(RecoverableGoalPauseReason::RateLimit);
    }
    if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("transport error")
        || lower.contains("error sending request")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("broken pipe")
        || lower.contains("unexpected eof")
        || lower.contains("network")
    {
        return Some(RecoverableGoalPauseReason::Transport);
    }
    if lower.contains("overloaded")
        || lower.contains("temporarily unavailable")
        || lower.contains("try again later")
        || lower.contains("503")
        || lower.contains("502")
    {
        return Some(RecoverableGoalPauseReason::TemporaryProvider);
    }
    None
}

fn mentions_provider_credit_problem(lower: &str) -> bool {
    (lower.contains("openrouter")
        && (lower.contains("credit")
            || lower.contains("credits")
            || lower.contains("billing")
            || lower.contains("payment")))
        || lower.contains("insufficient credits")
        || lower.contains("insufficient credit")
        || lower.contains("quota exceeded")
        || lower.contains("payment required")
        || lower.contains("billing hard limit")
}

fn terminal_task_notification_excerpt(value: Option<&str>) -> Option<String> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }

    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }

    let mut excerpt = collapsed.chars().take(240).collect::<String>();
    if collapsed.chars().count() > 240 {
        excerpt.push('…');
    }
    Some(excerpt)
}

fn terminal_task_notification_id(task: &AgentTask, kind: &str) -> String {
    if task.source == "event_trigger" {
        use sha2::Digest;
        let digest = format!("{:x}", sha2::Sha256::digest(task.title.as_bytes()));
        format!("task-terminal:trigger:{}:{kind}", &digest[..16])
    } else {
        format!("task-terminal:{}:{kind}", task.id)
    }
}

fn append_subagent_outcome_log_to_parent(
    parent: &mut AgentTask,
    child_task: &AgentTask,
    level: TaskLogLevel,
    message: &str,
    details: Option<&str>,
) {
    let detail_suffix = details
        .map(|value| format!("; {value}"))
        .unwrap_or_default();
    parent.logs.push(make_task_log_entry(
        child_task.retry_count,
        level,
        "subagent",
        &format!(
            "{}: {} ({}){}",
            message, child_task.title, child_task.id, detail_suffix
        ),
        Some(format!(
            "runtime={} status={} thread_id={} session_id={}",
            child_task.runtime,
            serde_json::to_string(&child_task.status).unwrap_or_else(|_| "unknown".to_string()),
            child_task.thread_id.as_deref().unwrap_or("-"),
            child_task.session_id.as_deref().unwrap_or("-"),
        )),
    ));
}

fn apply_dispatched_task_success_update(
    task: &mut AgentTask,
    outcome: &SendMessageOutcome,
    active_child_ids: &[String],
    now: u64,
) {
    if is_task_terminal_status(task.status) {
        return;
    }
    if outcome.interrupted_for_approval
        || crate::agent::tool_executor::task_is_awaiting_parent(task)
    {
        return;
    }
    let waiting_for_subagents = !active_child_ids.is_empty();
    let budget_exceeded_reason = "execution budget exceeded for this thread".to_string();
    if let Some(report) = outcome.subagent_report.as_ref() {
        task.result = Some(report.summary.clone());
        if report.status == SubagentReportStatus::Done {
            if let Some(contract) = task.completion_contract.as_mut() {
                contract.satisfy_requirement(
                    crate::agent::types::SUBAGENT_REPORT_REQUIREMENT_DESCRIPTION,
                    format!("reported summary: {}", report.summary),
                );
                contract
                    .completed_actions
                    .push("reported usable subagent outcome".into());
            }
        }
        match report.status {
            SubagentReportStatus::Done if waiting_for_subagents => {
                task.status = TaskStatus::Blocked;
                task.progress = task.progress.max(90);
                task.completed_at = None;
                task.blocked_reason = Some(format!(
                    "waiting for subagents: {}",
                    active_child_ids.join(", ")
                ));
                task.error = None;
                task.last_error = None;
            }
            SubagentReportStatus::Done => {
                apply_successful_completion_transition(task, now);
            }
            SubagentReportStatus::Cancelled => {
                task.status = TaskStatus::Cancelled;
                task.progress = 100;
                task.completed_at = Some(now);
                task.blocked_reason = Some(report.summary.clone());
                task.error = None;
                task.last_error = None;
            }
            SubagentReportStatus::Error if report.reason.as_deref() == Some("zorai_budget") => {
                task.status = TaskStatus::BudgetExceeded;
                task.progress = 100;
                task.completed_at = Some(now);
                task.blocked_reason = Some(budget_exceeded_reason.clone());
                task.error = Some(budget_exceeded_reason.clone());
                task.last_error = Some(budget_exceeded_reason);
            }
            SubagentReportStatus::Error => {
                task.status = TaskStatus::Failed;
                task.progress = 100;
                task.completed_at = Some(now);
                task.blocked_reason = Some(report.summary.clone());
                task.error = Some(report.summary.clone());
                task.last_error = Some(report.summary.clone());
            }
        }
        task.thread_id = Some(outcome.thread_id.clone());
        task.lane_id = None;
        task.awaiting_approval_id = None;
        task.next_retry_at = None;
        task.logs.push(make_task_log_entry(
            task.retry_count,
            TaskLogLevel::Info,
            "report",
            match report.status {
                SubagentReportStatus::Done if waiting_for_subagents => {
                    "task waiting for spawned subagents to finish"
                }
                SubagentReportStatus::Done => "subagent reported completion",
                SubagentReportStatus::Cancelled => "subagent reported cancellation",
                SubagentReportStatus::Error if report.reason.as_deref() == Some("zorai_budget") => {
                    "subagent reported after exhausting execution budget"
                }
                SubagentReportStatus::Error => "subagent reported an error",
            },
            Some(report.summary.clone()),
        ));
        record_goal_step_dispatch_finish(task);
        return;
    }
    if outcome.terminated_for_budget {
        let _ = task.transition_to_terminal(TaskStatus::BudgetExceeded, now);
    } else if waiting_for_subagents {
        task.status = TaskStatus::Blocked;
    } else {
        apply_successful_completion_transition(task, now);
    }
    let completion_blocked = !waiting_for_subagents
        && matches!(
            task.status,
            TaskStatus::Blocked | TaskStatus::AwaitingApproval
        );
    task.progress = if waiting_for_subagents {
        task.progress.max(90)
    } else if completion_blocked {
        task.progress.min(99)
    } else {
        100
    };
    if waiting_for_subagents {
        task.completed_at = None;
    }
    task.thread_id = Some(outcome.thread_id.clone());
    task.lane_id = None;
    task.blocked_reason = if outcome.terminated_for_budget {
        Some(budget_exceeded_reason.clone())
    } else if waiting_for_subagents {
        Some(format!(
            "waiting for subagents: {}",
            active_child_ids.join(", ")
        ))
    } else if completion_blocked {
        task.blocked_reason.clone()
    } else {
        None
    };
    if !matches!(task.status, TaskStatus::AwaitingApproval) {
        task.awaiting_approval_id = None;
    }
    task.error = if outcome.terminated_for_budget {
        Some(budget_exceeded_reason.clone())
    } else {
        None
    };
    task.last_error = if outcome.terminated_for_budget {
        Some(budget_exceeded_reason.clone())
    } else {
        None
    };
    task.next_retry_at = None;
    task.logs.push(make_task_log_entry(
        task.retry_count,
        TaskLogLevel::Info,
        if waiting_for_subagents {
            "subagent"
        } else {
            "execution"
        },
        if waiting_for_subagents {
            "task waiting for spawned subagents to finish"
        } else if outcome.terminated_for_budget {
            "task stopped after exhausting execution budget"
        } else if completion_blocked {
            "task produced progress but completion contract remains open"
        } else if task.retry_count > 0 {
            "task self-healed and completed"
        } else {
            "task completed"
        },
        if waiting_for_subagents || completion_blocked {
            task.blocked_reason.clone()
        } else {
            None
        },
    ));
    record_goal_step_dispatch_finish(task);
}

fn record_goal_step_dispatch_finish(task: &mut AgentTask) {
    if task.source != "goal_run" || is_task_terminal_status(task.status) {
        return;
    }
    if matches!(task.status, TaskStatus::AwaitingApproval) {
        return;
    }
    let attempts = {
        let contract = task
            .completion_contract
            .get_or_insert_with(TaskCompletionContract::default);
        contract.dispatch_finish_attempts = contract.dispatch_finish_attempts.saturating_add(1);
        contract.dispatch_finish_attempts
    };
    if attempts < GOAL_STEP_DISPATCH_FINISH_LIMIT {
        return;
    }
    task.status = TaskStatus::AwaitingApproval;
    task.progress = task.progress.max(90).min(99);
    task.completed_at = None;
    task.awaiting_approval_id = Some(format!("goal-step-dispatch-budget:{}", task.id));
    task.blocked_reason = Some(format!(
        "goal step exceeded {attempts} dispatch finishes without completing"
    ));
    task.logs.push(make_task_log_entry(
        task.retry_count,
        TaskLogLevel::Warn,
        "execution",
        "goal step dispatch budget exhausted",
        task.blocked_reason.clone(),
    ));
}

fn apply_successful_completion_transition(task: &mut AgentTask, now: u64) {
    match task.transition_to_terminal(TaskStatus::Completed, now) {
        Ok(()) => {
            task.blocked_reason = None;
            task.error = None;
            task.last_error = None;
            if let Some(contract) = task.completion_contract.as_mut() {
                contract.open_completion_attempts = 0;
                contract.dispatch_finish_attempts = 0;
            }
        }
        Err(reasons) => {
            let attempts = if let Some(contract) = task.completion_contract.as_mut() {
                contract.open_completion_attempts =
                    contract.open_completion_attempts.saturating_add(1);
                contract.open_completion_attempts
            } else {
                1
            };
            let detail = format!("completion contract remains open: {}", reasons.join("; "));
            if attempts >= OPEN_COMPLETION_CONTRACT_ATTEMPT_LIMIT {
                task.status = TaskStatus::AwaitingApproval;
                task.progress = task.progress.max(90).min(99);
                task.completed_at = None;
                task.awaiting_approval_id = Some(format!("open-completion-contract:{}", task.id));
                task.blocked_reason = Some(format!(
                    "completion contract remains open after {attempts} attempts: {}",
                    reasons.join("; ")
                ));
            } else {
                task.status = TaskStatus::Blocked;
                task.progress = task.progress.max(90).min(99);
                task.completed_at = None;
                task.blocked_reason = Some(detail);
            }
            task.error = None;
            task.last_error = None;
        }
    }
}

fn apply_dispatched_task_failure_update(
    task: &mut AgentTask,
    error_text: &str,
    retry_delay_ms: u64,
) {
    if is_task_terminal_status(task.status) {
        return;
    }
    task.retry_count = task.retry_count.saturating_add(1);
    task.error = Some(error_text.to_string());
    task.last_error = Some(error_text.to_string());
    task.progress = 0;
    task.lane_id = None;
    task.logs.push(make_task_log_entry(
        task.retry_count,
        TaskLogLevel::Error,
        "execution",
        "task execution failed",
        Some(error_text.to_string()),
    ));

    if recoverable_goal_pause_reason(error_text).is_none() && task.retry_count <= task.max_retries {
        task.status = TaskStatus::FailedAnalyzing;
        task.completed_at = None;
        task.next_retry_at = Some(now_millis().saturating_add(retry_delay_ms));
        task.blocked_reason = Some(format!(
            "retry {} of {} scheduled in {}s",
            task.retry_count,
            task.max_retries,
            retry_delay_ms.div_ceil(1000).max(1),
        ));
        task.logs.push(make_task_log_entry(
            task.retry_count,
            TaskLogLevel::Warn,
            "analysis",
            "agent queued self-healing retry",
            task.blocked_reason.clone(),
        ));
    } else {
        task.status = TaskStatus::Failed;
        task.completed_at = Some(now_millis());
        task.next_retry_at = None;
        task.blocked_reason = Some("retry budget exhausted".into());
        task.logs.push(make_task_log_entry(
            task.retry_count,
            TaskLogLevel::Error,
            "analysis",
            "task failed permanently after exhausting retry budget",
            Some(error_text.to_string()),
        ));
    }
}

fn apply_dispatched_subagent_provider_quota_failure(
    task: &mut AgentTask,
    error_text: &str,
    summary: Option<&str>,
) {
    let summary = summary
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("(no usable assistant summary was recorded before the provider quota error)");
    task.status = TaskStatus::Failed;
    task.progress = 100;
    task.completed_at = Some(now_millis());
    task.next_retry_at = None;
    task.lane_id = None;
    task.result = Some(summary.to_string());
    task.error = Some(error_text.to_string());
    task.last_error = Some(error_text.to_string());
    task.blocked_reason = Some("provider quota or billing limit".to_string());
    task.logs.push(make_task_log_entry(
        task.retry_count,
        TaskLogLevel::Error,
        "report",
        "subagent stopped by a provider quota independent of the zorai execution budget",
        Some(format!("{error_text}\n\n{summary}")),
    ));
}

impl AgentEngine {
    pub(in crate::agent) async fn task_by_id_for_dispatcher(
        &self,
        task_id: &str,
    ) -> Option<AgentTask> {
        match self
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
            None => {
                let tasks = self.tasks.lock().await;
                tasks.iter().find(|task| task.id == task_id).cloned()
            }
        }
    }

    async fn replay_pending_child_parent_notifications(&self) {
        let children = self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: vec![
                    "completed".to_string(),
                    "failed".to_string(),
                    "cancelled".to_string(),
                    "budget_exceeded".to_string(),
                ],
                source: Some("subagent".to_string()),
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: false,
                order_by_recent_activity_desc: true,
                limit: Some(64),
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await;
        for child in children {
            let pending = child
                .completion_contract
                .as_ref()
                .and_then(|contract| contract.child_result.as_ref())
                .is_some_and(|result| {
                    result.parent_notification
                        == crate::agent::types::ParentNotificationState::Pending
                        || (result.parent_notification
                            == crate::agent::types::ParentNotificationState::Delivered
                            && result.integration_acknowledged_at.is_none())
                });
            if pending {
                if let Some(thread_id) = child.parent_thread_id.as_deref() {
                    if self.operator_stream_stop_requested(thread_id).await {
                        self.mark_child_result_integrated(
                            &child.id,
                            child.parent_task_id.as_deref(),
                        )
                        .await;
                        continue;
                    }
                }
                let (level, message) = match child.status {
                    TaskStatus::Completed => (TaskLogLevel::Info, "subagent completed"),
                    TaskStatus::BudgetExceeded => (TaskLogLevel::Warn, "subagent budget exceeded"),
                    _ => (TaskLogLevel::Error, "subagent failed"),
                };
                self.record_subagent_outcome_on_parent(
                    &child,
                    level,
                    message,
                    child.result.clone().or(child.last_error.clone()),
                )
                .await;
            }
        }
    }

    async fn active_subagent_child_ids_for_dispatcher(&self, task_id: &str) -> Vec<String> {
        let mut active_child_ids = self
            .history
            .list_agent_task_ids_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: Some("subagent".to_string()),
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: Some(task_id.to_string()),
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: true,
                order_by_recent_activity_desc: false,
                limit: None,
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(
                    parent_task_id = task_id,
                    "failed to query active subagent child ids for dispatcher: {error}"
                );
                Vec::new()
            });

        let tasks = self.tasks.lock().await;
        for task in tasks.iter().filter(|entry| {
            entry.source == "subagent"
                && entry.parent_task_id.as_deref() == Some(task_id)
                && !is_task_terminal_status(entry.status)
        }) {
            if !active_child_ids.iter().any(|child_id| child_id == &task.id) {
                active_child_ids.push(task.id.clone());
            }
        }
        active_child_ids
    }

    async fn active_subagent_child_tasks_for_dispatcher(&self, task_id: &str) -> Vec<AgentTask> {
        let mut active_children = self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: Some("subagent".to_string()),
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: Some(task_id.to_string()),
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: true,
                order_by_recent_activity_desc: false,
                limit: None,
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await
            .into_iter()
            .collect::<Vec<_>>();

        let tasks = self.tasks.lock().await;
        for task in tasks.iter().filter(|entry| {
            entry.source == "subagent"
                && entry.parent_task_id.as_deref() == Some(task_id)
                && !is_task_terminal_status(entry.status)
        }) {
            if !active_children.iter().any(|child| child.id == task.id) {
                active_children.push(task.clone());
            }
        }
        active_children
    }

    pub(crate) async fn notify_task_terminal_state(&self, task: &AgentTask) {
        if !task.notify_on_complete {
            return;
        }

        let (kind, title, subtitle, severity, detail_line) = match task.status {
            TaskStatus::Completed => (
                "task_completed",
                format!("Task completed: {}", task.title),
                "completed",
                NotificationSeverity::Info,
                terminal_task_notification_excerpt(task.result.as_deref())
                    .map(|result| format!("Result: {result}")),
            ),
            TaskStatus::Failed => (
                "task_failed",
                format!("Task failed: {}", task.title),
                "failed",
                NotificationSeverity::Error,
                terminal_task_notification_excerpt(
                    task.last_error.as_deref().or(task.error.as_deref()),
                )
                .map(|error| format!("Error: {error}"))
                .or_else(|| {
                    terminal_task_notification_excerpt(task.blocked_reason.as_deref())
                        .map(|reason| format!("Reason: {reason}"))
                }),
            ),
            TaskStatus::BudgetExceeded => (
                "task_budget_exceeded",
                format!("Task budget exceeded: {}", task.title),
                "budget exceeded",
                NotificationSeverity::Warning,
                terminal_task_notification_excerpt(
                    task.blocked_reason
                        .as_deref()
                        .or(task.last_error.as_deref()),
                )
                .map(|reason| format!("Reason: {reason}")),
            ),
            _ => return,
        };

        let mut seen_channels = HashSet::new();
        let mut channels = task
            .notify_channels
            .iter()
            .map(|channel| channel.trim().to_ascii_lowercase())
            .filter(|channel| !channel.is_empty())
            .filter(|channel| seen_channels.insert(channel.clone()))
            .collect::<Vec<_>>();
        if channels.is_empty() {
            channels.push("in-app".to_string());
        }

        let mut body_lines = vec![
            format!("Task: {}", task.title),
            format!("Status: {subtitle}"),
            format!("Task ID: {}", task.id),
            format!("Source: {}", task.source),
            format!("Runtime: {}", task.runtime),
        ];
        if let Some(goal_run_title) = task.goal_run_title.as_deref() {
            body_lines.push(format!("Goal: {goal_run_title}"));
        }
        if let Some(goal_step_title) = task.goal_step_title.as_deref() {
            body_lines.push(format!("Goal step: {goal_step_title}"));
        }
        if let Some(thread_id) = task.thread_id.as_deref() {
            body_lines.push(format!("Thread: {thread_id}"));
        }
        if let Some(detail_line) = detail_line {
            body_lines.push(detail_line);
        }
        let body = body_lines.join("\n");

        let _ = self.event_tx.send(AgentEvent::Notification {
            title: title.clone(),
            body: body.clone(),
            severity,
            channels: channels.clone(),
        });

        let now = now_millis() as i64;
        let actions = task
            .thread_id
            .as_deref()
            .map(crate::notifications::open_thread_action)
            .into_iter()
            .collect::<Vec<_>>();
        let notification = zorai_protocol::InboxNotification {
            id: terminal_task_notification_id(task, kind),
            source: "task".to_string(),
            kind: kind.to_string(),
            title: title.clone(),
            body: body.clone(),
            subtitle: Some(subtitle.to_string()),
            severity: match severity {
                NotificationSeverity::Info => "info",
                NotificationSeverity::Warning => "warning",
                NotificationSeverity::Alert => "alert",
                NotificationSeverity::Error => "error",
            }
            .to_string(),
            created_at: now,
            updated_at: now,
            read_at: None,
            archived_at: None,
            deleted_at: None,
            actions,
            metadata_json: Some(
                serde_json::json!({
                    "task_id": task.id.clone(),
                    "status": subtitle,
                    "source": task.source.clone(),
                    "thread_id": task.thread_id.clone(),
                    "goal_run_id": task.goal_run_id.clone(),
                    "goal_step_id": task.goal_step_id.clone(),
                    "notify_channels": channels,
                })
                .to_string(),
            ),
        };
        if let Err(error) = self.upsert_inbox_notification(notification).await {
            tracing::warn!(task_id = %task.id, %error, "failed to upsert task terminal notification");
        }

        let outbound_message = format!("{title}\n\n{body}");
        let mut sent_gateway_channels = HashSet::new();
        for channel in task
            .notify_channels
            .iter()
            .map(|channel| channel.trim().to_ascii_lowercase())
            .filter(|channel| !channel.is_empty() && channel != "in-app")
            .filter(|channel| sent_gateway_channels.insert(channel.clone()))
        {
            let tool_name = match channel.as_str() {
                "slack" => Some(zorai_protocol::tool_names::SEND_SLACK_MESSAGE),
                "discord" => Some(zorai_protocol::tool_names::SEND_DISCORD_MESSAGE),
                "telegram" => Some(zorai_protocol::tool_names::SEND_TELEGRAM_MESSAGE),
                "whatsapp" => Some(zorai_protocol::tool_names::SEND_WHATSAPP_MESSAGE),
                other => {
                    tracing::warn!(task_id = %task.id, channel = %other, "unknown task notification channel");
                    None
                }
            };
            let Some(tool_name) = tool_name else {
                continue;
            };

            if let Err(error) = crate::agent::tool_executor::execute_gateway_message(
                tool_name,
                &serde_json::json!({ "message": outbound_message }),
                self,
                &self.http_client,
            )
            .await
            {
                tracing::warn!(
                    task_id = %task.id,
                    channel = %channel,
                    %error,
                    "failed to deliver task terminal notification"
                );
            }
        }
    }

    pub(super) async fn dispatch_goal_runs(self: Arc<Self>) {
        let goal_run_ids = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .filter(|goal_run| {
                    !matches!(
                        goal_run.status,
                        GoalRunStatus::AwaitingApproval
                            | GoalRunStatus::AwaitingReview
                            | GoalRunStatus::Planning
                            | GoalRunStatus::Paused
                            | GoalRunStatus::Completed
                            | GoalRunStatus::Failed
                            | GoalRunStatus::Cancelled
                    )
                })
                .map(|goal_run| goal_run.id.clone())
                .collect::<Vec<_>>()
        };

        let mut workers = Vec::new();
        for goal_run_id in goal_run_ids {
            if !self.try_begin_goal_run_work(&goal_run_id).await {
                continue;
            }

            let engine = self.clone();
            workers.push(tokio::spawn(async move {
                let result = engine.advance_goal_run(&goal_run_id).await;
                if let Err(error) = result {
                    tracing::error!(goal_run_id = %goal_run_id, error = %error, "goal run advancement failed");
                    if !engine
                        .pause_goal_run_for_recoverable_provider_error(&goal_run_id, &error)
                        .await
                    {
                        engine
                            .fail_goal_run(&goal_run_id, &error.to_string(), "goal-run", None)
                            .await;
                    }
                }
                engine.finish_goal_run_work(&goal_run_id).await;
            }));
        }
        for worker in workers {
            let _ = worker.await;
        }
    }

    pub(in crate::agent) async fn pause_goal_run_for_recoverable_provider_error(
        &self,
        goal_run_id: &str,
        error: &anyhow::Error,
    ) -> bool {
        let Some(reason) = recoverable_goal_pause_reason(&error.to_string()) else {
            return false;
        };

        let message = format!(
            "Goal paused after a recoverable provider issue: {}. Resolve it, then resume the goal.",
            reason.operator_label()
        );
        let error_text = error.to_string();
        let mut notification = None;
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                return false;
            };
            if goal_run.status.is_terminal() {
                return false;
            }
            goal_run.status = GoalRunStatus::Paused;
            goal_run.updated_at = now_millis();
            goal_run.completed_at = None;
            goal_run.last_error = Some(error_text.clone());
            goal_run.failure_cause = None;
            goal_run.events.push(make_goal_run_event(
                "provider_recovery",
                "goal run paused after recoverable provider issue",
                Some(message.clone()),
            ));
            if let Some(thread_id) = goal_run.thread_id.as_deref() {
                let now = now_millis() as i64;
                notification = Some(zorai_protocol::InboxNotification {
                    id: format!("goal-provider-recovery:{goal_run_id}"),
                    source: "goal_runner".to_string(),
                    kind: reason.notification_kind().to_string(),
                    title: "Goal paused".to_string(),
                    body: format!(
                        "{}\n\nGoal: {}\n\nAfter fixing the provider or network issue, resume this goal.",
                        message, goal_run.title
                    ),
                    subtitle: Some(reason.operator_label().to_string()),
                    severity: "warning".to_string(),
                    created_at: now,
                    updated_at: now,
                    read_at: None,
                    archived_at: None,
                    deleted_at: None,
                    actions: vec![crate::notifications::open_thread_action(thread_id)],
                    metadata_json: Some(
                        serde_json::json!({
                            "goal_run_id": goal_run_id,
                            "reason": reason.notification_kind(),
                        })
                        .to_string(),
                    ),
                });
            }
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.pause_goal_tasks(&updated).await;
        self.quiesce_goal_execution_tree(&updated, false).await;
        crate::governance::record_transition_audit(
            &self.history,
            crate::governance::TransitionKind::LaneRetry,
            crate::governance::TransitionAuditIds {
                goal_run_id: Some(goal_run_id.to_string()),
                thread_id: updated.thread_id.clone(),
                ..Default::default()
            },
            serde_json::json!({
                "pause_reason": reason.notification_kind(),
                "operator_label": reason.operator_label(),
                "error": error_text,
            }),
            "paused_for_retry",
        )
        .await;
        self.emit_goal_run_update(&updated, Some(message.clone()));
        if let Some(notification) = notification {
            if let Err(error) = self.upsert_inbox_notification(notification).await {
                tracing::warn!(goal_run_id, %error, "failed to upsert goal pause notification");
            }
        }
        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: updated.thread_id.clone().unwrap_or_default(),
            kind: reason.notification_kind().to_string(),
            message,
            details: Some(
                serde_json::json!({
                    "goal_run_id": goal_run_id,
                    "error": error_text,
                })
                .to_string(),
            ),
        });
        true
    }

    async fn try_begin_goal_run_work(&self, goal_run_id: &str) -> bool {
        let mut inflight = self.inflight_goal_runs.lock().await;
        inflight.insert(goal_run_id.to_string())
    }

    async fn finish_goal_run_work(&self, goal_run_id: &str) {
        self.inflight_goal_runs.lock().await.remove(goal_run_id);
    }

    async fn advance_goal_run(&self, goal_run_id: &str) -> Result<()> {
        let goal_run = match self.get_goal_run(goal_run_id).await {
            Some(goal_run) => goal_run,
            None => return Ok(()),
        };

        if goal_run.status.is_terminal()
            || matches!(
                goal_run.status,
                GoalRunStatus::AwaitingApproval
                    | GoalRunStatus::AwaitingReview
                    | GoalRunStatus::Planning
                    | GoalRunStatus::Paused
                    | GoalRunStatus::Blocked
            )
        {
            return Ok(());
        }

        if self.retire_legacy_goal_orchestrator_run(&goal_run).await {
            return Ok(());
        }

        if goal_run.active_task_id.is_none() {
            self.enqueue_goal_worker(goal_run_id).await?;
            return Ok(());
        }

        let task_id = goal_run.active_task_id.as_deref().unwrap_or_default();
        let task = self.task_by_id_for_dispatcher(task_id).await;

        let Some(task) = task else {
            self.enqueue_goal_worker(goal_run_id).await?;
            return Ok(());
        };

        match task.status {
            TaskStatus::Queued | TaskStatus::InProgress => {}
            TaskStatus::Blocked => {
                if task
                    .blocked_reason
                    .as_deref()
                    .is_some_and(|reason| reason.starts_with(AWAITING_SUPERVISOR_BLOCKED_PREFIX))
                {
                    return Ok(());
                }
            }
            TaskStatus::AwaitingApproval => {}
            TaskStatus::Completed | TaskStatus::BudgetExceeded => {
                self.nudge_goal_worker_for_review(goal_run_id, &task)
                    .await?;
            }
            TaskStatus::Failed | TaskStatus::Cancelled => {
                self.fail_goal_run(
                    goal_run_id,
                    task.last_error
                        .as_deref()
                        .or(task.error.as_deref())
                        .unwrap_or("goal worker failed"),
                    "goal-worker",
                    task.thread_id.clone(),
                )
                .await;
            }
            TaskStatus::FailedAnalyzing => {}
        }

        Ok(())
    }

    pub(super) async fn dispatch_ready_tasks(self: Arc<Self>) -> Result<()> {
        self.replay_pending_child_parent_notifications().await;
        let now = now_millis();
        let persisted_active_tasks = self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: None,
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: true,
                order_by_recent_activity_desc: false,
                limit: None,
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await;
        {
            let mut tasks = self.tasks.lock().await;
            for task in persisted_active_tasks {
                if !tasks.iter().any(|entry| entry.id == task.id) {
                    tasks.push_back(task);
                }
            }
        }
        if self.apply_provider_preflight_to_ready_tasks().await {
            self.persist_tasks().await;
        }
        let sessions = self.session_manager.list().await;
        let config = self.config.read().await.clone();
        let active_goal_statuses = [
            GoalRunStatus::Queued,
            GoalRunStatus::Running,
            GoalRunStatus::AwaitingReview,
            GoalRunStatus::Paused,
        ];
        let mut goal_run_statuses = match self
            .history
            .list_goal_run_status_refs_for_statuses(&active_goal_statuses)
            .await
        {
            Ok(goal_runs) => goal_runs.into_iter().collect::<HashMap<_, _>>(),
            Err(error) => {
                tracing::warn!(
                    "failed to query persisted active goal runs for dispatcher selection: {error}"
                );
                HashMap::new()
            }
        };
        {
            let goal_runs = self.goal_runs.lock().await;
            for goal_run in goal_runs.iter() {
                goal_run_statuses.insert(goal_run.id.clone(), goal_run.status);
            }
        }
        let (changed_before_start, dispatched_tasks) = {
            let mut tasks = self.tasks.lock().await;
            let changed_before_start =
                refresh_task_queue_state(&mut tasks, now, &sessions, &config);
            let next_dispatches =
                select_ready_task_indices(&tasks, &sessions, &goal_run_statuses, &config);
            if next_dispatches.is_empty() {
                drop(tasks);
                if !changed_before_start.is_empty() {
                    self.persist_tasks().await;
                    for task in &changed_before_start {
                        if let Some(goal_run_id) = task.goal_run_id.as_deref() {
                            self.sync_goal_run_with_task(goal_run_id, task).await;
                        }
                    }
                    for task in changed_before_start {
                        self.emit_task_update(&task, Some(status_message(&task).into()));
                    }
                }
                return Ok(());
            }

            let mut dispatched_tasks = Vec::with_capacity(next_dispatches.len());
            for (index, lane_id) in next_dispatches {
                let task = &mut tasks[index];
                task.status = TaskStatus::InProgress;
                task.started_at = Some(now);
                task.completed_at = None;
                task.progress = task.progress.max(5);
                task.blocked_reason = None;
                task.awaiting_approval_id = None;
                task.lane_id = Some(lane_id.clone());
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Info,
                    "execution",
                    &format!("task dispatched to {lane_id} lane"),
                    None,
                ));
                dispatched_tasks.push(task.clone());
            }
            (changed_before_start, dispatched_tasks)
        };

        self.persist_tasks().await;
        for changed in &changed_before_start {
            if let Some(goal_run_id) = changed.goal_run_id.as_deref() {
                self.sync_goal_run_with_task(goal_run_id, changed).await;
            }
        }
        for changed in changed_before_start {
            self.emit_task_update(&changed, Some(status_message(&changed).into()));
        }
        for task in dispatched_tasks {
            self.emit_task_update(&task, Some(format!("Starting: {}", task.title)));
            let engine = self.clone();
            tokio::spawn(async move {
                if let Err(error) = engine.execute_dispatched_task(task).await {
                    tracing::error!(error = %error, "agent task execution error");
                }
            });
        }

        Ok(())
    }

    async fn execute_dispatched_task(&self, task: AgentTask) -> Result<()> {
        let prompt = task_prompt::build_dispatched_task_prompt(self, &task).await;
        let use_internal_weles_dm = task.sub_agent_def_id.as_deref()
            == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
            && task.source != "workspace_review";
        let workspace_review_target_agent_id = (!use_internal_weles_dm
            && task.source == "workspace_review")
            .then(|| task.sub_agent_def_id.as_deref())
            .flatten();
        let weles_sender_scope = if use_internal_weles_dm {
            match task.parent_task_id.as_deref() {
                Some(parent_task_id) => self.task_by_id_for_dispatcher(parent_task_id).await,
                None => None,
            }
            .as_ref()
            .map(|parent| crate::agent::agent_identity::agent_scope_id_for_task(Some(parent)))
            .unwrap_or_else(|| crate::agent::agent_identity::MAIN_AGENT_ID.to_string())
        } else {
            String::new()
        };
        let outcome = if use_internal_weles_dm {
            self.send_internal_task_message(
                &weles_sender_scope,
                crate::agent::agent_identity::WELES_AGENT_ID,
                &task.id,
                task.session_id.as_deref(),
                Some(task.runtime.as_str()),
                &prompt,
            )
            .await
        } else if let Some(target_agent_id) = workspace_review_target_agent_id {
            let requested_thread_id = task.thread_id.as_deref().filter(|thread_id| {
                !crate::agent::agent_identity::is_internal_dm_thread(thread_id)
            });
            let (review_thread_id, _) = self
                .get_or_create_thread_with_target(
                    requested_thread_id,
                    &prompt,
                    Some(target_agent_id),
                )
                .await;
            self.send_task_message(
                &task.id,
                Some(&review_thread_id),
                task.session_id.as_deref(),
                Some(task.runtime.as_str()),
                &prompt,
            )
            .await
        } else {
            self.send_task_message(
                &task.id,
                task.thread_id.as_deref(),
                task.session_id.as_deref(),
                Some(task.runtime.as_str()),
                &prompt,
            )
            .await
        };
        match outcome {
            Ok(outcome) if outcome.interrupted_for_approval => Ok(()),
            Ok(outcome) => {
                let now = now_millis();
                let active_child_ids = self
                    .active_subagent_child_ids_for_dispatcher(&task.id)
                    .await;
                let updated_live_task = {
                    let mut tasks = self.tasks.lock().await;
                    if let Some(current) = tasks.iter_mut().find(|entry| entry.id == task.id) {
                        apply_dispatched_task_success_update(
                            current,
                            &outcome,
                            &active_child_ids,
                            now,
                        );
                        Some(current.clone())
                    } else {
                        None
                    }
                };
                let (updated, updated_live_task) = match updated_live_task {
                    Some(updated) => (updated, true),
                    None => {
                        let Some(mut persisted_task) =
                            self.task_by_id_for_dispatcher(&task.id).await
                        else {
                            return Ok(());
                        };
                        apply_dispatched_task_success_update(
                            &mut persisted_task,
                            &outcome,
                            &active_child_ids,
                            now,
                        );
                        if let Err(error) = self.history.upsert_agent_task(&persisted_task).await {
                            tracing::warn!(
                                task_id = %persisted_task.id,
                                "failed to persist dispatched task success update: {error}"
                            );
                        }
                        let active_children = self
                            .active_subagent_child_tasks_for_dispatcher(&task.id)
                            .await;
                        let mut tasks = self.tasks.lock().await;
                        for active_child in active_children {
                            if !tasks.iter().any(|entry| entry.id == active_child.id) {
                                tasks.push_back(active_child);
                            }
                        }
                        if !tasks.iter().any(|entry| entry.id == persisted_task.id) {
                            tasks.push_back(persisted_task.clone());
                        }
                        (persisted_task, false)
                    }
                };
                if outcome.interrupted_for_approval
                    || crate::agent::tool_executor::task_is_awaiting_parent(&updated)
                {
                    return Ok(());
                }
                if updated_live_task {
                    self.persist_tasks().await;
                }
                self.emit_task_update(
                    &updated,
                    Some(if updated.status == TaskStatus::Blocked {
                        format!(
                            "Waiting for {} subagent(s)",
                            updated
                                .blocked_reason
                                .as_deref()
                                .map(|reason| reason.split(',').count())
                                .unwrap_or(0)
                        )
                    } else if updated.status == TaskStatus::BudgetExceeded {
                        "Task stopped after exhausting its execution budget".into()
                    } else if updated.retry_count > 0 {
                        "Task self-healed and completed".into()
                    } else {
                        "Task completed".into()
                    }),
                );
                match updated.status {
                    TaskStatus::Completed => {
                        self.settle_task_skill_consultations(&updated, "success")
                            .await;
                        if updated.source == "divergent" {
                            if let Err(error) = self
                                .record_divergent_contribution_on_task_completion(&updated)
                                .await
                            {
                                tracing::warn!(
                                    task_id = %updated.id,
                                    "failed to process divergent contribution completion hook: {error}"
                                );
                            }
                        }
                        if updated.source == "handoff" {
                            if let Err(error) =
                                self.record_handoff_task_outcome(&updated, "success").await
                            {
                                tracing::warn!(
                                    task_id = %updated.id,
                                    "failed to record handoff success outcome: {error}"
                                );
                            }
                        }
                        if updated.source == "subagent" && !updated.is_internal_weles_review() {
                            self.record_collaboration_outcome(&updated, "success").await;
                            self.record_subagent_outcome_on_parent(
                                &updated,
                                TaskLogLevel::Info,
                                "subagent completed",
                                updated.blocked_reason.clone(),
                            )
                            .await;
                        }
                        self.notify_task_terminal_state(&updated).await;
                    }
                    TaskStatus::BudgetExceeded => {
                        self.handle_budget_exceeded_task_terminal_state(&updated)
                            .await;
                        self.settle_task_skill_consultations(&updated, "failure")
                            .await;
                        if updated.source == "handoff" {
                            if let Err(error) =
                                self.record_handoff_task_outcome(&updated, "failure").await
                            {
                                tracing::warn!(
                                    task_id = %updated.id,
                                    "failed to record handoff failure outcome: {error}"
                                );
                            }
                        }
                        if updated.source == "subagent" && !updated.is_internal_weles_review() {
                            self.record_collaboration_outcome(&updated, "failure").await;
                            self.record_subagent_outcome_on_parent(
                                &updated,
                                TaskLogLevel::Warn,
                                "subagent budget exceeded",
                                updated.result.clone().or(updated.blocked_reason.clone()),
                            )
                            .await;
                        }
                        self.notify_task_terminal_state(&updated).await;
                    }
                    TaskStatus::Cancelled | TaskStatus::Failed => {
                        self.settle_task_skill_consultations(&updated, "failure")
                            .await;
                        if updated.source == "handoff" {
                            if let Err(error) =
                                self.record_handoff_task_outcome(&updated, "failure").await
                            {
                                tracing::warn!(
                                    task_id = %updated.id,
                                    "failed to record handoff failure outcome: {error}"
                                );
                            }
                        }
                        if updated.source == "subagent" && !updated.is_internal_weles_review() {
                            self.record_collaboration_outcome(&updated, "failure").await;
                            self.record_subagent_outcome_on_parent(
                                &updated,
                                TaskLogLevel::Error,
                                if updated.status == TaskStatus::Cancelled {
                                    "subagent cancelled"
                                } else {
                                    "subagent failed"
                                },
                                updated
                                    .result
                                    .clone()
                                    .or(updated.last_error.clone())
                                    .or(updated.error.clone()),
                            )
                            .await;
                        }
                        self.notify_task_terminal_state(&updated).await;
                    }
                    _ => {}
                }
                Ok(())
            }
            Err(error) => {
                let error_text = error.to_string();
                if let Some(signal) =
                    crate::agent::provider_preflight::availability_signal_from_observed_failure(
                        &error_text,
                    )
                {
                    let provider = match task.override_provider.clone() {
                        Some(provider) => provider,
                        None => self.config.read().await.provider.clone(),
                    };
                    self.record_provider_availability_signal(&provider, signal)
                        .await;
                }
                let provider_quota_failure = task.is_spawned_subagent()
                    && recoverable_goal_pause_reason(&error_text)
                        == Some(RecoverableGoalPauseReason::ProviderCredits);
                let quota_summary = if provider_quota_failure {
                    let thread_id = task.thread_id.as_deref();
                    let threads = self.threads.read().await;
                    thread_id
                        .and_then(|thread_id| threads.get(thread_id))
                        .map(|thread| {
                            crate::agent::subagent::context_budget::synthesize_visible_assistant_summary(
                                &thread.messages,
                            )
                        })
                } else {
                    None
                };
                let retry_delay_ms = compute_task_backoff_ms(
                    self.config.read().await.retry_delay_ms,
                    task.retry_count.saturating_add(1),
                );
                let mut updated_live_task = false;
                let mut updated = {
                    let mut tasks = self.tasks.lock().await;
                    tasks
                        .iter_mut()
                        .find(|entry| entry.id == task.id)
                        .map(|current| {
                            if provider_quota_failure {
                                apply_dispatched_subagent_provider_quota_failure(
                                    current,
                                    &error_text,
                                    quota_summary.as_deref(),
                                );
                            } else {
                                apply_dispatched_task_failure_update(
                                    current,
                                    &error_text,
                                    retry_delay_ms,
                                );
                            }
                            updated_live_task = true;
                            current.clone()
                        })
                };
                if updated.is_none() {
                    let Some(mut persisted_task) = self.task_by_id_for_dispatcher(&task.id).await
                    else {
                        return Ok(());
                    };
                    if provider_quota_failure {
                        apply_dispatched_subagent_provider_quota_failure(
                            &mut persisted_task,
                            &error_text,
                            quota_summary.as_deref(),
                        );
                    } else {
                        apply_dispatched_task_failure_update(
                            &mut persisted_task,
                            &error_text,
                            retry_delay_ms,
                        );
                    }
                    if let Err(error) = self.history.upsert_agent_task(&persisted_task).await {
                        tracing::warn!(
                            task_id = %persisted_task.id,
                            "failed to persist dispatched task failure update: {error}"
                        );
                        return Ok(());
                    }
                    {
                        let mut tasks = self.tasks.lock().await;
                        if !tasks.iter().any(|entry| entry.id == persisted_task.id) {
                            tasks.push_back(persisted_task.clone());
                        }
                    }
                    updated = Some(persisted_task);
                }
                let updated = updated.expect("dispatched task failure update should exist");

                if updated_live_task {
                    self.persist_tasks().await;
                }
                self.emit_task_update(
                    &updated,
                    Some(match updated.status {
                        TaskStatus::FailedAnalyzing => {
                            format!("Attempt {} failed; retry scheduled", updated.retry_count)
                        }
                        _ => format!("Failed: {error_text}"),
                    }),
                );
                if updated.status == TaskStatus::Failed {
                    self.settle_task_skill_consultations(&updated, "failure")
                        .await;
                    if updated.source == "handoff" {
                        if let Err(error) =
                            self.record_handoff_task_outcome(&updated, "failure").await
                        {
                            tracing::warn!(
                                task_id = %updated.id,
                                "failed to record handoff failure outcome: {error}"
                            );
                        }
                    }
                }
                if updated.source == "subagent"
                    && !updated.is_internal_weles_review()
                    && matches!(updated.status, TaskStatus::Failed | TaskStatus::Cancelled)
                {
                    self.record_collaboration_outcome(&updated, "failure").await;
                    self.record_subagent_outcome_on_parent(
                        &updated,
                        TaskLogLevel::Error,
                        "subagent failed",
                        updated.result.clone().or(updated.last_error.clone()),
                    )
                    .await;
                }
                if updated.status == TaskStatus::Failed {
                    self.notify_task_terminal_state(&updated).await;
                }
                Ok(())
            }
        }
    }

    async fn handle_budget_exceeded_task_terminal_state(&self, task: &AgentTask) {
        let Some(thread_id) = task.thread_id.as_deref() else {
            return;
        };

        let message = format!(
            "Task budget exceeded for this thread.\n\nThread `{thread_id}` exhausted its execution budget and is now locked for further operator messages. Review the completed work in this thread. If more work is needed, continue from the parent thread and respawn from the last completed point with a larger child budget."
        );
        if self.append_system_thread_message(thread_id, message).await {
            self.emit_workflow_notice(
                thread_id,
                "thread-budget-exceeded",
                "Thread budget exceeded. Further sends are blocked for this thread.",
                Some(
                    serde_json::json!({
                        "task_id": task.id,
                        "thread_id": thread_id,
                        "parent_task_id": task.parent_task_id,
                        "parent_thread_id": task.parent_thread_id,
                    })
                    .to_string(),
                ),
            );
        }
    }

    async fn reconcile_child_result_contract(&self, child: &mut AgentTask) {
        if !child.is_spawned_subagent() {
            return;
        }
        let ask_records = match crate::agent::tool_executor::list_ask_records(self, &child.id).await
        {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(child_task_id = %child.id, %error, "failed to reconcile child asks");
                return;
            }
        };
        let open_ask_ids = ask_records
            .iter()
            .filter(|(_, record)| record.state == "open")
            .filter_map(|(key, _)| key.rsplit_once(':').map(|(_, id)| id.to_string()))
            .collect::<Vec<_>>();
        let Some(contract) = child.completion_contract.as_mut() else {
            return;
        };
        let result = contract.child_result.get_or_insert_with(|| {
            let summary = child
                .result
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            crate::agent::types::ChildResultContract {
                terminal_status: Some(child.status),
                terminal_version: 1,
                report_state: if summary.is_some() {
                    crate::agent::types::ChildReportState::Truncated
                } else {
                    crate::agent::types::ChildReportState::Unavailable
                },
                summary_chars: summary.as_deref().map(str::len).unwrap_or(0),
                summary,
                parent_notification: crate::agent::types::ParentNotificationState::Pending,
                ..Default::default()
            }
        });
        result.terminal_status = Some(child.status);
        result.terminal_version = result.terminal_version.max(1);
        result.open_ask_ids = open_ask_ids;
        result.asks_reconciled = result.open_ask_ids.is_empty();
    }

    async fn mark_child_parent_notification(
        &self,
        child_task_id: &str,
        delivered: bool,
        error: Option<String>,
    ) {
        let Some(mut child) = self.task_by_id_for_dispatcher(child_task_id).await else {
            return;
        };
        let Some(result) = child
            .completion_contract
            .as_mut()
            .and_then(|contract| contract.child_result.as_mut())
        else {
            return;
        };
        result.parent_notification = if delivered {
            crate::agent::types::ParentNotificationState::Delivered
        } else {
            crate::agent::types::ParentNotificationState::Pending
        };
        result.parent_notification_error = error;
        if delivered {
            result.parent_notified_at = Some(now_millis());
        }
        let _ = self.history.upsert_agent_task(&child).await;
        let mut tasks = self.tasks.lock().await;
        if let Some(live) = tasks.iter_mut().find(|task| task.id == child.id) {
            *live = child;
        }
    }

    async fn mark_child_result_integrated(
        &self,
        child_task_id: &str,
        parent_task_id: Option<&str>,
    ) {
        let now = now_millis();
        if let Some(mut child) = self.task_by_id_for_dispatcher(child_task_id).await {
            if let Some(result) = child
                .completion_contract
                .as_mut()
                .and_then(|contract| contract.child_result.as_mut())
            {
                result.integration_acknowledged_at = Some(now);
                result.parent_notification_error = None;
                let _ = self.history.upsert_agent_task(&child).await;
                let mut tasks = self.tasks.lock().await;
                if let Some(live) = tasks.iter_mut().find(|task| task.id == child.id) {
                    *live = child;
                }
            }
        }

        let Some(parent_task_id) = parent_task_id else {
            return;
        };
        let Some(mut parent) = self.task_by_id_for_dispatcher(parent_task_id).await else {
            return;
        };
        if let Some(contract) = parent.completion_contract.as_mut() {
            contract.acknowledge_child_result_integration(
                child_task_id,
                format!("parent continuation consumed child result at {now}"),
            );
        }
        let _ = self.history.upsert_agent_task(&parent).await;
        let mut tasks = self.tasks.lock().await;
        if let Some(live) = tasks.iter_mut().find(|task| task.id == parent.id) {
            *live = parent;
        }
    }

    async fn record_subagent_outcome_on_parent(
        &self,
        child_task: &AgentTask,
        level: TaskLogLevel,
        message: &str,
        details: Option<String>,
    ) {
        if child_task.is_internal_weles_review() {
            return;
        }
        let mut child_task = child_task.clone();
        self.reconcile_child_result_contract(&mut child_task).await;
        if let Err(error) = self.history.upsert_agent_task(&child_task).await {
            tracing::warn!(child_task_id = %child_task.id, %error, "failed to persist child terminal contract");
            return;
        }
        // Keep the live queue aligned before any later `persist_tasks` call
        // writes the full snapshot; otherwise the stale live child can erase
        // the just-persisted outbox/ask reconciliation state.
        {
            let mut tasks = self.tasks.lock().await;
            if let Some(live) = tasks.iter_mut().find(|task| task.id == child_task.id) {
                *live = child_task.clone();
            }
        }
        let child_task = &child_task;
        let mut updated_live_parent = false;
        let mut updated_parent = None;
        if let Some(parent_task_id) = child_task.parent_task_id.as_deref() {
            updated_parent = {
                let mut tasks = self.tasks.lock().await;
                tasks
                    .iter_mut()
                    .find(|entry| entry.id == parent_task_id)
                    .map(|parent| {
                        if let Some(contract) = parent.completion_contract.as_mut() {
                            contract.require_child_result_integration(&child_task.id);
                        }
                        append_subagent_outcome_log_to_parent(
                            parent,
                            child_task,
                            level,
                            message,
                            details.as_deref(),
                        );
                        updated_live_parent = true;
                        parent.clone()
                    })
            };

            if updated_parent.is_none() {
                if let Some(mut parent) = self.task_by_id_for_dispatcher(parent_task_id).await {
                    if let Some(contract) = parent.completion_contract.as_mut() {
                        contract.require_child_result_integration(&child_task.id);
                    }
                    append_subagent_outcome_log_to_parent(
                        &mut parent,
                        child_task,
                        level,
                        message,
                        details.as_deref(),
                    );
                    if let Err(error) = self.history.upsert_agent_task(&parent).await {
                        tracing::warn!(
                            parent_task_id,
                            child_task_id = %child_task.id,
                            "failed to persist subagent outcome on parent task: {error}"
                        );
                    } else {
                        updated_parent = Some(parent);
                    }
                }
            }
        }

        if let Some(parent) = updated_parent.as_ref() {
            if updated_live_parent {
                self.persist_tasks().await;
            }
            self.emit_task_update(parent, Some("Subagent update received".into()));
        }

        let parent_thread_id = updated_parent
            .as_ref()
            .and_then(|parent| parent.thread_id.clone())
            .or_else(|| child_task.parent_thread_id.clone());
        let Some(parent_thread_id) = parent_thread_id else {
            return;
        };
        let parent_task_id = updated_parent
            .as_ref()
            .map(|parent| parent.id.clone())
            .or_else(|| child_task.parent_task_id.clone());

        let child_slug = crate::agent::tool_executor::ensure_task_slug(self, child_task)
            .await
            .unwrap_or_else(|_| child_task.id.clone());
        let child_task_id = child_task.id.as_str();
        let notice = match child_task.status {
            TaskStatus::Completed => {
                let result_text = child_task
                    .result
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("(the subagent did not record a result)");
                let ledger_checkpoint = None::<String>;
                let integration_checkpoint = ledger_checkpoint
                    .as_deref()
                    .map(|checkpoint| format!("\n\n{checkpoint}"))
                    .unwrap_or_default();
                Some((
                    format!(
                        "Child `{child_slug}` finished and reported back.\n\nResult:\n{result_text}{integration_checkpoint}\n\nIntegrate this result against the ledger checkpoint, then report the outcome back to the operator. Use `list_subagents` if you need the full child output."
                    ),
                    "child-thread-completed",
                    format!("Child {child_slug} reported back."),
                ))
            }
            TaskStatus::Failed | TaskStatus::Cancelled => {
                let outcome = if child_task.status == TaskStatus::Cancelled {
                    "was cancelled before completing"
                } else {
                    "failed"
                };
                let error_text = child_task
                    .last_error
                    .as_deref()
                    .or(child_task.error.as_deref())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("(no error detail recorded)");
                let result_text = child_task
                    .result
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("(no job summary recorded)");
                Some((
                    format!(
                        "Child `{child_slug}` {outcome}.\n\nStatus: {}\nSummary:\n{result_text}\n\nError:\n{error_text}\n\nDecide whether to retry, call `extend_subagent_budget` with child_task_id `{child_slug}` if this was a zorai budget issue, or report the failure back to the operator.",
                        if child_task.status == TaskStatus::Cancelled {
                            "cancelled"
                        } else {
                            "error"
                        }
                    ),
                    "child-thread-failed",
                    format!("Child {child_slug} {outcome}."),
                ))
            }
            TaskStatus::BudgetExceeded => {
                let result_text = child_task
                    .result
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("(the subagent did not record a usable summary)");
                Some((
                    format!(
                        "Child `{child_slug}` exhausted its execution budget and reported back.\n\nStatus: error\nSummary:\n{result_text}\n\nIf the work is sufficient, keep it. To continue that same child thread, call `extend_subagent_budget` with child_task_id `{child_slug}` and additional_tokens. Do not respawn from scratch unless the child result is unusable."
                    ),
                    "child-thread-budget-exceeded",
                    format!("Child {child_slug} exhausted its budget."),
                ))
            }
            _ => None,
        };
        if let Some((parent_message, notice_kind, notice_summary)) = notice {
            let already_integrated = child_task
                .completion_contract
                .as_ref()
                .and_then(|contract| contract.child_result.as_ref())
                .is_some_and(|result| result.integration_acknowledged_at.is_some());
            if already_integrated {
                return;
            }
            let issued_this_run = self
                .subagent_completion_continuations_issued
                .lock()
                .await
                .contains(&child_task.id);
            if issued_this_run {
                let still_queued = self
                    .deferred_visible_thread_continuations_for(&parent_thread_id)
                    .await
                    .iter()
                    .any(|continuation| {
                        continuation.llm_user_content.contains(child_task_id)
                            || continuation
                                .llm_user_content
                                .contains(&format!("Child `{child_slug}`"))
                    });
                let delivered = !still_queued
                    || matches!(
                        self.resume_idle_parent_after_subagent_completion(
                            &parent_thread_id,
                            &child_task.id
                        )
                        .await,
                        Ok(true)
                    );
                if delivered {
                    self.subagent_completion_continuations_issued
                        .lock()
                        .await
                        .remove(&child_task.id);
                    self.mark_child_result_integrated(&child_task.id, parent_task_id.as_deref())
                        .await;
                }
                return;
            }
            let message_already_present = if self
                .ensure_thread_messages_loaded(&parent_thread_id)
                .await
            {
                self.threads
                    .read()
                    .await
                    .get(&parent_thread_id)
                    .is_some_and(|thread| {
                        thread.messages.iter().any(|message| {
                            message.role == MessageRole::System
                                && (message.content.contains(child_task_id)
                                    || message.content.contains(&format!("Child `{child_slug}`")))
                        })
                    })
            } else {
                false
            };
            if message_already_present
                || self
                    .append_system_thread_message(&parent_thread_id, parent_message.clone())
                    .await
            {
                self.emit_workflow_notice(
                    &parent_thread_id,
                    notice_kind,
                    notice_summary,
                    Some(
                        serde_json::json!({
                            "child_task_id": child_task.id,
                            "child_thread_id": child_task.thread_id,
                            "parent_task_id": parent_task_id,
                        })
                        .to_string(),
                    ),
                );
                self.enqueue_subagent_completion_continuation(
                    &parent_thread_id,
                    &parent_message,
                    parent_task_id.clone(),
                )
                .await;
                let queued = self
                    .deferred_visible_thread_continuations_for(&parent_thread_id)
                    .await
                    .iter()
                    .any(|continuation| {
                        continuation.llm_user_content.contains(child_task_id)
                            || continuation
                                .llm_user_content
                                .contains(&format!("Child `{child_slug}`"))
                    });
                if !queued {
                    self.mark_child_parent_notification(
                        &child_task.id,
                        false,
                        Some("failed to queue parent completion continuation".to_string()),
                    )
                    .await;
                    return;
                }
                // The child row is the durable outbox. Once the deterministic
                // continuation has been accepted by the local queue, persist
                // Delivered while leaving integration unacknowledged. A restart
                // replays Delivered+unacknowledged rows and reconstructs the
                // in-memory continuation without duplicating the thread message.
                self.mark_child_parent_notification(&child_task.id, true, None)
                    .await;
                self.subagent_completion_continuations_issued
                    .lock()
                    .await
                    .insert(child_task.id.clone());
                match self
                    .resume_idle_parent_after_subagent_completion(&parent_thread_id, &child_task.id)
                    .await
                {
                    Ok(true) => {
                        self.subagent_completion_continuations_issued
                            .lock()
                            .await
                            .remove(&child_task.id);
                        self.mark_child_result_integrated(
                            &child_task.id,
                            parent_task_id.as_deref(),
                        )
                        .await;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.mark_child_parent_notification(&child_task.id, false, Some(error))
                            .await;
                    }
                }
            } else {
                self.mark_child_parent_notification(
                    &child_task.id,
                    false,
                    Some("failed to append parent completion message".to_string()),
                )
                .await;
            }
        }
    }

    pub(in crate::agent) async fn thread_is_idle_for_subagent_wakeup(
        &self,
        thread_id: &str,
    ) -> bool {
        let streams = self.stream_cancellations.lock().await;
        stream_entry_is_idle_for_subagent_wakeup(streams.get(thread_id))
    }

    async fn resume_idle_parent_after_subagent_completion(
        &self,
        parent_thread_id: &str,
        child_task_id: &str,
    ) -> std::result::Result<bool, String> {
        let provider = self.config.read().await.provider.clone();
        if !self.provider_circuit_is_closed(&provider).await {
            tracing::info!(
                thread_id = %parent_thread_id,
                child_task_id,
                provider,
                "not resuming parent continuation while the provider circuit breaker is open"
            );
            return Ok(false);
        }
        for attempt in 0..3 {
            let idle = self
                .thread_is_idle_for_subagent_wakeup(parent_thread_id)
                .await;
            if idle {
                let _ = self.stop_stream(parent_thread_id).await;
                self.clear_operator_stream_stop(parent_thread_id).await;
            } else {
                tracing::info!(
                    thread_id = %parent_thread_id,
                    child_task_id,
                    attempt,
                    "deferring subagent parent wakeup until the live parent stream finishes"
                );
                return Ok(false);
            }

            if let Err(error) = self
                .flush_deferred_visible_thread_continuations(parent_thread_id)
                .await
            {
                tracing::warn!(
                    thread_id = %parent_thread_id,
                    child_task_id,
                    attempt,
                    error = %error,
                    "subagent completion continuation flush failed"
                );
                return Err(error.to_string());
            }
            if self
                .deferred_visible_thread_continuations_for(parent_thread_id)
                .await
                .is_empty()
            {
                return Ok(true);
            }
            tracing::info!(
                thread_id = %parent_thread_id,
                child_task_id,
                attempt,
                "idle parent still has a queued subagent continuation; retrying wakeup"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        Ok(false)
    }

    async fn enqueue_subagent_completion_continuation(
        &self,
        thread_id: &str,
        completion_message: &str,
        parent_task_id: Option<String>,
    ) {
        let prior_user_message = match self.history.latest_user_message_content(thread_id).await {
            Ok(message) => message,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    "failed to load latest user message for subagent continuation: {error}"
                );
                None
            }
        };
        let agent_id = if let Some(task_id) = parent_task_id.as_deref() {
            self.agent_scope_id_for_turn(Some(thread_id), Some(task_id))
                .await
        } else {
            self.active_agent_id_for_thread(thread_id)
                .await
                .unwrap_or_else(|| MAIN_AGENT_ID.to_string())
        };
        self.enqueue_visible_thread_continuation(
            thread_id,
            DeferredVisibleThreadContinuation {
                agent_id,
                task_id: parent_task_id,
                preferred_session_hint: None,
                llm_user_content: subagent_completion_continuation_prompt(
                    completion_message,
                    prior_user_message.as_deref(),
                ),
                queued_at_ms: 0,
                force_compaction: false,
                rerun_participant_observers_after_turn: true,
                internal_delegate_sender: None,
                internal_delegate_message: None,
            },
        )
        .await;
    }

    async fn record_handoff_task_outcome(&self, task: &AgentTask, outcome: &str) -> Result<()> {
        let Some(context) = self
            .get_handoff_learning_context_by_task_id(&task.id)
            .await?
        else {
            return Ok(());
        };

        let duration_ms = task
            .started_at
            .zip(task.completed_at)
            .map(|(started, completed)| completed.saturating_sub(started));
        let error_message = if matches!(outcome, "failure") {
            task.last_error.as_deref().or(task.error.as_deref())
        } else {
            None
        };

        let thread_tokens = if let Some(thread_id) = task.thread_id.as_deref() {
            let threads = self.threads.read().await;
            threads
                .get(thread_id)
                .map(|thread| thread.total_input_tokens + thread.total_output_tokens)
                .unwrap_or(0)
        } else {
            0
        };

        let ema_alpha = self.config.read().await.routing.confidence_ema_alpha;

        self.update_handoff_outcome(
            &context.handoff_log_id,
            if matches!(outcome, "success") {
                "completed"
            } else {
                "failed"
            },
            duration_ms,
            error_message,
        )
        .await?;

        self.record_capability_outcome(
            &context.to_specialist_id,
            &context.capability_tags,
            outcome,
            context.routing_score,
            thread_tokens,
            ema_alpha,
        )
        .await?;

        Ok(())
    }
}

fn stream_entry_is_idle_for_subagent_wakeup(entry: Option<&StreamCancellationEntry>) -> bool {
    match entry {
        None => true,
        Some(entry) => entry.token.is_cancelled(),
    }
}

fn subagent_completion_continuation_prompt(
    completion_message: &str,
    prior_user_message: Option<&str>,
) -> String {
    let mut prompt = format!(
        "A spawned subagent for the active task has finished. This is an internal runtime continuation, not a new operator request.\n\nCompletion notice:\n{completion_message}\n"
    );
    if let Some(prior_user_message) = prior_user_message
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("\nOriginal operator request:\n");
        prompt.push_str(prior_user_message);
        prompt.push('\n');
    }
    prompt.push_str(
        "\nContinue the original task now. Integrate the subagent result before deciding the next step. Do not merely acknowledge this notification or reply only with a confirmation such as \"OK\". Use the result, continue any remaining work, validate the outcome, and report meaningful progress or the final result. If the subagent failed, diagnose the failure and continue with an appropriate recovery when safe.",
    );
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::{timeout, Duration};

    async fn spawn_dispatcher_stub_assistant_server(response_text: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub assistant server");
        let addr = listener.local_addr().expect("stub assistant local addr");
        let response_json =
            serde_json::to_string(response_text).expect("assistant response should serialize");

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let response_json = response_json.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0u8; 65536];
                    let _ = socket.read(&mut buffer).await;
                    let response = format!(
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\n",
                            "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\n",
                            "data: [DONE]\n\n"
                        ),
                        response_json
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write stub assistant response");
                });
            }
        });

        format!("http://{addr}/v1")
    }

    async fn insert_parent_thread(engine: &AgentEngine, thread_id: &str, user_message: &str) {
        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Svarog".to_string()),
                title: "Parent".to_string(),
                messages: vec![AgentMessage::user(user_message, 1)],
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
    async fn advance_goal_run_uses_persisted_worker_task_before_requeueing() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let goal = engine
            .start_goal_run(
                "Keep persisted running worker attached".to_string(),
                Some("Persisted worker task".to_string()),
                Some("thread-persisted-worker-task".to_string()),
                None,
                None,
                None,
                Some("agent".to_string()),
                None,
            )
            .await;
        engine
            .enqueue_goal_worker(&goal.id)
            .await
            .expect("enqueue worker");
        let task_id = engine
            .get_goal_run(&goal.id)
            .await
            .expect("goal")
            .active_task_id
            .clone()
            .expect("worker task");
        {
            let mut tasks = engine.tasks.lock().await;
            let persisted = tasks
                .iter_mut()
                .find(|entry| entry.id == task_id)
                .expect("enqueued task should exist");
            persisted.status = TaskStatus::InProgress;
            persisted.started_at = Some(now_millis());
        }
        engine.persist_tasks().await;
        engine.persist_goal_runs().await;
        engine.tasks.lock().await.clear();

        let persisted_task = engine
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
            .next()
            .expect("persisted task row should be queryable by id");
        assert_eq!(persisted_task.status, TaskStatus::InProgress);
        let persisted_goal = engine
            .get_goal_run(&goal.id)
            .await
            .expect("persisted goal should be queryable by id");
        assert_eq!(persisted_goal.status, GoalRunStatus::Running);
        assert_eq!(
            persisted_goal.active_task_id.as_deref(),
            Some(task_id.as_str())
        );

        engine
            .advance_goal_run(&goal.id)
            .await
            .expect("advancing goal should not fail");

        let updated = engine
            .get_goal_run(&goal.id)
            .await
            .expect("goal should remain available");
        assert_eq!(updated.active_task_id.as_deref(), Some(task_id.as_str()));
        assert_eq!(updated.child_task_ids, vec![task_id.clone()]);
        assert_eq!(updated.status, GoalRunStatus::Running);
    }

    #[tokio::test]
    async fn advance_goal_run_does_not_complete_when_persisted_goal_task_is_active() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let goal = engine
            .start_goal_run(
                "Wait for persisted active task".to_string(),
                Some("Persisted active task".to_string()),
                Some("thread-persisted-active-task".to_string()),
                None,
                None,
                None,
                Some("agent".to_string()),
                None,
            )
            .await;
        engine
            .enqueue_goal_worker(&goal.id)
            .await
            .expect("enqueue worker");
        let task_id = engine
            .get_goal_run(&goal.id)
            .await
            .expect("goal")
            .active_task_id
            .clone()
            .expect("worker task");
        {
            let mut tasks = engine.tasks.lock().await;
            let persisted = tasks
                .iter_mut()
                .find(|entry| entry.id == task_id)
                .expect("enqueued task should exist");
            persisted.status = TaskStatus::InProgress;
            persisted.started_at = Some(now_millis());
        }
        engine.persist_tasks().await;
        engine.persist_goal_runs().await;
        engine.tasks.lock().await.clear();

        engine
            .advance_goal_run(&goal.id)
            .await
            .expect("advancing goal should not fail");

        let updated = engine
            .get_goal_run(&goal.id)
            .await
            .expect("goal should remain available");
        assert_eq!(
            updated.status,
            GoalRunStatus::Running,
            "persisted active worker should prevent premature goal completion"
        );
        assert_eq!(updated.active_task_id.as_deref(), Some(task_id.as_str()));
        assert!(updated.completed_at.is_none());
    }

    #[tokio::test]
    async fn internal_weles_review_does_not_report_back_or_wake_visible_parent() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let parent_thread_id = "thread-glmus-governance-parent";
        let _ = engine
            .get_or_create_thread(Some(parent_thread_id), "operator request")
            .await;
        let original_count = engine
            .threads
            .read()
            .await
            .get(parent_thread_id)
            .map(|thread| thread.messages.len())
            .unwrap_or_default();

        let mut review = engine
            .enqueue_task(
                "WELES".to_string(),
                "Internal governance review".to_string(),
                "high",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some("dm:subagent-1781800311670:weles".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        review.status = TaskStatus::Completed;
        review.parent_thread_id = Some(parent_thread_id.to_string());
        review.sub_agent_def_id =
            Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
        review.override_system_prompt =
            crate::agent::weles_governance::build_weles_internal_override_payload(
                crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
                &serde_json::json!({"tool_name":"bash_command"}),
            );

        engine
            .record_subagent_outcome_on_parent(
                &review,
                TaskLogLevel::Info,
                "subagent completed",
                None,
            )
            .await;

        let messages = engine
            .threads
            .read()
            .await
            .get(parent_thread_id)
            .map(|thread| thread.messages.len())
            .unwrap_or_default();
        assert_eq!(messages, original_count);
        assert!(engine
            .deferred_visible_thread_continuations_for(parent_thread_id)
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn completed_goal_subagent_injects_ledger_checkpoint_into_parent_thread() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let parent_thread_id = "thread-goal-subagent-parent";

        engine.threads.write().await.insert(
            parent_thread_id.to_string(),
            AgentThread {
                id: parent_thread_id.to_string(),
                agent_name: Some("Svarog".to_string()),
                title: "Goal parent".to_string(),
                messages: vec![AgentMessage::user("Integrate child results", 1)],
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

        let goal = engine
            .start_goal_run(
                "Integrate verified child work".to_string(),
                Some("Goal child integration".to_string()),
                Some(parent_thread_id.to_string()),
                None,
                None,
                None,
                Some("agent".to_string()),
                None,
            )
            .await;
        let parent = engine
            .enqueue_task(
                "Parent task".to_string(),
                "Wait for child".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "goal_run",
                Some(goal.id.clone()),
                None,
                Some(parent_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        let mut child = engine
            .enqueue_task(
                "Child task".to_string(),
                "Return verified work".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                Some(goal.id.clone()),
                Some(parent.id.clone()),
                Some("thread-goal-subagent-child".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Completed;
        child.result = Some("child verification artifact".to_string());
        child.parent_thread_id = Some(parent_thread_id.to_string());

        engine
            .record_subagent_outcome_on_parent(
                &child,
                TaskLogLevel::Info,
                "subagent completed",
                None,
            )
            .await;

        let threads = engine.threads.read().await;
        let parent_thread = threads
            .get(parent_thread_id)
            .expect("parent thread should exist");
        let message = parent_thread
            .messages
            .iter()
            .find(|message| {
                message.role == MessageRole::System
                    && message.content.contains("child verification artifact")
            })
            .expect("completed child result should be appended to parent");
        assert!(message.content.contains("reported back"));
        assert!(message.content.contains("child verification artifact"));
    }

    #[tokio::test]
    async fn completed_subagent_queues_parent_continuation_while_parent_stream_is_live() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let parent_thread_id = "thread-parent-live-stream";
        insert_parent_thread(&engine, parent_thread_id, "Coordinate the child work").await;
        engine.begin_stream_cancellation(parent_thread_id).await;

        let parent = engine
            .enqueue_task(
                "Parent task".to_string(),
                "Wait for child".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some(parent_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        let mut child = engine
            .enqueue_task(
                "Child task".to_string(),
                "Finish the assigned slice".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                Some(parent.id.clone()),
                Some("thread-child-live-stream".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Completed;
        child.result = Some("parser patch applied".to_string());
        child.parent_thread_id = Some(parent_thread_id.to_string());

        engine
            .record_subagent_outcome_on_parent(
                &child,
                TaskLogLevel::Info,
                "subagent completed",
                None,
            )
            .await;

        let continuations = engine
            .deferred_visible_thread_continuations_for(parent_thread_id)
            .await;
        assert_eq!(
            continuations.len(),
            1,
            "a live parent stream should defer the continuation like background operation finish"
        );
        assert!(
            continuations[0]
                .llm_user_content
                .contains("A spawned subagent for the active task has finished"),
            "parent continuation should describe the finished child, got: {}",
            continuations[0].llm_user_content
        );
        assert!(continuations[0]
            .llm_user_content
            .contains("parser patch applied"));
    }

    #[tokio::test]
    async fn idle_parent_is_forced_awake_when_spawned_subagent_finishes() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
        config.base_url =
            spawn_dispatcher_stub_assistant_server("Integrated the child result.").await;
        config.model = "gpt-4o-mini".to_string();
        config.api_key = "test-key".to_string();
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let parent_thread_id = "thread-parent-idle-wakeup";
        insert_parent_thread(&engine, parent_thread_id, "Investigate the failing parser").await;

        let mut child = engine
            .enqueue_task(
                "Child task".to_string(),
                "Fix the parser".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some(parent_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Completed;
        child.result = Some("parser tests now pass".to_string());
        child.parent_thread_id = Some(parent_thread_id.to_string());

        timeout(
            Duration::from_secs(8),
            engine.record_subagent_outcome_on_parent(
                &child,
                TaskLogLevel::Info,
                "subagent completed",
                None,
            ),
        )
        .await
        .expect("idle parent wakeup should finish");

        let threads = engine.threads.read().await;
        let parent_thread = threads
            .get(parent_thread_id)
            .expect("parent thread should exist");
        assert!(parent_thread.messages.iter().any(|message| {
            message.role == MessageRole::System && message.content.contains("parser tests now pass")
        }));
        assert!(
            parent_thread.messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.content.contains("Integrated the child result.")
            }),
            "an idle parent must be forced awake to continue after the child finishes"
        );
        assert!(engine
            .deferred_visible_thread_continuations_for(parent_thread_id)
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn stale_parent_stream_is_forced_awake_when_spawned_subagent_finishes() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
        config.base_url =
            spawn_dispatcher_stub_assistant_server("Integrated the child result.").await;
        config.model = "gpt-4o-mini".to_string();
        config.api_key = "test-key".to_string();
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let parent_thread_id = "thread-parent-stale-stream-wakeup";
        insert_parent_thread(&engine, parent_thread_id, "Investigate the failing parser").await;
        engine.begin_stream_cancellation(parent_thread_id).await;
        let _ = engine.stop_stream(parent_thread_id).await;

        let mut child = engine
            .enqueue_task(
                "Child task".to_string(),
                "Fix the parser".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some(parent_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Completed;
        child.result = Some("parser tests now pass".to_string());
        child.parent_thread_id = Some(parent_thread_id.to_string());

        timeout(
            Duration::from_secs(8),
            engine.record_subagent_outcome_on_parent(
                &child,
                TaskLogLevel::Info,
                "subagent completed",
                None,
            ),
        )
        .await
        .expect("stale parent wakeup should finish");

        let threads = engine.threads.read().await;
        let parent_thread = threads
            .get(parent_thread_id)
            .expect("parent thread should exist");
        assert!(parent_thread.messages.iter().any(|message| {
            message.role == MessageRole::System && message.content.contains("parser tests now pass")
        }));
        assert!(
            parent_thread.messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.content.contains("Integrated the child result.")
            }),
            "a cancelled leftover parent stream must not block wakeup after the child finishes"
        );
        assert!(engine
            .deferred_visible_thread_continuations_for(parent_thread_id)
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn live_parent_first_token_wait_is_not_cancelled_when_child_finishes() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
        config.base_url =
            spawn_dispatcher_stub_assistant_server("Integrated the child result.").await;
        config.model = "gpt-4o-mini".to_string();
        config.api_key = "test-key".to_string();
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let parent_thread_id = "thread-parent-first-token-wait";
        insert_parent_thread(&engine, parent_thread_id, "Investigate the failing parser").await;
        engine.begin_stream_cancellation(parent_thread_id).await;
        {
            let mut streams = engine.stream_cancellations.lock().await;
            let entry = streams
                .get_mut(parent_thread_id)
                .expect("parent stream entry should exist");
            entry.last_progress_at = now_millis().saturating_sub(60_000);
            entry.last_progress_kind = StreamProgressKind::Started;
        }

        let mut child = engine
            .enqueue_task(
                "Child task".to_string(),
                "Fix the parser".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some(parent_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Completed;
        child.result = Some("parser tests now pass".to_string());
        child.parent_thread_id = Some(parent_thread_id.to_string());

        timeout(
            Duration::from_secs(8),
            engine.record_subagent_outcome_on_parent(
                &child,
                TaskLogLevel::Info,
                "subagent completed",
                None,
            ),
        )
        .await
        .expect("live parent deferral should finish without cancelling the stream");

        let streams = engine.stream_cancellations.lock().await;
        let entry = streams
            .get(parent_thread_id)
            .expect("live parent stream entry should remain");
        assert!(
            !entry.token.is_cancelled(),
            "waiting for the first token must not look idle just because last_progress_kind is still Started"
        );
        drop(streams);
        assert!(
            !engine
                .deferred_visible_thread_continuations_for(parent_thread_id)
                .await
                .is_empty(),
            "child completion must wait for the live parent turn instead of aborting it"
        );
    }

    #[test]
    fn idle_wakeup_treats_cancelled_and_stale_streams_as_idle() {
        let live = StreamCancellationEntry {
            generation: 1,
            token: CancellationToken::new(),
            retry_now: Arc::new(tokio::sync::Notify::new()),
            started_at: 0,
            last_progress_at: 1_000,
            last_progress_kind: StreamProgressKind::Started,
            last_progress_excerpt: String::new(),
        };
        assert!(
            !stream_entry_is_idle_for_subagent_wakeup(Some(&live)),
            "a fresh live stream must keep the parent deferred"
        );

        let cancelled_token = CancellationToken::new();
        cancelled_token.cancel();
        let cancelled = StreamCancellationEntry {
            generation: 1,
            token: cancelled_token,
            retry_now: Arc::new(tokio::sync::Notify::new()),
            started_at: 0,
            last_progress_at: 1_000,
            last_progress_kind: StreamProgressKind::Started,
            last_progress_excerpt: String::new(),
        };
        assert!(stream_entry_is_idle_for_subagent_wakeup(Some(&cancelled)));

        let waiting_for_first_token = StreamCancellationEntry {
            generation: 1,
            token: CancellationToken::new(),
            retry_now: Arc::new(tokio::sync::Notify::new()),
            started_at: 0,
            last_progress_at: 0,
            last_progress_kind: StreamProgressKind::Started,
            last_progress_excerpt: String::new(),
        };
        assert!(
            !stream_entry_is_idle_for_subagent_wakeup(Some(&waiting_for_first_token)),
            "first-token wait and provider retries stay Started; cancelling them aborts a live parent turn"
        );
        assert!(stream_entry_is_idle_for_subagent_wakeup(None));

        let tool_in_flight = StreamCancellationEntry {
            generation: 1,
            token: CancellationToken::new(),
            retry_now: Arc::new(tokio::sync::Notify::new()),
            started_at: 0,
            last_progress_at: 0,
            last_progress_kind: StreamProgressKind::ToolCalls,
            last_progress_excerpt: String::new(),
        };
        assert!(
            !stream_entry_is_idle_for_subagent_wakeup(Some(&tool_in_flight)),
            "a live tool call must not be treated as an abandoned leftover stream"
        );
        let content_in_flight = StreamCancellationEntry {
            generation: 1,
            token: CancellationToken::new(),
            retry_now: Arc::new(tokio::sync::Notify::new()),
            started_at: 0,
            last_progress_at: 0,
            last_progress_kind: StreamProgressKind::Content,
            last_progress_excerpt: String::new(),
        };
        assert!(!stream_entry_is_idle_for_subagent_wakeup(Some(
            &content_in_flight
        )));
    }

    #[test]
    fn dispatched_success_preserves_open_ask_parent_block() {
        let mut task = terminal_notification_test_task(
            "child-awaiting",
            "Ask parent",
            TaskStatus::Blocked,
            Vec::new(),
        );
        task.completed_at = None;
        task.progress = 40;
        task.source = "subagent".to_string();
        task.blocked_reason = Some("awaiting parent: Which schema?".to_string());
        task.parent_task_id = Some("parent".to_string());
        let outcome = SendMessageOutcome {
            thread_id: "thread-child".to_string(),
            stream_generation: 0,
            interrupted_for_approval: false,
            terminated_for_budget: false,
            subagent_report: None,
            upstream_message: None,
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
        };
        apply_dispatched_task_success_update(&mut task, &outcome, &["nested-1".to_string()], 99);
        assert_eq!(task.status, TaskStatus::Blocked);
        assert_eq!(
            task.blocked_reason.as_deref(),
            Some("awaiting parent: Which schema?"),
            "dispatch success must not complete a child that is still waiting on ask_parent"
        );
        assert!(
            task.completed_at.is_none(),
            "answer_child only unblocks Blocked tasks; completing here would strand the open ask"
        );
    }

    #[test]
    fn dispatched_success_does_not_complete_child_after_parent_answers_mid_turn() {
        let mut task = terminal_notification_test_task(
            "child-answered-mid-turn",
            "Ask parent",
            TaskStatus::Queued,
            Vec::new(),
        );
        task.completed_at = None;
        task.progress = 40;
        task.source = "subagent".to_string();
        task.blocked_reason = None;
        task.parent_task_id = Some("parent".to_string());
        let outcome = SendMessageOutcome {
            thread_id: "thread-child".to_string(),
            stream_generation: 0,
            interrupted_for_approval: true,
            terminated_for_budget: false,
            subagent_report: None,
            upstream_message: None,
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
        };
        apply_dispatched_task_success_update(&mut task, &outcome, &[], 99);
        assert_eq!(
            task.status,
            TaskStatus::Queued,
            "answer_child clears blocked_reason while the original turn is still finishing; that turn must not complete the child"
        );
        assert!(
            task.completed_at.is_none(),
            "the injected parent answer needs a follow-up turn on the still-open child task"
        );
    }

    #[test]
    fn dispatched_subagent_report_discharges_production_report_obligation() {
        let mut task = terminal_notification_test_task(
            "contract-subagent",
            "Report a usable child outcome",
            TaskStatus::InProgress,
            Vec::new(),
        );
        task.source = "subagent".into();
        task.completion_contract = Some(TaskCompletionContract::for_new_task(
            task.description.clone(),
            &task.source,
        ));
        let outcome = SendMessageOutcome {
            thread_id: "thread-contract-subagent".into(),
            stream_generation: 0,
            interrupted_for_approval: false,
            terminated_for_budget: false,
            subagent_report: Some(SubagentTurnReport {
                status: SubagentReportStatus::Done,
                summary: "Implemented and verified the delegated slice".into(),
                reason: None,
            }),
            upstream_message: None,
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
        };

        apply_dispatched_task_success_update(&mut task, &outcome, &[], 99);

        assert_eq!(task.status, TaskStatus::Completed);
        assert!(task.completion_blockers().is_empty());
        let contract = task.completion_contract.as_ref().unwrap();
        assert!(contract.required_deliverables[0].completed);
        assert!(contract.required_deliverables[0].evidence.is_some());
    }

    #[test]
    fn dispatched_subagent_progress_without_report_cannot_complete() {
        let mut task = terminal_notification_test_task(
            "contract-subagent-progress",
            "Report a usable child outcome",
            TaskStatus::InProgress,
            Vec::new(),
        );
        task.source = "subagent".into();
        task.completion_contract = Some(TaskCompletionContract::for_new_task(
            task.description.clone(),
            &task.source,
        ));
        let outcome = SendMessageOutcome {
            thread_id: "thread-contract-subagent-progress".into(),
            stream_generation: 0,
            interrupted_for_approval: false,
            terminated_for_budget: false,
            subagent_report: None,
            upstream_message: None,
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
        };

        apply_dispatched_task_success_update(&mut task, &outcome, &[], 99);

        assert_eq!(task.status, TaskStatus::Blocked);
        assert!(task
            .blocked_reason
            .as_deref()
            .unwrap_or_default()
            .contains("usable subagent outcome report"));
    }

    #[test]
    fn dispatched_success_does_not_resurrect_cancelled_task() {
        let mut task = terminal_notification_test_task(
            "cancelled-live",
            "Select candidate",
            TaskStatus::Cancelled,
            Vec::new(),
        );
        let outcome = SendMessageOutcome {
            thread_id: "thread-cancelled-live".into(),
            stream_generation: 0,
            interrupted_for_approval: false,
            terminated_for_budget: false,
            subagent_report: None,
            upstream_message: None,
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
        };

        apply_dispatched_task_success_update(&mut task, &outcome, &[], 99);

        assert_eq!(task.status, TaskStatus::Cancelled);
        assert_eq!(task.completed_at, Some(3));
    }

    #[test]
    fn open_completion_contract_escalates_after_three_attempts() {
        let mut task = terminal_notification_test_task(
            "contract-open-cap",
            "Write the step marker",
            TaskStatus::InProgress,
            Vec::new(),
        );
        task.completed_at = None;
        task.completion_contract = Some(TaskCompletionContract {
            required_deliverables: vec![TaskCompletionRequirement {
                description: "required goal step completion marker".into(),
                completed: false,
                evidence: None,
            }],
            ..TaskCompletionContract::default()
        });
        let outcome = SendMessageOutcome {
            thread_id: "thread-contract-open-cap".into(),
            stream_generation: 0,
            interrupted_for_approval: false,
            terminated_for_budget: false,
            subagent_report: None,
            upstream_message: None,
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
        };

        apply_dispatched_task_success_update(&mut task, &outcome, &[], 10);
        assert_eq!(task.status, TaskStatus::Blocked);
        assert_eq!(
            task.completion_contract
                .as_ref()
                .map(|contract| contract.open_completion_attempts),
            Some(1)
        );

        apply_dispatched_task_success_update(&mut task, &outcome, &[], 11);
        assert_eq!(task.status, TaskStatus::Blocked);
        assert_eq!(
            task.completion_contract
                .as_ref()
                .map(|contract| contract.open_completion_attempts),
            Some(2)
        );

        apply_dispatched_task_success_update(&mut task, &outcome, &[], 12);
        assert_eq!(task.status, TaskStatus::AwaitingApproval);
        assert_eq!(
            task.awaiting_approval_id.as_deref(),
            Some("open-completion-contract:contract-open-cap")
        );
        assert!(task
            .blocked_reason
            .as_deref()
            .unwrap_or_default()
            .contains("after 3 attempts"));
    }

    #[test]
    fn goal_step_dispatch_budget_escalates_after_twelve_finishes() {
        let mut task = terminal_notification_test_task(
            "goal-step-dispatch-budget",
            "Keep working the step",
            TaskStatus::InProgress,
            Vec::new(),
        );
        task.source = "goal_run".to_string();
        task.completed_at = None;
        task.completion_contract = Some(TaskCompletionContract::default());
        let outcome = SendMessageOutcome {
            thread_id: "thread-goal-step-dispatch-budget".into(),
            stream_generation: 0,
            interrupted_for_approval: false,
            terminated_for_budget: false,
            subagent_report: None,
            upstream_message: None,
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
        };
        let active_children = ["child-1".to_string()];

        for finish in 1..GOAL_STEP_DISPATCH_FINISH_LIMIT {
            apply_dispatched_task_success_update(
                &mut task,
                &outcome,
                &active_children,
                finish as u64,
            );
            assert_eq!(task.status, TaskStatus::Blocked);
            assert_eq!(
                task.completion_contract
                    .as_ref()
                    .map(|contract| contract.dispatch_finish_attempts),
                Some(finish)
            );
        }

        apply_dispatched_task_success_update(
            &mut task,
            &outcome,
            &active_children,
            GOAL_STEP_DISPATCH_FINISH_LIMIT as u64,
        );
        assert_eq!(task.status, TaskStatus::AwaitingApproval);
        assert_eq!(
            task.awaiting_approval_id.as_deref(),
            Some("goal-step-dispatch-budget:goal-step-dispatch-budget")
        );
        assert_eq!(
            task.completion_contract
                .as_ref()
                .map(|contract| contract.dispatch_finish_attempts),
            Some(GOAL_STEP_DISPATCH_FINISH_LIMIT)
        );
    }

    #[test]
    fn auth_configuration_is_a_recoverable_goal_pause() {
        let message = format!(
            "invalid API key{}{}",
            crate::agent::llm_client::UPSTREAM_DIAGNOSTICS_MARKER,
            serde_json::json!({
                "class": "auth_configuration",
                "summary": "invalid API key",
                "diagnostics": {}
            })
        );
        assert_eq!(
            recoverable_goal_pause_reason(&message),
            Some(RecoverableGoalPauseReason::AuthConfiguration)
        );

        let mut task = terminal_notification_test_task(
            "auth-fail",
            "Call the provider",
            TaskStatus::InProgress,
            Vec::new(),
        );
        task.max_retries = 3;
        task.retry_count = 0;
        task.completed_at = None;
        apply_dispatched_task_failure_update(&mut task, &message, 1_000);
        assert_eq!(task.status, TaskStatus::Failed);
        assert_eq!(task.retry_count, 1);
    }

    #[tokio::test]
    async fn parent_completion_waits_for_open_child_ask_and_result_integration() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let parent = engine
            .enqueue_task(
                "Parent task".to_string(),
                "Integrate child after answering its ask".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some("thread-parent-contract-ask".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        let mut child = engine
            .enqueue_task(
                "Child task".to_string(),
                "Wait for the parent then report".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                Some(parent.id.clone()),
                Some("thread-child-contract-ask".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Failed;
        child.completed_at = Some(now_millis());
        child.last_error = Some("child terminated with an unresolved ask".to_string());
        child.result = None;
        child.parent_thread_id = Some("thread-parent-contract-ask".to_string());
        let ask_key = format!("ask_parent:{}:ask-open", child.id);
        engine
            .history
            .set_consolidation_state(
                &ask_key,
                &serde_json::json!({
                    "question": "Which result should be integrated?",
                    "options": [],
                    "asked_at": 1,
                    "timeout_minutes": 240,
                    "state": "open",
                    "answer": null,
                    "answer_delivered": false
                })
                .to_string(),
                now_millis(),
            )
            .await
            .unwrap();

        engine
            .record_subagent_outcome_on_parent(
                &child,
                TaskLogLevel::Error,
                "subagent failed",
                child.last_error.clone(),
            )
            .await;

        let mut persisted_parent = engine.task_by_id_for_dispatcher(&parent.id).await.unwrap();
        let reasons = persisted_parent
            .transition_to_terminal(TaskStatus::Completed, now_millis())
            .expect_err("parent completion must retain its child integration obligation");
        assert!(reasons
            .iter()
            .any(|reason| reason.contains("integrate child result")));
        let persisted_child = engine.task_by_id_for_dispatcher(&child.id).await.unwrap();
        let child_result = persisted_child
            .completion_contract
            .as_ref()
            .and_then(|contract| contract.child_result.as_ref())
            .unwrap();
        assert_eq!(child_result.open_ask_ids, vec!["ask-open"]);
        assert!(!child_result.asks_reconciled);
        assert!(!child_result.is_usable());
    }

    #[test]
    fn parent_completion_waits_for_child_result_integration_acknowledgement() {
        let mut task = terminal_notification_test_task(
            "contract-parent-child-result",
            "Integrate a required child result",
            TaskStatus::InProgress,
            Vec::new(),
        );
        let mut contract = TaskCompletionContract::for_new_task(task.description.clone(), "user");
        contract.require_child_result_integration("child-required");
        task.completion_contract = Some(contract);

        let reasons = task
            .transition_to_terminal(TaskStatus::Completed, 50)
            .expect_err("parent completion must wait for child-result integration");
        assert!(reasons
            .iter()
            .any(|reason| reason.contains("integrate child result: child-required")));

        task.completion_contract
            .as_mut()
            .unwrap()
            .acknowledge_child_result_integration("child-required", "continuation consumed result");
        task.transition_to_terminal(TaskStatus::Completed, 51)
            .expect("acknowledged child result should discharge the parent obligation");
        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn dispatched_progress_turn_stays_blocked_while_contract_work_remains() {
        let mut task = terminal_notification_test_task(
            "contract-progress",
            "Continue after progress",
            TaskStatus::InProgress,
            Vec::new(),
        );
        task.completed_at = None;
        task.progress = 55;
        task.completion_contract = Some(TaskCompletionContract {
            objective: "finish the background build and report verification".into(),
            outstanding_promised_actions: vec!["report final verification".into()],
            pending_operations: vec!["operation-build-1".into()],
            ..TaskCompletionContract::default()
        });
        let outcome = SendMessageOutcome {
            thread_id: "thread-contract-progress".to_string(),
            stream_generation: 0,
            interrupted_for_approval: false,
            terminated_for_budget: false,
            subagent_report: None,
            upstream_message: None,
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
        };

        apply_dispatched_task_success_update(&mut task, &outcome, &[], 99);

        assert_eq!(task.status, TaskStatus::Blocked);
        assert!(task.progress < 100);
        assert_eq!(task.completed_at, None);
        let reason = task.blocked_reason.as_deref().unwrap_or_default();
        assert!(reason.contains("report final verification"));
        assert!(reason.contains("operation-build-1"));
        assert!(task.logs.iter().any(|log| {
            log.message == "task produced progress but completion contract remains open"
        }));
    }

    #[tokio::test]
    async fn duplicate_terminal_child_events_queue_one_parent_continuation() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let parent_thread_id = "thread-duplicate-child-terminal";
        insert_parent_thread(&engine, parent_thread_id, "Integrate the child once").await;
        engine.begin_stream_cancellation(parent_thread_id).await;

        let mut child = engine
            .enqueue_task(
                "Duplicate child".to_string(),
                "Report once".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some(parent_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Completed;
        child.result = Some("stable child result".to_string());
        child.parent_thread_id = Some(parent_thread_id.to_string());

        for _ in 0..2 {
            engine
                .record_subagent_outcome_on_parent(
                    &child,
                    TaskLogLevel::Info,
                    "subagent completed",
                    None,
                )
                .await;
        }

        let continuations = engine
            .deferred_visible_thread_continuations_for(parent_thread_id)
            .await;
        assert_eq!(continuations.len(), 1);
        let system_messages = engine
            .threads
            .read()
            .await
            .get(parent_thread_id)
            .unwrap()
            .messages
            .iter()
            .filter(|message| {
                message.role == MessageRole::System && message.content.contains(&child.id)
            })
            .count();
        assert_eq!(system_messages, 1);
    }

    #[tokio::test]
    async fn child_result_delivered_by_parent_flush_is_acknowledged_without_a_second_wakeup() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let parent_thread_id = "thread-child-delivered-by-parent-flush";
        insert_parent_thread(&engine, parent_thread_id, "Integrate the child once").await;
        engine.begin_stream_cancellation(parent_thread_id).await;

        let mut child = engine
            .enqueue_task(
                "Busy parent child".to_string(),
                "Report while the parent streams".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some(parent_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Completed;
        child.result = Some("child result".to_string());
        child.parent_thread_id = Some(parent_thread_id.to_string());
        let child_id = child.id.clone();

        engine
            .record_subagent_outcome_on_parent(&child, TaskLogLevel::Info, "subagent completed", None)
            .await;
        assert_eq!(
            engine
                .deferred_visible_thread_continuations_for(parent_thread_id)
                .await
                .len(),
            1
        );

        engine
            .clear_deferred_visible_thread_continuations(parent_thread_id)
            .await;
        engine
            .record_subagent_outcome_on_parent(&child, TaskLogLevel::Info, "subagent completed", None)
            .await;

        assert!(
            engine
                .deferred_visible_thread_continuations_for(parent_thread_id)
                .await
                .is_empty(),
            "once the parent's own flush delivered the child result, the next pass must not wake the parent again"
        );
        let acknowledged = engine
            .task_by_id_for_dispatcher(&child_id)
            .await
            .and_then(|task| task.completion_contract)
            .and_then(|contract| contract.child_result)
            .and_then(|result| result.integration_acknowledged_at);
        assert!(
            acknowledged.is_some(),
            "a delivered child result must be acknowledged so later dispatcher ticks stop replaying it"
        );
    }

    #[tokio::test]
    async fn parent_notification_failure_stays_pending_and_retries() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let parent_thread_id = "thread-child-notification-retry";
        let mut child = engine
            .enqueue_task(
                "Retry child".to_string(),
                "Persist notification failure".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some(parent_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Failed;
        child.completed_at = Some(now_millis());
        child.last_error = Some("expected child failure".to_string());
        child.parent_thread_id = Some(parent_thread_id.to_string());

        engine
            .record_subagent_outcome_on_parent(
                &child,
                TaskLogLevel::Error,
                "subagent failed",
                child.last_error.clone(),
            )
            .await;
        let failed_delivery = engine.task_by_id_for_dispatcher(&child.id).await.unwrap();
        let result = failed_delivery
            .completion_contract
            .as_ref()
            .and_then(|contract| contract.child_result.as_ref())
            .unwrap();
        assert_eq!(
            result.parent_notification,
            crate::agent::types::ParentNotificationState::Pending
        );
        assert_eq!(
            result.parent_notification_error.as_deref(),
            Some("failed to append parent completion message")
        );

        insert_parent_thread(&engine, parent_thread_id, "Retry the notification").await;
        engine.begin_stream_cancellation(parent_thread_id).await;
        engine
            .record_subagent_outcome_on_parent(
                &failed_delivery,
                TaskLogLevel::Error,
                "subagent failed",
                failed_delivery.last_error.clone(),
            )
            .await;

        let retried = engine.task_by_id_for_dispatcher(&child.id).await.unwrap();
        let result = retried
            .completion_contract
            .as_ref()
            .and_then(|contract| contract.child_result.as_ref())
            .unwrap();
        assert_eq!(
            result.parent_notification,
            crate::agent::types::ParentNotificationState::Delivered
        );
        assert!(result.parent_notification_error.is_none());
        assert_eq!(
            engine
                .deferred_visible_thread_continuations_for(parent_thread_id)
                .await
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn pending_child_notification_replays_after_engine_restart() {
        let root = tempdir().expect("tempdir");
        let parent_thread_id = "thread-child-restart-replay";
        let child_id;
        {
            let manager = SessionManager::new_test(root.path()).await;
            let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
            insert_parent_thread(&engine, parent_thread_id, "Resume after restart").await;
            engine.persist_thread_by_id(parent_thread_id).await;
            engine.begin_stream_cancellation(parent_thread_id).await;
            let mut child = engine
                .enqueue_task(
                    "Restart child".to_string(),
                    "Persist terminal event".to_string(),
                    "normal",
                    None,
                    None,
                    Vec::new(),
                    None,
                    "subagent",
                    None,
                    None,
                    Some(parent_thread_id.to_string()),
                    Some("daemon".to_string()),
                )
                .await;
            child.status = TaskStatus::Failed;
            child.completed_at = Some(now_millis());
            child.last_error = Some("restart-safe child failure".to_string());
            child.result = None;
            child.parent_thread_id = Some(parent_thread_id.to_string());
            engine.reconcile_child_result_contract(&mut child).await;
            engine.history.upsert_agent_task(&child).await.unwrap();
            child_id = child.id.clone();
        }

        let manager = SessionManager::new_test(root.path()).await;
        let restarted = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        restarted.begin_stream_cancellation(parent_thread_id).await;
        restarted.replay_pending_child_parent_notifications().await;

        let reloaded = restarted
            .task_by_id_for_dispatcher(&child_id)
            .await
            .unwrap();
        let result = reloaded
            .completion_contract
            .as_ref()
            .and_then(|contract| contract.child_result.as_ref())
            .unwrap();
        assert_eq!(
            result.parent_notification,
            crate::agent::types::ParentNotificationState::Delivered
        );
        assert!(result.integration_acknowledged_at.is_none());
        assert_eq!(
            restarted
                .deferred_visible_thread_continuations_for(parent_thread_id)
                .await
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn chat_spawned_subagent_notifies_parent_thread_without_parent_task() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let parent_thread_id = "thread-chat-parent";
        insert_parent_thread(&engine, parent_thread_id, "Split this into child work").await;
        engine.begin_stream_cancellation(parent_thread_id).await;

        let mut child = engine
            .enqueue_task(
                "Chat-spawned child".to_string(),
                "Handle one slice".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                None,
                Some(parent_thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Completed;
        child.result = Some("slice complete".to_string());
        child.parent_thread_id = Some(parent_thread_id.to_string());

        engine
            .record_subagent_outcome_on_parent(
                &child,
                TaskLogLevel::Info,
                "subagent completed",
                None,
            )
            .await;

        let threads = engine.threads.read().await;
        let parent_thread = threads
            .get(parent_thread_id)
            .expect("parent thread should exist");
        assert!(parent_thread.messages.iter().any(|message| {
            message.role == MessageRole::System && message.content.contains("slice complete")
        }));
        let continuations = engine
            .deferred_visible_thread_continuations_for(parent_thread_id)
            .await;
        assert_eq!(
            continuations.len(),
            1,
            "chat-spawned children still have to wake the parent thread"
        );
    }

    #[tokio::test]
    async fn budget_exceeded_subagent_notifies_child_and_parent_threads() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let parent_thread_id = "thread-parent";
        let child_thread_id = "thread-child";
        let parent_task_id = "task-parent";
        let child_task_id = "task-child";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                parent_thread_id.to_string(),
                AgentThread {
                    id: parent_thread_id.to_string(),
                    agent_name: Some("Svarog".to_string()),
                    title: "Parent".to_string(),
                    messages: vec![AgentMessage::user("Continue until done", 1)],
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
            threads.insert(
                child_thread_id.to_string(),
                AgentThread {
                    id: child_thread_id.to_string(),
                    agent_name: Some("Dazhbog".to_string()),
                    title: "Child".to_string(),
                    messages: vec![AgentMessage::user("Do the refactor", 2)],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    created_at: 2,
                    updated_at: 2,
                },
            );
        }

        let parent_task = AgentTask {
            id: parent_task_id.to_string(),
            title: "Parent".to_string(),
            description: "Wait for child".to_string(),
            status: TaskStatus::Blocked,
            priority: TaskPriority::Normal,
            progress: 90,
            created_at: 1,
            started_at: Some(1),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(parent_thread_id.to_string()),
            source: "user".to_string(),
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
            blocked_reason: Some(format!("waiting for subagents: {child_task_id}")),
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
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        };
        let child_task = AgentTask {
            id: child_task_id.to_string(),
            title: "Child".to_string(),
            description: "Refactor everything".to_string(),
            status: TaskStatus::BudgetExceeded,
            priority: TaskPriority::Normal,
            progress: 100,
            created_at: 2,
            started_at: Some(2),
            completed_at: Some(3),
            error: Some("execution budget exceeded for this thread".to_string()),
            result: None,
            thread_id: Some(child_thread_id.to_string()),
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
            parent_task_id: Some(parent_task_id.to_string()),
            parent_thread_id: Some(parent_thread_id.to_string()),
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: Some("execution budget exceeded for this thread".to_string()),
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: Some("execution budget exceeded for this thread".to_string()),
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        };

        {
            let mut tasks = engine.tasks.lock().await;
            tasks.push_back(parent_task);
            tasks.push_back(child_task.clone());
        }

        engine
            .handle_budget_exceeded_task_terminal_state(&child_task)
            .await;
        engine
            .record_subagent_outcome_on_parent(
                &child_task,
                TaskLogLevel::Warn,
                "subagent budget exceeded",
                child_task.blocked_reason.clone(),
            )
            .await;

        let threads = engine.threads.read().await;
        let child_thread = threads
            .get(child_thread_id)
            .expect("child thread should exist");
        assert!(child_thread.messages.iter().any(|message| {
            message.role == MessageRole::System
                && message
                    .content
                    .contains("Task budget exceeded for this thread")
                && message
                    .content
                    .contains("locked for further operator messages")
        }));

        let parent_thread = threads
            .get(parent_thread_id)
            .expect("parent thread should exist");
        assert!(parent_thread.messages.iter().any(|message| {
            message.role == MessageRole::System
                && message.content.contains("child_task_id `child`")
                && message.content.contains("extend_subagent_budget")
        }));
    }

    #[tokio::test]
    async fn record_subagent_outcome_updates_persisted_parent_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let parent = engine
            .enqueue_task(
                "Parent task".to_string(),
                "Wait for subagent outcome".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some("thread-parent-persisted-outcome".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        let mut child = engine
            .enqueue_task(
                "Child task".to_string(),
                "Report failure to parent".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                Some(parent.id.clone()),
                Some("thread-parent-persisted-outcome".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        child.status = TaskStatus::Failed;
        child.completed_at = Some(now_millis());
        child.last_error = Some("child failed".to_string());
        {
            let mut tasks = engine.tasks.lock().await;
            let persisted = tasks
                .iter_mut()
                .find(|entry| entry.id == child.id)
                .expect("child task should exist");
            *persisted = child.clone();
        }
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        engine
            .record_subagent_outcome_on_parent(
                &child,
                TaskLogLevel::Error,
                "subagent failed",
                child.last_error.clone(),
            )
            .await;

        let updated_parent = engine
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some(parent.id.clone()),
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
            .expect("persisted parent should remain queryable");

        assert!(
            updated_parent.logs.iter().any(|entry| {
                entry.phase == "subagent"
                    && entry.message.contains("subagent failed")
                    && entry.message.contains(&child.id)
            }),
            "persisted parent task should record subagent outcome"
        );
    }

    #[tokio::test]
    async fn persist_tasks_backstop_updates_live_and_durable_task_state() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let task = engine
            .enqueue_task(
                "Guard live completion".to_string(),
                "Do not complete while an operation remains pending".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some("thread-live-contract-guard".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        {
            let mut tasks = engine.tasks.lock().await;
            let live = tasks
                .iter_mut()
                .find(|entry| entry.id == task.id)
                .expect("enqueued task should exist");
            live.status = TaskStatus::Completed;
            live.progress = 100;
            live.completed_at = Some(now_millis());
            let contract = live
                .completion_contract
                .as_mut()
                .expect("new production task should have a completion contract");
            contract.pending_operations = vec!["operation-live-guard".to_string()];
            contract.terminal_status = Some(TaskStatus::Completed);
        }

        engine.persist_tasks().await;

        let live = engine
            .tasks
            .lock()
            .await
            .iter()
            .find(|entry| entry.id == task.id)
            .cloned()
            .expect("live task should remain available");
        assert_eq!(live.status, TaskStatus::Blocked);
        assert!(live.progress < 100);
        assert_eq!(live.completed_at, None);
        assert!(live
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("operation-live-guard")));
        assert_eq!(
            live.completion_contract
                .as_ref()
                .expect("contract should remain attached")
                .terminal_status,
            None
        );

        let durable = engine
            .history
            .list_agent_tasks()
            .await
            .expect("durable task query should succeed")
            .into_iter()
            .find(|entry| entry.id == task.id)
            .expect("durable task should remain queryable");
        assert_eq!(durable.status, TaskStatus::Blocked);
        assert_eq!(durable.completed_at, None);
    }

    #[tokio::test]
    async fn dispatch_ready_tasks_uses_persisted_queued_tasks_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-dispatch-persisted-ready";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Dispatch persisted ready task".to_string(),
                messages: Vec::new(),
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
                "Persisted ready dispatch".to_string(),
                "Dispatcher should select this from SQLite after live queue clear".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some(thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        task.thread_id = Some(thread_id.to_string());
        task.override_provider = Some("missing-provider-for-ready-dispatch-test".to_string());
        {
            let mut tasks = engine.tasks.lock().await;
            let persisted = tasks
                .iter_mut()
                .find(|entry| entry.id == task.id)
                .expect("task should exist");
            *persisted = task.clone();
        }
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        Arc::clone(&engine)
            .dispatch_ready_tasks()
            .await
            .expect("dispatch should not fail");

        timeout(Duration::from_millis(500), async {
            loop {
                let persisted = engine
                    .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                        id: Some(task.id.clone()),
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
                    .expect("persisted task should remain queryable");
                if persisted.status == TaskStatus::FailedAnalyzing {
                    assert_eq!(persisted.retry_count, 1);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("persisted queued task should be dispatched and record local failure");
    }

    #[tokio::test]
    async fn dispatch_ready_tasks_uses_persisted_goal_statuses_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-dispatch-persisted-goal-ready";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Dispatch persisted goal task".to_string(),
                messages: Vec::new(),
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

        let goal_run = engine
            .start_goal_run(
                "Dispatch persisted goal task".to_string(),
                Some("Dispatch persisted goal task".to_string()),
                Some(thread_id.to_string()),
                None,
                Some("normal"),
                None,
                None,
                None,
            )
            .await;
        engine
            .enqueue_goal_worker(&goal_run.id)
            .await
            .expect("enqueue worker");
        let task_id = engine
            .get_goal_run(&goal_run.id)
            .await
            .expect("goal")
            .active_task_id
            .clone()
            .expect("worker task");
        {
            let mut tasks = engine.tasks.lock().await;
            let persisted = tasks
                .iter_mut()
                .find(|entry| entry.id == task_id)
                .expect("task should exist");
            persisted.status = TaskStatus::Queued;
            persisted.started_at = None;
            persisted.override_provider =
                Some("missing-provider-for-ready-goal-dispatch-test".to_string());
        }
        engine.persist_goal_runs().await;
        engine.persist_tasks().await;
        engine.goal_runs.lock().await.clear();
        engine.tasks.lock().await.clear();

        Arc::clone(&engine)
            .dispatch_ready_tasks()
            .await
            .expect("dispatch should not fail");

        let persisted = engine
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
            .next()
            .expect("persisted task should remain queryable");
        assert_eq!(
            persisted.status,
            TaskStatus::Blocked,
            "persisted goal worker should be selected from sqlite and fail provider preflight"
        );
        assert!(
            persisted
                .blocked_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("no_compatible_provider")),
            "expected provider preflight block, got {:?}",
            persisted.blocked_reason
        );
        let goal = engine
            .get_goal_run(&goal_run.id)
            .await
            .expect("goal should remain available from sqlite");
        assert_eq!(goal.status, GoalRunStatus::Running);
        assert_eq!(goal.active_task_id.as_deref(), Some(task_id.as_str()));
    }

    #[tokio::test]
    async fn dispatched_task_failure_updates_persisted_task_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-dispatch-persisted-failure";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Dispatch failure".to_string(),
                messages: Vec::new(),
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
                "Persisted dispatch failure".to_string(),
                "Missing provider should fail before network IO".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some(thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        task.thread_id = Some(thread_id.to_string());
        task.override_provider = Some("missing-provider-for-dispatch-test".to_string());
        {
            let mut tasks = engine.tasks.lock().await;
            let persisted = tasks
                .iter_mut()
                .find(|entry| entry.id == task.id)
                .expect("task should exist");
            *persisted = task.clone();
        }
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        engine
            .execute_dispatched_task(task.clone())
            .await
            .expect("dispatcher should record the send failure");

        let persisted = engine
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some(task.id.clone()),
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
            .expect("persisted task should remain queryable");

        assert_eq!(persisted.status, TaskStatus::FailedAnalyzing);
        assert_eq!(persisted.retry_count, 1);
        assert!(persisted
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("missing-provider-for-dispatch-test")));
    }

    #[tokio::test]
    async fn dispatched_task_success_blocks_persisted_task_on_persisted_active_child_after_live_queue_clear(
    ) {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
        config.base_url = spawn_dispatcher_stub_assistant_server("Parent turn completed.").await;
        config.model = "gpt-4o-mini".to_string();
        config.api_key = "test-key".to_string();
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let thread_id = "thread-dispatch-persisted-success";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Dispatch success".to_string(),
                messages: Vec::new(),
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

        let mut parent = engine
            .enqueue_task(
                "Persisted dispatch success".to_string(),
                "Successful dispatcher sends should persist the parent status".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some(thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        parent.thread_id = Some(thread_id.to_string());
        let child = engine
            .enqueue_task(
                "Active persisted child".to_string(),
                "Keep the parent blocked after the parent turn completes".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                Some(parent.id.clone()),
                Some(thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        {
            let mut tasks = engine.tasks.lock().await;
            let persisted = tasks
                .iter_mut()
                .find(|entry| entry.id == parent.id)
                .expect("parent task should exist");
            *persisted = parent.clone();
        }
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        engine
            .execute_dispatched_task(parent.clone())
            .await
            .expect("dispatcher should record the successful send");

        let persisted = engine
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some(parent.id.clone()),
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
            .expect("persisted parent should remain queryable");

        assert_eq!(persisted.status, TaskStatus::Blocked);
        assert_eq!(persisted.progress, 90);
        assert_eq!(persisted.completed_at, None);
        assert!(persisted
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| reason.contains(&child.id)));
        assert!(persisted.logs.iter().any(|entry| {
            entry.phase == "subagent"
                && entry.message == "task waiting for spawned subagents to finish"
        }));
    }

    fn terminal_notification_test_task(
        id: &str,
        title: &str,
        status: TaskStatus,
        notify_channels: Vec<String>,
    ) -> AgentTask {
        AgentTask {
            id: id.to_string(),
            title: title.to_string(),
            description: "deliver a terminal notification".to_string(),
            status,
            priority: TaskPriority::Normal,
            progress: 100,
            created_at: 1,
            started_at: Some(2),
            completed_at: Some(3),
            error: None,
            result: None,
            thread_id: Some("thread-terminal-notify".to_string()),
            source: "user".to_string(),
            notify_on_complete: true,
            notify_channels,
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
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        }
    }

    #[test]
    fn event_trigger_terminal_notifications_share_one_id_per_trigger_title() {
        let mut first = terminal_notification_test_task(
            "task-fire-1",
            "Handle trigger: File changed: /tmp/watched",
            TaskStatus::Completed,
            vec!["in-app".to_string()],
        );
        first.source = "event_trigger".to_string();
        let mut second = first.clone();
        second.id = "task-fire-2".to_string();

        let first_id = terminal_task_notification_id(&first, "task_completed");
        assert_eq!(
            first_id,
            terminal_task_notification_id(&second, "task_completed"),
            "re-fires of the same trigger must upsert one inbox row instead of stacking duplicates"
        );
        assert!(first_id.starts_with("task-terminal:trigger:"));

        let mut other_trigger = first.clone();
        other_trigger.title = "Handle trigger: File changed: /tmp/other".to_string();
        assert_ne!(
            terminal_task_notification_id(&other_trigger, "task_completed"),
            first_id,
            "distinct triggers must keep distinct inbox rows"
        );
        assert_ne!(
            terminal_task_notification_id(&first, "task_failed"),
            first_id,
            "a failure must not overwrite the completion notification for the same trigger"
        );
    }

    #[test]
    fn non_trigger_terminal_notifications_keep_per_task_ids() {
        let task = terminal_notification_test_task(
            "task-user-1",
            "Ship release",
            TaskStatus::Completed,
            vec!["in-app".to_string()],
        );
        assert_eq!(
            terminal_task_notification_id(&task, "task_completed"),
            "task-terminal:task-user-1:task_completed"
        );
    }

    #[tokio::test]
    async fn completed_task_notification_persists_inbox_and_routes_gateway_channels() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.gateway.slack_channel_filter = "C123".to_string();
        config.gateway.discord_channel_filter = "D456".to_string();
        config.gateway.telegram_allowed_chats = "T789".to_string();
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        engine.set_gateway_ipc_sender(Some(tx)).await;
        let mut events = engine.event_tx.subscribe();

        let mut task = terminal_notification_test_task(
            "task-terminal-completed",
            "Ship release",
            TaskStatus::Completed,
            vec![
                "in-app".to_string(),
                "slack".to_string(),
                "discord".to_string(),
                "telegram".to_string(),
            ],
        );
        task.result = Some("release artifacts published successfully".to_string());

        let engine_for_task = engine.clone();
        let notify_task = tokio::spawn(async move {
            engine_for_task.notify_task_terminal_state(&task).await;
        });

        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("notification event should arrive")
            .expect("notification event should exist");
        match event {
            AgentEvent::Notification {
                title,
                body,
                severity,
                channels,
            } => {
                assert_eq!(title, "Task completed: Ship release");
                assert_eq!(severity, NotificationSeverity::Info);
                assert_eq!(channels, vec!["in-app", "slack", "discord", "telegram"]);
                assert!(body.contains("Status: completed"));
                assert!(body.contains("Result: release artifacts published successfully"));
            }
            other => panic!("expected notification event, got {other:?}"),
        }

        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("notification inbox event should arrive")
            .expect("notification inbox event should exist");
        match event {
            AgentEvent::NotificationInboxUpsert { notification } => {
                assert_eq!(
                    notification.id,
                    "task-terminal:task-terminal-completed:task_completed"
                );
                assert_eq!(notification.kind, "task_completed");
                assert_eq!(notification.severity, "info");
                assert_eq!(notification.subtitle.as_deref(), Some("completed"));
            }
            other => panic!("expected notification inbox upsert, got {other:?}"),
        }

        for (platform, channel_id) in [("slack", "C123"), ("discord", "D456"), ("telegram", "T789")]
        {
            let request = match timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("gateway send request should be emitted")
                .expect("gateway send request should exist")
            {
                zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
                other => panic!("expected GatewaySendRequest, got {other:?}"),
            };
            assert_eq!(request.platform, platform);
            assert_eq!(request.channel_id, channel_id);
            assert!(request.content.contains("Task completed: Ship release"));

            engine
                .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
                    correlation_id: request.correlation_id.clone(),
                    platform: platform.to_string(),
                    channel_id: channel_id.to_string(),
                    requested_channel_id: Some(channel_id.to_string()),
                    delivery_id: Some(format!("delivery-{platform}")),
                    ok: true,
                    error: None,
                    completed_at_ms: now_millis(),
                })
                .await;
        }

        notify_task.await.expect("notification task should finish");

        let notifications = engine
            .history
            .list_notifications(false, Some(10))
            .await
            .expect("list notifications should succeed");
        let notification = notifications
            .iter()
            .find(|entry| entry.id == "task-terminal:task-terminal-completed:task_completed")
            .expect("persisted completed task notification should exist");
        assert!(notification.body.contains("Task: Ship release"));
        assert!(notification.body.contains("Thread: thread-terminal-notify"));
    }

    #[tokio::test]
    async fn failed_task_notification_uses_error_severity_and_persists_inbox() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.gateway.slack_channel_filter = "C123".to_string();
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        engine.set_gateway_ipc_sender(Some(tx)).await;
        let mut events = engine.event_tx.subscribe();

        let mut task = terminal_notification_test_task(
            "task-terminal-failed",
            "Ship release",
            TaskStatus::Failed,
            vec!["slack".to_string()],
        );
        task.last_error = Some("provider request timed out after 30s".to_string());
        task.error = task.last_error.clone();
        task.retry_count = 4;

        let engine_for_task = engine.clone();
        let notify_task = tokio::spawn(async move {
            engine_for_task.notify_task_terminal_state(&task).await;
        });

        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("notification event should arrive")
            .expect("notification event should exist");
        match event {
            AgentEvent::Notification {
                title,
                body,
                severity,
                channels,
            } => {
                assert_eq!(title, "Task failed: Ship release");
                assert_eq!(severity, NotificationSeverity::Error);
                assert_eq!(channels, vec!["slack"]);
                assert!(body.contains("Status: failed"));
                assert!(body.contains("Error: provider request timed out after 30s"));
            }
            other => panic!("expected notification event, got {other:?}"),
        }

        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("notification inbox event should arrive")
            .expect("notification inbox event should exist");
        match event {
            AgentEvent::NotificationInboxUpsert { notification } => {
                assert_eq!(notification.kind, "task_failed");
                assert_eq!(notification.severity, "error");
                assert!(notification
                    .body
                    .contains("Error: provider request timed out after 30s"));
            }
            other => panic!("expected notification inbox upsert, got {other:?}"),
        }

        let request = match timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("gateway send request should be emitted")
            .expect("gateway send request should exist")
        {
            zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        };
        assert_eq!(request.platform, "slack");
        assert_eq!(request.channel_id, "C123");
        assert!(request.content.contains("Task failed: Ship release"));

        engine
            .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
                correlation_id: request.correlation_id.clone(),
                platform: "slack".to_string(),
                channel_id: "C123".to_string(),
                requested_channel_id: Some("C123".to_string()),
                delivery_id: Some("delivery-slack".to_string()),
                ok: true,
                error: None,
                completed_at_ms: now_millis(),
            })
            .await;

        notify_task.await.expect("notification task should finish");

        let notifications = engine
            .history
            .list_notifications(false, Some(10))
            .await
            .expect("list notifications should succeed");
        let notification = notifications
            .iter()
            .find(|entry| entry.id == "task-terminal:task-terminal-failed:task_failed")
            .expect("persisted failed task notification should exist");
        assert_eq!(notification.severity, "error");
        assert_eq!(notification.subtitle.as_deref(), Some("failed"));
    }
}
