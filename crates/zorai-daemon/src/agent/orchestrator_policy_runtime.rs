use anyhow::Result;

use crate::agent::metacognitive::{escalation, replanning};
use crate::agent::{
    generate_message_id, make_task_log_entry, now_millis, AgentEngine, AgentEvent, AgentMessage,
    AgentTask, MessageRole, TaskLogLevel, TaskStatus,
};

use super::*;

impl AgentEngine {
    async fn task_by_id_for_orchestrator_policy(
        &self,
        task_id: &str,
    ) -> Option<crate::agent::types::AgentTask> {
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
}

fn build_strategy_refresh_prompt(
    trigger: &PolicyTriggerContext,
    step_title: &str,
    task_retry_count: u32,
    tool_success_rate: f64,
    strategy_hint: Option<&str>,
) -> String {
    let decision = replanning::select_replan_strategy(&replanning::ReplanContext {
        current_step_index: 0,
        step_title: step_title.to_string(),
        stuck_reason: infer_stuck_reason(trigger),
        attempt_count: task_retry_count,
        error_rate: if tool_success_rate >= 1.0 {
            0.0
        } else {
            1.0 - tool_success_rate
        },
        tool_success_rate,
        context_utilization_pct: 0,
        has_checkpoint: false,
        recent_tool_names: Vec::new(),
    });
    let mut prompt = replanning::build_replan_prompt(&decision, step_title);
    if let Some(strategy_hint) = strategy_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("\n\nPolicy hint: ");
        prompt.push_str(strategy_hint);
    }
    prompt
}

fn mark_task_policy_halted_retry_failure(task: &mut AgentTask, detail: Option<String>) {
    task.retry_count = task.max_retries;
    task.status = TaskStatus::Failed;
    task.completed_at = Some(now_millis());
    task.blocked_reason = Some("policy halted repeated retry".to_string());
    task.last_error = Some("policy halted repeated retry".to_string());
    task.logs.push(make_task_log_entry(
        task.retry_count,
        TaskLogLevel::Warn,
        "policy",
        "policy halted repeated retry",
        detail,
    ));
}

fn retry_guard_matches_runtime_context(
    decision: &PolicyDecision,
    context: &PolicyEvaluationContext,
) -> bool {
    decision
        .retry_guard
        .as_deref()
        .zip(context.current_retry_guard.as_deref())
        .is_some_and(|(decision_guard, current_guard)| decision_guard == current_guard)
}

impl AgentEngine {
    async fn mark_policy_halted_retry_failure(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        detail: Option<String>,
    ) {
        if let Some(task_id) = task_id {
            let live_updated = {
                let mut tasks = self.tasks.lock().await;
                tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .map(|task| {
                        mark_task_policy_halted_retry_failure(task, detail.clone());
                        task.clone()
                    })
            };
            let updated = if let Some(updated) = live_updated {
                self.persist_tasks().await;
                updated
            } else {
                let Some(mut task) = self.task_by_id_for_orchestrator_policy(task_id).await else {
                    return;
                };
                mark_task_policy_halted_retry_failure(&mut task, detail);
                if let Err(error) = self.history.upsert_agent_task(&task).await {
                    tracing::warn!(
                        task_id = %task.id,
                        %error,
                        "failed to persist policy-halted task"
                    );
                    return;
                }
                task
            };
            self.emit_task_update(&updated, Some("Policy halted repeated retry".into()));
            if let Some(goal_run_id) = updated.goal_run_id.as_deref() {
                self.sync_goal_run_with_task(goal_run_id, &updated).await;
            }
        }
        self.append_system_message(
            thread_id,
            "Policy halted a repeated retry for the same failing approach.".to_string(),
        )
        .await;
    }

