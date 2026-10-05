#![allow(dead_code)]

use super::*;

const TASK_APPROVAL_REASON_PREFIX: &str = "waiting for operator approval: ";

fn apply_weles_task_target(task: &mut AgentTask, persona_prompt: &str) {
    task.sub_agent_def_id =
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string());
    task.override_system_prompt = Some(match task.override_system_prompt.take() {
        Some(existing) if !existing.trim().is_empty() => {
            format!("{persona_prompt}\n\n{existing}")
        }
        _ => persona_prompt.to_string(),
    });
}

fn task_matches_list_query(task: &AgentTask, query: &crate::history::AgentTaskListQuery) -> bool {
    if let Some(id) = query.id.as_deref().filter(|value| !value.is_empty()) {
        if task.id != id {
            return false;
        }
    }
    if let Some(status) = query.status.as_deref().filter(|value| !value.is_empty()) {
        let task_status = serde_json::to_value(task.status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned));
        if task_status.as_deref() != Some(status) {
            return false;
        }
    }
    if !query.statuses.is_empty() {
        let task_status = serde_json::to_value(task.status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned));
        let matches_status = task_status.as_deref().is_some_and(|task_status| {
            query
                .statuses
                .iter()
                .map(|status| status.trim())
                .any(|status| !status.is_empty() && status == task_status)
        });
        if !matches_status {
            return false;
        }
    }
    if let Some(source) = query.source.as_deref().filter(|value| !value.is_empty()) {
        if task.source != source {
            return false;
        }
    }
    if let Some(thread_id) = query.thread_id.as_deref().filter(|value| !value.is_empty()) {
        if task.thread_id.as_deref() != Some(thread_id) {
            return false;
        }
    }
    if !query.thread_ids.is_empty()
        && !task.thread_id.as_deref().is_some_and(|thread_id| {
            query
                .thread_ids
                .iter()
                .map(|candidate| candidate.trim())
                .any(|candidate| !candidate.is_empty() && candidate == thread_id)
        })
    {
        return false;
    }
    if let Some(goal_run_id) = query
        .goal_run_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        if task.goal_run_id.as_deref() != Some(goal_run_id) {
            return false;
        }
    }
    if let Some(parent_task_id) = query
        .parent_task_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        if task.parent_task_id.as_deref() != Some(parent_task_id) {
            return false;
        }
    }
    if let Some(approval_id) = query
        .awaiting_approval_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        if task.awaiting_approval_id.as_deref() != Some(approval_id) {
            return false;
        }
    }
    if query.supervisor_config_present && task.supervisor_config.is_none() {
        return false;
    }
    if query.exclude_terminal_statuses
        && crate::agent::task_scheduler::is_task_terminal_status(task.status)
    {
        return false;
    }
    true
}

fn task_recent_activity_at(task: &AgentTask) -> u64 {
    task.completed_at
        .or(task.started_at)
        .unwrap_or(task.created_at)
}

fn filter_task_snapshot_for_query(
    mut tasks: Vec<AgentTask>,
    query: &crate::history::AgentTaskListQuery,
) -> Vec<AgentTask> {
    tasks.retain(|task| task_matches_list_query(task, query));
    if query.order_by_recent_activity_desc {
        tasks.sort_by_key(|task| std::cmp::Reverse(task_recent_activity_at(task)));
    }
    if let Some(limit) = query.limit {
        tasks.truncate(limit.max(1));
    }
    tasks
}

fn matches_parent_thread_subagent(
    task: &AgentTask,
    parent_thread_id: &str,
    status: Option<&str>,
) -> bool {
    if task.source != "subagent" || task.parent_thread_id.as_deref() != Some(parent_thread_id) {
        return false;
    }
    let Some(status) = status else {
        return true;
    };
    serde_json::to_value(task.status)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .is_some_and(|value| value == status)
}

fn is_policy_escalation_approval(approval_id: &str) -> bool {
    approval_id.starts_with("policy-escalation-")
}

impl AgentEngine {
    fn approval_command_from_task(task: &AgentTask) -> Option<String> {
        task.blocked_reason
            .as_deref()
            .and_then(|reason| reason.strip_prefix(TASK_APPROVAL_REASON_PREFIX))
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .map(str::to_string)
            .or_else(|| {
                task.command
                    .as_deref()
                    .map(str::trim)
                    .filter(|command| !command.is_empty())
                    .map(str::to_string)
            })
    }

    pub(crate) async fn remember_pending_approval_command(
        &self,
        pending_approval: &ToolPendingApproval,
    ) {
        self.pending_approval_commands.write().await.insert(
            pending_approval.approval_id.clone(),
            pending_approval.command.clone(),
        );
    }

    pub(crate) async fn forget_pending_approval_command(&self, approval_id: &str) {
        self.pending_approval_commands
            .write()
            .await
            .remove(approval_id);
    }

    pub(in crate::agent) async fn has_policy_escalation_session_grant(
        &self,
        thread_id: &str,
    ) -> bool {
        self.policy_escalation_session_grants
            .read()
            .await
            .contains(thread_id)
    }