    async fn append_system_message(&self, thread_id: &str, content: String) {
        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(thread_id) {
                thread.messages.push(AgentMessage {
                    reasoning_tokens: None,
                    thinking_ms: None,
                    id: generate_message_id(),
                    role: MessageRole::System,
                    content,
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
                    message_kind: crate::agent::types::AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    tool_output_preview_path: None,
                    structural_refs: Vec::new(),
                    pinned_for_compaction: false,
                    timestamp: now_millis(),
                    feedback: None,
                });
                thread.updated_at = now_millis();
            }
        }
        self.persist_thread_by_id(thread_id).await;
    }

    async fn goal_run_current_step_title_for_orchestrator_policy(
        &self,
        goal_run_id: &str,
    ) -> Option<String> {
        let live_title = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .find(|goal_run| goal_run.id == goal_run_id)
                .and_then(|goal_run| {
                    goal_run.current_step_title.clone().or_else(|| {
                        goal_run
                            .steps
                            .get(goal_run.current_step_index)
                            .map(|step| step.title.clone())
                    })
                })
        };
        if live_title.is_some() {
            return live_title;
        }

        match self.history.goal_run_current_step_title(goal_run_id).await {
            Ok(title) => title,
            Err(error) => {
                tracing::warn!(
                    goal_run_id,
                    %error,
                    "failed to query goal step title for orchestrator policy"
                );
                self.get_goal_run(goal_run_id).await.and_then(|goal_run| {
                    goal_run.current_step_title.or_else(|| {
                        goal_run
                            .steps
                            .get(goal_run.current_step_index)
                            .map(|step| step.title.clone())
                    })
                })
            }
        }
    }

    async fn apply_policy_pivot(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        trigger: &PolicyTriggerContext,
        decision: &PolicyDecision,
    ) -> Result<PolicyLoopAction> {
        let step_title = if let Some(goal_run_id) = goal_run_id {
            self.goal_run_current_step_title_for_orchestrator_policy(goal_run_id)
                .await
                .unwrap_or_else(|| "current step".to_string())
        } else {
            "current step".to_string()
        };
        let task_retry_count = if let Some(task_id) = task_id {
            self.task_by_id_for_orchestrator_policy(task_id)
                .await
                .map(|task| task.retry_count)
                .unwrap_or(0)
        } else {
            0
        };
        let tool_success_rate = {
            let monitor = self.awareness.read().await;
            monitor
                .get_window(thread_id)
                .map(|window| window.short_term_success_rate)
                .unwrap_or(0.8)
        };
        let prompt = build_strategy_refresh_prompt(
            trigger,
            &step_title,
            task_retry_count,
            tool_success_rate,
            decision.strategy_hint.as_deref(),
        );
        self.append_system_message(thread_id, prompt).await;
        self.emit_workflow_notice(
            thread_id,
            "strategy-refresh",
            "Policy pivot injected a strategy refresh before retrying.",
            decision.strategy_hint.clone(),
        );
        Ok(PolicyLoopAction::RestartLoop)
    }

    async fn apply_policy_escalation(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        decision: &PolicyDecision,
        now_epoch_secs: u64,
    ) -> Result<PolicyLoopAction> {
        if self
            .mark_task_approval_rule_used("orchestrator_policy_escalation")
            .await
        {
            self.emit_workflow_notice(
                thread_id,
                "policy-escalation-always-approved",
                "Applied a saved always-approve rule for policy escalation and continued automatically.",
                Some(decision.reason.clone()),
            );
            return Ok(PolicyLoopAction::Continue);
        }

        if self.has_policy_escalation_session_grant(thread_id).await {
            self.emit_workflow_notice(
                thread_id,
                "policy-escalation-session-grant",
                "Reused session approval for policy escalation and continued automatically.",
                Some(decision.reason.clone()),
            );
            return Ok(PolicyLoopAction::Continue);
        }

        let task_id = match task_id {
            Some(task_id) => task_id,
            None => return Ok(PolicyLoopAction::Continue),
        };
        let attempts = {
            self.task_by_id_for_orchestrator_policy(task_id)
                .await
                .map(|task| task.retry_count)
                .unwrap_or(0)
        };
        let from_level = escalation::EscalationLevel::SelfCorrection;
        let to_level = escalation::EscalationLevel::User;
        let audit = escalation::escalation_audit_data(
            &from_level,
            &to_level,
            &decision.reason,
            attempts,
            Some(thread_id),
            &[],
            now_epoch_secs.saturating_mul(1000),
        );
        let pending_approval = crate::agent::types::ToolPendingApproval {
            approval_id: format!("policy-escalation-{thread_id}-{now_epoch_secs}"),
            execution_id: format!("policy-escalation-exec-{thread_id}-{now_epoch_secs}"),
            command: "orchestrator_policy_escalation".to_string(),
            rationale: decision.reason.clone(),
            risk_level: "medium".to_string(),
            blast_radius: "thread".to_string(),
            reasons: vec![decision.reason.clone()],
            session_id: None,
        };
        self.mark_task_awaiting_approval(task_id, thread_id, &pending_approval)
            .await;
        let task = self.task_by_id_for_orchestrator_policy(task_id).await;
        if let Some(task) = task.as_ref() {
            if let Some(goal_run_id) = task.goal_run_id.as_deref() {
                self.sync_goal_run_with_task(goal_run_id, task).await;
            }
        }
        self.append_system_message(
            thread_id,
            format!(
                "Policy escalation requested operator guidance: {}",
                decision.reason,
            ),
        )
        .await;
        let _ = self.event_tx.send(AgentEvent::AuditAction {
            id: audit.audit_id.clone(),
            timestamp: audit.timestamp,
            action_type: "policy_escalation".to_string(),
            summary: audit.summary.clone(),
            explanation: Some(audit.reason.clone()),
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: Some(thread_id.to_string()),
        });
        // When an escalation transitions into the external (L3) tier, emit a
        // critical inbox notification so the operator sees it through the
        // notifications surface and not only via the escalation event. Today
        // this handler hardcodes `to_level = User`, so the branch is
        // dormant; the wiring exists so any future producer that sets
        // `to_level = External` (e.g. an L2-response-timeout watcher) gets the
        // notification dispatch automatically.
        let dispatch_external =
            audit.to_label == escalation::EscalationLevel::External.as_label().to_string();
        let _ = self.event_tx.send(AgentEvent::EscalationUpdate {
            thread_id: thread_id.to_string(),
            from_level: audit.from_label.clone(),
            to_level: audit.to_label.clone(),
            reason: audit.reason.clone(),
            attempts: audit.attempts,
            audit_id: Some(audit.audit_id.clone()),
        });
        if dispatch_external {
            if let Err(error) = self
                .dispatch_external_escalation_notification(thread_id, &audit)
                .await
            {
                tracing::warn!(
                    thread_id,
                    error = %error,
                    "failed to dispatch external escalation notification",
                );
            }
        }
        Ok(PolicyLoopAction::InterruptForApproval)
    }

    pub(crate) async fn apply_orchestrator_policy_decision(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        trigger: &PolicyTriggerContext,
        decision: &PolicyDecision,
        decision_source: PolicyDecisionSource,
        now_epoch_secs: u64,
    ) -> Result<PolicyLoopAction> {
        if decision_source == PolicyDecisionSource::ReusedRecent
            && decision.action != PolicyAction::Continue
        {
            return Ok(policy_loop_action_for_decision(decision));
        }

        match decision.action {
            PolicyAction::Continue => Ok(PolicyLoopAction::Continue),
            PolicyAction::Pivot => {
                self.apply_policy_pivot(thread_id, task_id, goal_run_id, trigger, decision)
                    .await
            }
            PolicyAction::Escalate => {
                self.apply_policy_escalation(thread_id, task_id, decision, now_epoch_secs)
                    .await
            }
            PolicyAction::HaltRetries => {
                let detail = decision.retry_guard.as_ref().map(|retry_guard| {
                    format!("approach_hash={retry_guard}; reason={}", decision.reason)
                });
                self.mark_policy_halted_retry_failure(thread_id, task_id, detail)
                    .await;
                Ok(PolicyLoopAction::AbortRetry)
            }
        }
    }

    pub(crate) async fn enforce_orchestrator_retry_guard(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        scope: &PolicyDecisionScope,
        approach_hash: &str,
        now_epoch_secs: u64,
    ) -> Result<PolicyLoopAction> {
        if !self
            .is_retry_guard_active(scope, approach_hash, now_epoch_secs)
            .await
        {
            return Ok(PolicyLoopAction::Continue);
        }
        self.mark_policy_halted_retry_failure(
            thread_id,
            task_id,
            Some(format!("approach_hash={approach_hash}")),
        )
        .await;
        Ok(PolicyLoopAction::AbortRetry)
    }

    pub(crate) async fn evaluate_orchestrator_policy_turn(
        &self,
        scope: &PolicyDecisionScope,
        context: PolicyEvaluationContext,
        now_epoch_secs: u64,
    ) -> Result<SelectedPolicyDecision> {
        if scope.goal_run_id.is_some() {
            return Ok(SelectedPolicyDecision {
                source: PolicyDecisionSource::ReusedRecent,
                decision: continue_policy_decision(
                    "Goal worker uses owner supervisor review instead of orchestrator policy.",
                ),
            });
        }
        let recent = self.latest_policy_decision(scope, now_epoch_secs).await;
        if let Some(recent) = recent.as_ref() {
            if recent.decision.action != PolicyAction::Continue
                && recent.decision.retry_guard.is_some()
                && retry_guard_matches_runtime_context(&recent.decision, &context)
            {
                return Ok(SelectedPolicyDecision {
                    source: PolicyDecisionSource::ReusedRecent,
                    decision: recent.decision.clone(),
                });
            }
        }
        let mut context = context;
        context.recent_decision_summary = recent.as_ref().map(summarize_recent_policy_decision);
        let prompt = build_policy_eval_prompt(&context);
        let evaluated = normalize_policy_eval_decision(Some(runtime_owns_policy_retry_guard(
            self.request_orchestrator_policy_decision(&prompt)
                .await?
                .unwrap_or_else(|| {
                    continue_policy_decision(
                        "Policy evaluation unavailable; continuing current execution.",
                    )
                }),
            context.current_retry_guard.as_deref(),
        )));
        let selection = if evaluated.retry_guard.is_none()
            || retry_guard_matches_runtime_context(&evaluated, &context)
        {
            select_orchestrator_policy_decision(
                recent.as_ref(),
                &context.trigger,
                evaluated.clone(),
            )
        } else {
            SelectedPolicyDecision {
                source: PolicyDecisionSource::FreshEvaluation,
                decision: evaluated.clone(),
            }
        };
        if selection.source == PolicyDecisionSource::ReusedRecent {
            return Ok(selection);
        }
        self.record_policy_decision(scope, evaluated.clone(), now_epoch_secs)
            .await;
        Ok(SelectedPolicyDecision {
            source: PolicyDecisionSource::FreshEvaluation,
            decision: evaluated,
        })
    }
}

fn policy_loop_action_for_decision(decision: &PolicyDecision) -> PolicyLoopAction {
    match decision.action {
        PolicyAction::Continue => PolicyLoopAction::Continue,
        PolicyAction::Pivot => PolicyLoopAction::RestartLoop,
        PolicyAction::Escalate => PolicyLoopAction::InterruptForApproval,
        PolicyAction::HaltRetries => PolicyLoopAction::AbortRetry,
    }
}