    pub(in crate::agent) async fn store_policy_escalation_session_grant(&self, thread_id: &str) {
        self.policy_escalation_session_grants
            .write()
            .await
            .insert(thread_id.to_string());
    }

    async fn persist_task_approval_rules(&self) {
        let rules = self.task_approval_rules.read().await.clone();
        if let Err(error) =
            persist_json(&self.data_dir.join("task-approval-rules.json"), &rules).await
        {
            tracing::warn!(%error, "failed to persist task approval rules");
        }
    }

    pub async fn list_task_approval_rules(&self) -> Vec<zorai_protocol::TaskApprovalRule> {
        self.task_approval_rules.read().await.clone()
    }

    pub(in crate::agent) async fn find_task_approval_rule_for_command(
        &self,
        command: &str,
    ) -> Option<zorai_protocol::TaskApprovalRule> {
        self.task_approval_rules
            .read()
            .await
            .iter()
            .find(|rule| rule.command == command)
            .cloned()
    }

    pub(in crate::agent) async fn mark_task_approval_rule_used(&self, command: &str) -> bool {
        let mut rules = self.task_approval_rules.write().await;
        let Some(rule) = rules.iter_mut().find(|rule| rule.command == command) else {
            return false;
        };
        rule.last_used_at = Some(now_millis());
        rule.use_count = rule.use_count.saturating_add(1);
        drop(rules);
        self.persist_task_approval_rules().await;
        true
    }

    pub async fn create_task_approval_rule_from_pending(
        &self,
        approval_id: &str,
    ) -> Result<Option<zorai_protocol::TaskApprovalRule>> {
        let persisted_command = self
            .history
            .pending_agent_task_approval_command(approval_id)
            .await?;
        let command = match persisted_command {
            Some(command) => Some(command),
            None => match {
                let tasks = self.tasks.lock().await;
                tasks.iter().find_map(|task| {
                    (task.awaiting_approval_id.as_deref() == Some(approval_id))
                        .then(|| Self::approval_command_from_task(task))
                        .flatten()
                })
            } {
                Some(command) => Some(command),
                None => self
                    .pending_approval_commands
                    .read()
                    .await
                    .get(approval_id)
                    .cloned(),
            },
        };
        let Some(command) = command else {
            return Ok(None);
        };

        let now = now_millis();
        let rule = {
            let mut rules = self.task_approval_rules.write().await;
            if let Some(existing) = rules.iter_mut().find(|rule| rule.command == command) {
                existing.last_used_at = Some(now);
                existing.clone()
            } else {
                let rule = zorai_protocol::TaskApprovalRule {
                    id: format!("task-approval-rule-{}", Uuid::new_v4()),
                    command,
                    created_at: now,
                    last_used_at: None,
                    use_count: 0,
                };
                rules.push(rule.clone());
                rules.sort_by(|left, right| left.command.cmp(&right.command));
                rule
            }
        };
        self.persist_task_approval_rules().await;
        Ok(Some(rule))
    }

    pub async fn revoke_task_approval_rule(&self, rule_id: &str) -> bool {
        let mut rules = self.task_approval_rules.write().await;
        let original_len = rules.len();
        rules.retain(|rule| rule.id != rule_id);
        let removed = rules.len() != original_len;
        drop(rules);
        if removed {
            self.persist_task_approval_rules().await;
        }
        removed
    }

    pub(in crate::agent) async fn auto_approve_task_if_rule_matches(
        &self,
        task_id: &str,
        thread_id: &str,
        pending_approval: &ToolPendingApproval,
    ) -> bool {
        if !self
            .mark_task_approval_rule_used(&pending_approval.command)
            .await
        {
            return false;
        }
        self.mark_task_awaiting_approval(task_id, thread_id, pending_approval)
            .await;
        self.handle_task_approval_resolution(
            &pending_approval.approval_id,
            zorai_protocol::ApprovalDecision::ApproveOnce,
        )
        .await;
        self.emit_workflow_notice(
            thread_id,
            "task-approval-auto-approved",
            "Applied a saved always-approve rule and continued automatically.",
            Some(pending_approval.command.clone()),
        );
        true
    }

    async fn pending_approval_task_for_resolution(
        &self,
        approval_id: &str,
        source: Option<&str>,
    ) -> Option<AgentTask> {
        let task = self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: source.map(ToOwned::to_owned),
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: Some(approval_id.to_string()),
                supervisor_config_present: false,
                exclude_terminal_statuses: false,
                order_by_recent_activity_desc: true,
                limit: Some(1),
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await
            .into_iter()
            .next();
        if let Some(task) = task {
            {
                let mut tasks = self.tasks.lock().await;
                if !tasks.iter().any(|entry| entry.id == task.id) {
                    tasks.push_back(task.clone());
                }
            }
            return Some(task);
        }

        let task = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .find(|task| {
                    task.awaiting_approval_id.as_deref() == Some(approval_id)
                        && source.is_none_or(|source| task.source == source)
                })
                .cloned()
        };
        let task = task?;
        {
            let mut tasks = self.tasks.lock().await;
            if !tasks.iter().any(|entry| entry.id == task.id) {
                tasks.push_back(task.clone());
            }
        }
        Some(task)
    }

    pub async fn add_task(
        &self,
        title: String,
        description: String,
        priority: &str,
        command: Option<String>,
        session_id: Option<String>,
        dependencies: Vec<String>,
    ) -> String {
        self.enqueue_task(
            title,
            description,
            priority,
            command,
            session_id,
            dependencies,
            None,
            "user",
            None,
            None,
            None,
            None,
        )
        .await
        .id
    }

    pub async fn enqueue_task(
        &self,
        title: String,
        description: String,
        priority: &str,
        command: Option<String>,
        session_id: Option<String>,
        dependencies: Vec<String>,
        scheduled_at: Option<u64>,
        source: &str,
        goal_run_id: Option<String>,
        parent_task_id: Option<String>,
        parent_thread_id: Option<String>,
        runtime: Option<String>,
    ) -> AgentTask {
        let id = format!("task_{}", Uuid::new_v4());
        let now = now_millis();
        let adaptation_mode = {
            let model = self.operator_model.read().await;
            SatisfactionAdaptationMode::from_label(&model.operator_satisfaction.label)
        };
        let initial_schedule_reason = scheduled_at
            .filter(|deadline| *deadline > now)
            .map(describe_scheduled_time);
        let default_max_retries = self.config.read().await.max_retries.max(1);
        let max_retries = if goal_run_id.is_some() {
            adaptation_mode.max_goal_task_retries(default_max_retries)
        } else {
            default_max_retries
        };
        // New rows always carry an explicit objective. Only records loaded
        // from the pre-contract schema retain `None` legacy semantics.
        let completion_contract = Some(TaskCompletionContract::for_new_task(
            description.clone(),
            source,
        ));
        let task = AgentTask {
            id: id.clone(),
            title,
            description,
            status: if initial_schedule_reason.is_some() {
                TaskStatus::Blocked
            } else {
                TaskStatus::Queued
            },
            priority: parse_priority_str(priority),
            progress: 0,
            created_at: now,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: source.into(),
            notify_on_complete: true,
            notify_channels: vec!["in-app".into()],
            dependencies,
            command,
            session_id,
            goal_run_id,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id,
            parent_thread_id,
            runtime: runtime.unwrap_or_else(|| "daemon".to_string()),
            retry_count: 0,
            max_retries,
            next_retry_at: None,
            scheduled_at,
            blocked_reason: initial_schedule_reason.clone(),
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            completion_contract,
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
            logs: vec![make_task_log_entry(
                0,
                TaskLogLevel::Info,
                "queue",
                if initial_schedule_reason.is_some() {
                    "task scheduled"
                } else {
                    "task enqueued"
                },
                initial_schedule_reason,
            )],
        };
        let mut task = task;
        super::enforce_goal_task_autonomy_tool_blacklist(&mut task);

        self.tasks.lock().await.push_back(task);
        let task = self
            .tasks
            .lock()
            .await
            .iter()
            .find(|task| task.id == id)
            .cloned()
            .expect("enqueued task missing from queue");
        self.record_memory_graph_from_task(&task).await;
        self.persist_tasks().await;

        self.emit_task_update(&task, Some(status_message(&task).into()));

        task
    }

    pub(in crate::agent) async fn retarget_task_to_weles(
        &self,
        task_id: &str,
    ) -> Option<AgentTask> {
        let persona_prompt = crate::agent::agent_identity::build_weles_persona_prompt(
            crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
        );
        let mut updated_live_task = false;
        let mut updated = {
            let mut tasks = self.tasks.lock().await;
            tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .map(|task| {
                    apply_weles_task_target(task, &persona_prompt);
                    updated_live_task = true;
                    task.clone()
                })
        };
        if updated.is_none() {
            let Some(mut task) = self
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
            else {
                return None;
            };
            apply_weles_task_target(&mut task, &persona_prompt);
            if let Err(error) = self.history.upsert_agent_task(&task).await {
                tracing::warn!(
                    task_id,
                    "failed to persist Weles task retarget update: {error}"
                );
                return None;
            }
            {
                let mut tasks = self.tasks.lock().await;
                if !tasks.iter().any(|entry| entry.id == task.id) {
                    tasks.push_back(task.clone());
                }
            }
            updated = Some(task);
        }
        self.trusted_weles_tasks
            .write()
            .await
            .insert(task_id.to_string());
        if updated_live_task {
            self.persist_tasks().await;
        }
        updated
    }

    pub async fn cancel_task(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            if matches!(
                task.status,
                TaskStatus::Queued
                    | TaskStatus::InProgress
                    | TaskStatus::Blocked
                    | TaskStatus::FailedAnalyzing
                    | TaskStatus::AwaitingApproval
            ) {
                let thread_to_stop = task.thread_id.clone();
                let session_to_interrupt = task.session_id.clone();
                task.status = TaskStatus::Cancelled;
                task.completed_at = Some(now_millis());
                task.lane_id = None;
                task.blocked_reason = None;
                task.awaiting_approval_id = None;
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Warn,
                    "queue",
                    "task cancelled by user",
                    None,
                ));
                let updated = task.clone();
                drop(tasks);
                self.persist_tasks().await;
                if let Some(thread_id) = thread_to_stop {
                    let _ = self.stop_stream(&thread_id).await;
                }
                if let Some(session_id) =
                    session_to_interrupt.and_then(|value| Uuid::parse_str(&value).ok())
                {
                    let _ = self.session_manager.write_input(session_id, &[3]).await;
                }
                self.emit_task_update(&updated, Some("Cancelled by user".into()));
                self.settle_task_skill_consultations(&updated, "cancelled")
                    .await;
                self.record_collaboration_outcome(&updated, "cancelled")
                    .await;
                self.record_provenance_event(
                    "step_cancelled",
                    "task cancelled by operator",
                    serde_json::json!({
                        "task_id": updated.id,
                        "title": updated.title,
                        "source": updated.source,
                    }),
                    updated.goal_run_id.as_deref(),
                    Some(updated.id.as_str()),
                    updated.thread_id.as_deref(),
                    None,
                    None,
                )
                .await;
                return true;
            }
        }
        drop(tasks);

        let Some(mut task) = self
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
        else {
            return false;
        };
        if !matches!(
            task.status,
            TaskStatus::Queued
                | TaskStatus::InProgress
                | TaskStatus::Blocked
                | TaskStatus::FailedAnalyzing
                | TaskStatus::AwaitingApproval
        ) {
            return false;
        }

        let thread_to_stop = task.thread_id.clone();
        let session_to_interrupt = task.session_id.clone();
        task.status = TaskStatus::Cancelled;
        task.completed_at = Some(now_millis());
        task.lane_id = None;
        task.blocked_reason = None;
        task.awaiting_approval_id = None;
        task.logs.push(make_task_log_entry(
            task.retry_count,
            TaskLogLevel::Warn,
            "queue",
            "task cancelled by user",
            None,
        ));
        {
            let mut tasks = self.tasks.lock().await;
            if !tasks.iter().any(|entry| entry.id == task.id) {
                tasks.push_back(task.clone());
            }
        }
        self.persist_tasks().await;
        if let Some(thread_id) = thread_to_stop {
            let _ = self.stop_stream(&thread_id).await;
        }
        if let Some(session_id) =
            session_to_interrupt.and_then(|value| Uuid::parse_str(&value).ok())
        {
            let _ = self.session_manager.write_input(session_id, &[3]).await;
        }
        self.emit_task_update(&task, Some("Cancelled by user".into()));
        self.settle_task_skill_consultations(&task, "cancelled")
            .await;
        self.record_collaboration_outcome(&task, "cancelled").await;
        self.record_provenance_event(
            "step_cancelled",
            "task cancelled by operator",
            serde_json::json!({
                "task_id": task.id,
                "title": task.title,
                "source": task.source,
            }),
            task.goal_run_id.as_deref(),
            Some(task.id.as_str()),
            task.thread_id.as_deref(),
            None,
            None,
        )
        .await;
        true
    }

    pub(in crate::agent) async fn cancel_task_and_descendants(&self, task_id: &str) {
        let children = self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: None,
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
            .await;
        for child in children {
            Box::pin(self.cancel_task_and_descendants(&child.id)).await;
        }
        let _ = self.cancel_task(task_id).await;
    }

    pub async fn handle_task_approval_resolution(
        &self,
        approval_id: &str,
        decision: zorai_protocol::ApprovalDecision,
    ) -> bool {
        let goal_plan_approval_task = self
            .pending_approval_task_for_resolution(approval_id, Some("goal_plan_approval"))
            .await;
        if let Some(task) = goal_plan_approval_task {
            let review_task_id = task.id.clone();
            let goal_run_id = task.goal_run_id.clone();
            let thread_id = task.thread_id.clone();
            let updated = {
                let mut tasks = self.tasks.lock().await;
                let Some(task) = tasks.iter_mut().find(|entry| entry.id == review_task_id) else {
                    return false;
                };

                match decision {
                    zorai_protocol::ApprovalDecision::ApproveOnce
                    | zorai_protocol::ApprovalDecision::ApproveSession => {
                        task.status = TaskStatus::Completed;
                        task.progress = 100;
                        task.started_at = None;
                        task.completed_at = Some(now_millis());
                        task.awaiting_approval_id = None;
                        task.blocked_reason = None;
                        task.result =
                            Some("operator approved low-confidence goal plan".to_string());
                        task.error = None;
                        task.last_error = None;
                        task.logs.push(make_task_log_entry(
                            task.retry_count,
                            TaskLogLevel::Info,
                            "approval",
                            "operator approved low-confidence goal plan",
                            None,
                        ));
                    }
                    zorai_protocol::ApprovalDecision::Deny => {
                        let reason =
                            "operator denied low-confidence goal plan approval".to_string();
                        task.status = TaskStatus::Failed;
                        task.started_at = None;
                        task.completed_at = Some(now_millis());
                        task.awaiting_approval_id = None;
                        task.blocked_reason = Some(reason.clone());
                        task.error = Some(reason.clone());
                        task.last_error = Some(reason.clone());
                        task.logs.push(make_task_log_entry(
                            task.retry_count,
                            TaskLogLevel::Error,
                            "approval",
                            "operator denied low-confidence goal plan",
                            Some(reason),
                        ));
                    }
                }

                task.clone()
            };

            self.persist_tasks().await;
            self.emit_task_update(&updated, Some(status_message(&updated).into()));

            match decision {
                zorai_protocol::ApprovalDecision::ApproveOnce
                | zorai_protocol::ApprovalDecision::ApproveSession => {
                    if let Some(goal_run_id) = goal_run_id.as_deref() {
                        let maybe_goal = {
                            let mut goal_runs = self.goal_runs.lock().await;
                            goal_runs.iter_mut().find(|goal| goal.id == goal_run_id).map(|goal| {
                                goal.status = GoalRunStatus::Running;
                                goal.updated_at = now_millis();
                                goal.awaiting_approval_id = None;
                                goal.active_task_id = None;
                                goal.events.push(make_goal_run_event(
                                    "approval",
                                    "operator approved low-confidence goal plan; resuming execution",
                                    Some(approval_id.to_string()),
                                ));
                                goal.clone()
                            })
                        };
                        if let Some(goal) = maybe_goal {
                            self.persist_goal_runs().await;
                            self.emit_goal_run_update(&goal, Some("Goal plan approved".into()));
                        }
                    }
                }
                zorai_protocol::ApprovalDecision::Deny => {
                    if let Some(goal_run_id) = goal_run_id.as_deref() {
                        self.fail_goal_run(
                            goal_run_id,
                            "operator denied low-confidence goal plan approval",
                            "approval",
                            None,
                        )
                        .await;
                    }
                }
            }

            self.record_provenance_event(
                match decision {
                    zorai_protocol::ApprovalDecision::ApproveOnce
                    | zorai_protocol::ApprovalDecision::ApproveSession => "approval_granted",
                    zorai_protocol::ApprovalDecision::Deny => "approval_denied",
                },
                match decision {
                    zorai_protocol::ApprovalDecision::ApproveOnce
                    | zorai_protocol::ApprovalDecision::ApproveSession => {
                        "operator approved low-confidence goal plan"
                    }
                    zorai_protocol::ApprovalDecision::Deny => {
                        "operator denied low-confidence goal plan"
                    }
                },
                serde_json::json!({
                    "approval_id": approval_id,
                    "task_id": updated.id,
                    "title": updated.title,
                    "decision": format!("{decision:?}").to_lowercase(),
                    "source": updated.source,
                }),
                goal_run_id.as_deref(),
                Some(updated.id.as_str()),
                thread_id.as_deref(),
                Some(approval_id),
                None,
            )
            .await;

            if matches!(decision, zorai_protocol::ApprovalDecision::Deny) {
                let correction_desc = format!("Denied approval for task: {}", updated.title);
                let thread_id = thread_id.unwrap_or_default();
                self.update_counter_who_on_correction(&thread_id, &correction_desc)
                    .await;
            }

            return true;
        }

        let handoff_task = self
            .pending_approval_task_for_resolution(approval_id, Some("thread_handoff"))
            .await;
        if let Some(task) = handoff_task {
            let handoff_task_id = task.id.clone();
            let request = task.command.as_deref().and_then(|value| {
                serde_json::from_str::<PendingThreadHandoffActivation>(value).ok()
            });
            let Some(request) = request else {
                return false;
            };

            match decision {
                zorai_protocol::ApprovalDecision::ApproveOnce
                | zorai_protocol::ApprovalDecision::ApproveSession => {
                    let activation = self
                        .apply_thread_handoff_activation(&request, Some(approval_id.to_string()))
                        .await;
                    let updated = {
                        let mut tasks = self.tasks.lock().await;
                        let Some(task) = tasks.iter_mut().find(|task| task.id == handoff_task_id)
                        else {
                            return false;
                        };
                        task.awaiting_approval_id = None;
                        task.blocked_reason = None;
                        task.started_at = None;
                        task.completed_at = Some(now_millis());
                        match activation {
                            Ok(event) => {
                                task.status = TaskStatus::Completed;
                                task.progress = 100;
                                task.result = Some(format!(
                                    "handoff activated: {} -> {}",
                                    canonical_agent_name(&event.from_agent_id),
                                    canonical_agent_name(&event.to_agent_id)
                                ));
                                task.error = None;
                                task.last_error = None;
                                task.logs.push(make_task_log_entry(
                                    task.retry_count,
                                    TaskLogLevel::Info,
                                    "approval",
                                    "operator approved thread handoff; activation completed",
                                    None,
                                ));
                            }
                            Err(error) => {
                                let message = error.to_string();
                                task.status = TaskStatus::Failed;
                                task.error = Some(message.clone());
                                task.last_error = Some(message.clone());
                                task.blocked_reason = Some(message.clone());
                                task.logs.push(make_task_log_entry(
                                    task.retry_count,
                                    TaskLogLevel::Error,
                                    "approval",
                                    "operator approved thread handoff but activation failed",
                                    Some(message),
                                ));
                            }
                        }
                        task.clone()
                    };
                    self.persist_tasks().await;
                    self.emit_task_update(&updated, Some(status_message(&updated).into()));
                    self.record_provenance_event(
                        "approval_granted",
                        "operator approved thread handoff",
                        serde_json::json!({
                            "approval_id": approval_id,
                            "task_id": updated.id,
                            "title": updated.title,
                            "decision": format!("{decision:?}").to_lowercase(),
                            "source": updated.source,
                        }),
                        updated.goal_run_id.as_deref(),
                        Some(updated.id.as_str()),
                        updated.thread_id.as_deref(),
                        Some(approval_id),
                        None,
                    )
                    .await;
                    return true;
                }
                zorai_protocol::ApprovalDecision::Deny => {
                    self.clear_pending_thread_handoff_approval(&request.thread_id, approval_id)
                        .await;
                    let updated = {
                        let mut tasks = self.tasks.lock().await;
                        let Some(task) = tasks.iter_mut().find(|task| task.id == handoff_task_id)
                        else {
                            return false;
                        };
                        let reason = "operator denied thread handoff approval".to_string();
                        task.status = TaskStatus::Failed;
                        task.started_at = None;
                        task.completed_at = Some(now_millis());
                        task.awaiting_approval_id = None;
                        task.blocked_reason = Some(reason.clone());
                        task.error = Some(reason.clone());
                        task.last_error = Some(reason.clone());
                        task.logs.push(make_task_log_entry(
                            task.retry_count,
                            TaskLogLevel::Error,
                            "approval",
                            "operator denied thread handoff; task failed",
                            Some(reason),
                        ));
                        task.clone()
                    };
                    self.persist_tasks().await;
                    self.emit_task_update(&updated, Some(status_message(&updated).into()));
                    self.record_provenance_event(
                        "approval_denied",
                        "operator denied thread handoff",
                        serde_json::json!({
                            "approval_id": approval_id,
                            "task_id": updated.id,
                            "title": updated.title,
                            "decision": format!("{decision:?}").to_lowercase(),
                            "source": updated.source,
                        }),
                        updated.goal_run_id.as_deref(),
                        Some(updated.id.as_str()),
                        updated.thread_id.as_deref(),
                        Some(approval_id),
                        None,
                    )
                    .await;
                    let correction_desc = format!("Denied approval for task: {}", updated.title);
                    let thread_id = updated.thread_id.clone().unwrap_or_default();
                    self.update_counter_who_on_correction(&thread_id, &correction_desc)
                        .await;
                    return true;
                }
            }
        }

        if self
            .pending_approval_task_for_resolution(approval_id, None)
            .await
            .is_none()
        {
            return false;
        }

        let updated = {
            let mut tasks = self.tasks.lock().await;
            let Some(task) = tasks
                .iter_mut()
                .find(|task| task.awaiting_approval_id.as_deref() == Some(approval_id))
            else {
                return false;
            };

            match decision {
                zorai_protocol::ApprovalDecision::ApproveOnce
                | zorai_protocol::ApprovalDecision::ApproveSession => {
                    task.status = TaskStatus::Queued;
                    task.started_at = None;
                    task.awaiting_approval_id = None;
                    task.blocked_reason = None;
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "approval",
                        "operator approved managed command; task re-queued",
                        None,
                    ));
                }
                zorai_protocol::ApprovalDecision::Deny => {
                    let reason = "operator denied managed command approval".to_string();
                    task.status = TaskStatus::Failed;
                    task.started_at = None;
                    task.completed_at = Some(now_millis());
                    task.awaiting_approval_id = None;
                    task.blocked_reason = Some(reason.clone());
                    task.error = Some(reason.clone());
                    task.last_error = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Error,
                        "approval",
                        "operator denied managed command; task failed",
                        Some(reason),
                    ));
                }
            }

            task.clone()
        };

        self.persist_tasks().await;
        if matches!(
            decision,
            zorai_protocol::ApprovalDecision::ApproveOnce
                | zorai_protocol::ApprovalDecision::ApproveSession
        ) {
            if let Some(goal_run_id) = updated.goal_run_id.as_deref() {
                self.sync_goal_run_with_task(goal_run_id, &updated).await;
            }
        }
        if matches!(
            decision,
            zorai_protocol::ApprovalDecision::ApproveOnce
                | zorai_protocol::ApprovalDecision::ApproveSession
        ) {
            if matches!(decision, zorai_protocol::ApprovalDecision::ApproveSession)
                && is_policy_escalation_approval(approval_id)
            {
                if let Some(thread_id) = updated.thread_id.as_deref() {
                    self.store_policy_escalation_session_grant(thread_id).await;
                }
            }
            if let Some(thread_id) = updated.thread_id.as_deref() {
                let _ = self
                    .record_thread_skill_approval_resolution(thread_id, approval_id)
                    .await;
            }
        } else if let Some(thread_id) = updated.thread_id.as_deref() {
            let _ = self
                .record_thread_skill_approval_denial(thread_id, approval_id)
                .await;
        }
        self.emit_task_update(&updated, Some(status_message(&updated).into()));
        self.record_provenance_event(
            match decision {
                zorai_protocol::ApprovalDecision::ApproveOnce
                | zorai_protocol::ApprovalDecision::ApproveSession => "approval_granted",
                zorai_protocol::ApprovalDecision::Deny => "approval_denied",
            },
            match decision {
                zorai_protocol::ApprovalDecision::ApproveOnce
                | zorai_protocol::ApprovalDecision::ApproveSession => {
                    "operator approved managed command"
                }
                zorai_protocol::ApprovalDecision::Deny => "operator denied managed command",
            },
            serde_json::json!({
                "approval_id": approval_id,
                "task_id": updated.id,
                "title": updated.title,
                "decision": format!("{decision:?}").to_lowercase(),
            }),
            updated.goal_run_id.as_deref(),
            Some(updated.id.as_str()),
            updated.thread_id.as_deref(),
            Some(approval_id),
            None,
        )
        .await;

        if matches!(decision, zorai_protocol::ApprovalDecision::Deny) {
            let correction_desc = format!("Denied approval for task: {}", updated.title);
            let thread_id = updated.thread_id.clone().unwrap_or_default();
            self.update_counter_who_on_correction(&thread_id, &correction_desc)
                .await;
        }

        true
    }

    pub(super) async fn snapshot_tasks(&self) -> Vec<AgentTask> {
        let sessions = self.session_manager.list().await;
        let config = self.config.read().await.clone();
        let mut tasks = self.tasks.lock().await;
        let changed = refresh_task_queue_state(&mut tasks, now_millis(), &sessions, &config);
        let snapshot = tasks
            .iter()
            .cloned()
            .map(|mut task| {
                crate::agent::persistence::sanitize_task_for_external_view(&mut task);
                task
            })
            .collect();
        drop(tasks);

        if !changed.is_empty() {
            self.persist_tasks().await;
            for task in changed {
                self.emit_task_update(&task, Some(status_message(&task).into()));
            }
        }

        snapshot
    }

    pub async fn list_tasks(&self) -> Vec<AgentTask> {
        self.list_tasks_filtered(&crate::history::AgentTaskListQuery::default())
            .await
    }

    pub(super) async fn refresh_task_queue_state_for_filtered_list(&self) {
        let sessions = self.session_manager.list().await;
        let config = self.config.read().await.clone();
        let changed = {
            let mut tasks = self.tasks.lock().await;
            refresh_task_queue_state(&mut tasks, now_millis(), &sessions, &config)
        };

        if !changed.is_empty() {
            self.persist_tasks().await;
            for task in changed {
                self.emit_task_update(&task, Some(status_message(&task).into()));
            }
        }
    }

    pub(crate) async fn list_tasks_filtered(
        &self,
        query: &crate::history::AgentTaskListQuery,
    ) -> Vec<AgentTask> {
        self.refresh_task_queue_state_for_filtered_list().await;
        match self.history.list_agent_tasks_filtered(query).await {
            Ok(mut tasks) => {
                for task in &mut tasks {
                    crate::agent::persistence::sanitize_task_for_external_view(task);
                }
                let in_memory = filter_task_snapshot_for_query(self.snapshot_tasks().await, query);
                if !in_memory.is_empty() {
                    let known_ids: std::collections::HashSet<String> =
                        tasks.iter().map(|task| task.id.clone()).collect();
                    for task in in_memory {
                        if !known_ids.contains(&task.id) {
                            tasks.push(task);
                        }
                    }
                    if query.order_by_recent_activity_desc {
                        tasks.sort_by_key(|task| std::cmp::Reverse(task_recent_activity_at(task)));
                    }
                    if let Some(limit) = query.limit {
                        tasks.truncate(limit.max(1));
                    }
                }
                tasks
            }
            Err(error) => {
                tracing::warn!("failed to list filtered persisted tasks: {error}");
                let snapshot = self.snapshot_tasks().await;
                filter_task_snapshot_for_query(snapshot, query)
            }
        }
    }

    pub(crate) async fn list_parent_thread_subagent_tasks(
        &self,
        parent_thread_id: &str,
        status: Option<&str>,
    ) -> Vec<AgentTask> {
        self.refresh_task_queue_state_for_filtered_list().await;
        match self
            .history
            .list_agent_tasks_for_parent_thread_subagents(parent_thread_id, status, None)
            .await
        {
            Ok(mut tasks) => {
                for task in &mut tasks {
                    crate::agent::persistence::sanitize_task_for_external_view(task);
                }
                tasks
            }
            Err(error) => {
                tracing::warn!("failed to list parent-thread persisted subagent tasks: {error}");
                self.snapshot_tasks()
                    .await
                    .into_iter()
                    .filter(|task| matches_parent_thread_subagent(task, parent_thread_id, status))
                    .collect()
            }
        }
    }

    pub(crate) async fn count_tasks_filtered(
        &self,
        query: &crate::history::AgentTaskListQuery,
    ) -> usize {
        self.refresh_task_queue_state_for_filtered_list().await;
        match self.history.count_agent_tasks_filtered(query).await {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!("failed to count filtered persisted tasks: {error}");
                let snapshot = self.snapshot_tasks().await;
                filter_task_snapshot_for_query(snapshot, query).len()
            }
        }
    }

    pub(crate) async fn list_task_quiet_recovery_refs_filtered(
        &self,
        query: &crate::history::AgentTaskListQuery,
    ) -> Vec<crate::history::AgentTaskQuietRecoveryRef> {
        self.refresh_task_queue_state_for_filtered_list().await;
        match self
            .history
            .list_agent_task_quiet_recovery_refs_filtered(query)
            .await
        {
            Ok(refs) => refs,
            Err(error) => {
                tracing::warn!("failed to list quiet-goal recovery task refs: {error}");
                let snapshot = self.snapshot_tasks().await;
                filter_task_snapshot_for_query(snapshot, query)
                    .iter()
                    .map(crate::history::AgentTaskQuietRecoveryRef::from)
                    .collect()
            }
        }
    }

    pub(crate) async fn list_task_quiet_recovery_refs_for_goal_runs_statuses(
        &self,
        goal_run_ids: &[String],
        statuses: &[String],
    ) -> Vec<crate::history::AgentTaskQuietRecoveryRef> {
        let goal_run_ids = goal_run_ids
            .iter()
            .map(|goal_run_id| goal_run_id.trim())
            .filter(|goal_run_id| !goal_run_id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<std::collections::HashSet<_>>();
        let statuses = statuses
            .iter()
            .map(|status| status.trim())
            .filter(|status| !status.is_empty())
            .map(ToOwned::to_owned)
            .collect::<std::collections::HashSet<_>>();
        if goal_run_ids.is_empty() || statuses.is_empty() {
            return Vec::new();
        }

        self.refresh_task_queue_state_for_filtered_list().await;
        let persisted_goal_run_ids = goal_run_ids.iter().cloned().collect::<Vec<_>>();
        let persisted_statuses = statuses.iter().cloned().collect::<Vec<_>>();
        let mut refs = match self
            .history
            .list_agent_task_quiet_recovery_refs_for_goal_runs_statuses(
                &persisted_goal_run_ids,
                &persisted_statuses,
            )
            .await
        {
            Ok(refs) => refs,
            Err(error) => {
                tracing::warn!("failed to list quiet-goal recovery task refs by goal run: {error}");
                Vec::new()
            }
        };
        let known_ids = refs
            .iter()
            .map(|task| task.id.clone())
            .collect::<std::collections::HashSet<_>>();
        let live_refs = self
            .snapshot_tasks()
            .await
            .iter()
            .filter(|task| {
                task.goal_run_id
                    .as_deref()
                    .is_some_and(|goal_run_id| goal_run_ids.contains(goal_run_id))
                    && serde_json::to_value(task.status)
                        .ok()
                        .and_then(|value| value.as_str().map(ToOwned::to_owned))
                        .is_some_and(|status| statuses.contains(&status))
                    && !known_ids.contains(&task.id)
            })
            .map(crate::history::AgentTaskQuietRecoveryRef::from)
            .collect::<Vec<_>>();
        refs.extend(live_refs);
        refs.sort_by(|a, b| a.id.cmp(&b.id));
        refs
    }

    pub async fn list_runs_for_parent_thread(&self, parent_thread_id: &str) -> Vec<AgentRun> {
        let tasks = self
            .list_parent_thread_subagent_tasks(parent_thread_id, None)
            .await;
        self.project_runs(&tasks).await
    }

    pub async fn list_runs(&self) -> Vec<AgentRun> {
        let tasks = self
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
                exclude_terminal_statuses: false,
                order_by_recent_activity_desc: true,
                limit: Some(200),
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await;
        self.project_runs(&tasks).await
    }

    pub async fn get_run(&self, run_id: &str) -> Option<AgentRun> {
        let mut tasks = self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some(run_id.to_string()),
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
            .await;
        let parent_task_id = tasks.first().and_then(|task| task.parent_task_id.clone());
        if let Some(parent_task_id) = parent_task_id {
            tasks.extend(
                self.list_tasks_filtered(&crate::history::AgentTaskListQuery {
                    id: Some(parent_task_id),
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
                .await,
            );
        }
        self.project_runs(&tasks)
            .await
            .into_iter()
            .find(|run| run.id == run_id)
    }

    async fn project_runs(&self, tasks: &[AgentTask]) -> Vec<AgentRun> {
        let sessions = self.session_manager.list().await;
        let profiles = self.thread_execution_profiles.read().await.clone();
        let sub_agents = self.config.read().await.sub_agents.clone();
        let mut runs = project_task_runs_with_runtime(tasks, &sessions, &profiles, &sub_agents);
        runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        runs
    }
}
