use super::*;
#[path = "budget_extension.rs"]
mod budget_extension;
use budget_extension::{
    budget_extension_applies_to_runner, finish_subagent_synthesis,
    max_loops_after_successful_budget_extend, max_loops_for_budget_report_back,
    parse_successful_budget_extension, should_exit_report_back_after_live_ceiling_sync,
    should_mark_terminated_after_failed_report_back_entry, FinishSubagentSynthesis,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReportBackReason {
    ExecutionBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BudgetOverflowDecision {
    Continue,
    RelyOnCompaction,
    EnterReportBack,
    Stop,
}

pub(super) fn budget_overflow_decision(
    is_spawned_subagent: bool,
    already_in_report_back: bool,
    overflow_action: ContextOverflowAction,
) -> BudgetOverflowDecision {
    if already_in_report_back {
        return BudgetOverflowDecision::Continue;
    }
    match overflow_action {
        ContextOverflowAction::Error if is_spawned_subagent => {
            BudgetOverflowDecision::EnterReportBack
        }
        ContextOverflowAction::Error => BudgetOverflowDecision::Stop,
        _ => BudgetOverflowDecision::RelyOnCompaction,
    }
}

pub(super) fn should_force_budget_report_back(
    already_in_report_back: bool,
    already_has_terminal_report: bool,
    spawned_subagent: bool,
) -> bool {
    spawned_subagent && !already_in_report_back && !already_has_terminal_report
}

pub(super) fn should_emit_turn_done_on_text_completion(continue_for_report_back: bool) -> bool {
    !continue_for_report_back
}

pub(super) fn should_emit_deferred_turn_done(
    needs_turn_done: bool,
    interrupted_for_approval: bool,
) -> bool {
    needs_turn_done && !interrupted_for_approval
}

pub(super) fn terminal_subagent_report_requires_turn_done(turn_done_already_emitted: bool) -> bool {
    !turn_done_already_emitted
}

pub(super) fn should_emit_tool_execution_limit_error(
    was_cancelled: bool,
    max_loops: u32,
    loop_count: u32,
    has_terminal_subagent_report: bool,
) -> bool {
    !was_cancelled && max_loops > 0 && loop_count >= max_loops && !has_terminal_subagent_report
}

pub(super) fn parse_subagent_report_status(status: &str) -> Option<SubagentReportStatus> {
    match status.trim() {
        "done" => Some(SubagentReportStatus::Done),
        "cancelled" | "canceled" => Some(SubagentReportStatus::Cancelled),
        "error" => Some(SubagentReportStatus::Error),
        _ => None,
    }
}

impl<'a> SendMessageRunner<'a> {
    pub(super) fn spawned_subagent_turn_active(&self) -> bool {
        self.current_task_snapshot
            .as_ref()
            .is_some_and(crate::agent::types::AgentTask::is_spawned_subagent)
    }

    pub(super) fn emit_turn_done(
        &mut self,
        input_tokens: u64,
        output_tokens: u64,
        cost: Option<f64>,
        tps: Option<f64>,
        generation_ms: Option<u64>,
        reasoning: Option<String>,
        upstream_message: Option<CompletionUpstreamMessage>,
        provider_final_result: Option<CompletionProviderFinalResult>,
        message_id: Option<String>,
    ) {
        self.needs_turn_done = false;
        self.turn_done_emitted = true;
        let _ = self.engine.event_tx.send(AgentEvent::Done {
            thread_id: self.tid.clone(),
            input_tokens,
            output_tokens,
            cost,
            provider: Some(self.config.provider.clone()),
            model: Some(self.provider_config.model.clone()),
            tps,
            generation_ms,
            reasoning,
            upstream_message,
            provider_final_result,
            message_id,
        });
    }

    pub(super) async fn enter_execution_budget_report_back(&mut self) -> bool {
        if !should_force_budget_report_back(
            self.report_back_phase.is_some(),
            self.subagent_report.is_some(),
            self.spawned_subagent_turn_active(),
        ) {
            return false;
        }
        let consumed = self
            .task_context_budget
            .as_ref()
            .map(|budget| budget.consumed())
            .unwrap_or(0);
        let max = self
            .task_context_budget
            .as_ref()
            .map(|budget| budget.max_tokens())
            .unwrap_or(0);
        let prompt = format!(
            "Execution budget exceeded ({consumed}/{max} visible output tokens). Reporting back is not counted against this budget.\n\nYou must call exactly one of:\n- `extend_subagent_budget` with additional_tokens and reason, which actually raises this thread's budget so you can continue.\n- `report_subagent_outcome` with status `done`, `cancelled`, or `error` and a concrete summary of what was completed, what remains, and any artifacts.\nDo not continue other tools until one of those two calls succeeds."
        );
        if !self
            .engine
            .append_system_thread_message(&self.tid, prompt)
            .await
        {
            return false;
        }
        self.engine.emit_workflow_notice(
            &self.tid,
            "budget-report-back",
            "Output token budget exceeded; reporting back is budget-exempt.",
            None,
        );
        self.report_back_phase = Some(ReportBackReason::ExecutionBudget);
        self.terminated_for_budget = false;
        self.restrict_tools_to_report_back();
        self.max_loops = max_loops_for_budget_report_back(self.max_loops, self.loop_count);
        true
    }

    fn restrict_tools_to_report_back(&mut self) {
        if let Some(filter) = self.task_tool_filter.as_mut() {
            filter.allow_tools([
                zorai_protocol::tool_names::REPORT_SUBAGENT_OUTCOME,
                zorai_protocol::tool_names::EXTEND_SUBAGENT_BUDGET,
            ]);
        }
        if self.tools_before_report_back.is_none() {
            self.tools_before_report_back = Some(self.tools.clone());
        }
        let is_report_back_tool = |name: &str| {
            matches!(
                name,
                zorai_protocol::tool_names::REPORT_SUBAGENT_OUTCOME
                    | zorai_protocol::tool_names::EXTEND_SUBAGENT_BUDGET
            )
        };
        self.tools
            .retain(|tool| is_report_back_tool(&tool.function.name));
        let mut extra: Vec<_> = self
            .tools_before_report_back
            .as_ref()
            .into_iter()
            .flatten()
            .chain(self.deferred_tool_pool.iter())
            .filter(|tool| is_report_back_tool(&tool.function.name))
            .cloned()
            .collect();
        if extra.len() < 2 {
            extra.extend(
                crate::agent::tool_executor::get_available_tools(
                    &self.config,
                    &self.engine.data_dir,
                    false,
                )
                .into_iter()
                .filter(|tool| is_report_back_tool(&tool.function.name)),
            );
        }
        for tool in extra {
            if !self
                .tools
                .iter()
                .any(|existing| existing.function.name == tool.function.name)
            {
                self.tools.push(tool);
            }
        }
    }

    pub(super) async fn maybe_force_budget_report_back(&mut self) -> bool {
        self.sync_task_context_budget_from_live_task().await;
        if !should_force_budget_report_back(
            self.report_back_phase.is_some(),
            self.subagent_report.is_some(),
            self.spawned_subagent_turn_active(),
        ) {
            return false;
        }
        let Some(budget) = self.task_context_budget.as_mut() else {
            return false;
        };
        let current_tokens = {
            let threads = self.engine.threads.read().await;
            threads
                .get(&self.tid)
                .map(|thread| {
                    crate::agent::subagent::context_budget::visible_output_budget_tokens(
                        &thread.messages,
                    )
                })
                .unwrap_or(0)
        };
        budget.set_consumed(current_tokens);
        match budget.check() {
            crate::agent::subagent::context_budget::BudgetStatus::Exceeded {
                overflow_action: ContextOverflowAction::Error,
                ..
            } => {
                let entered = self.enter_execution_budget_report_back().await;
                if should_mark_terminated_after_failed_report_back_entry(entered) {
                    self.terminated_for_budget = true;
                }
                entered
            }
            _ => false,
        }
    }

    pub(super) fn exit_report_back_phase(&mut self) {
        self.report_back_phase = None;
        if let Some(tools) = self.tools_before_report_back.take() {
            self.tools = tools;
        }
    }

    pub(super) async fn apply_finish_subagent_synthesis(&mut self) {
        match finish_subagent_synthesis(
            self.was_cancelled,
            self.report_back_phase.is_some(),
            self.subagent_report.is_some(),
            self.terminated_for_budget,
            self.spawned_subagent_turn_active(),
        ) {
            FinishSubagentSynthesis::CancelledReport => {
                let summary = self.synthesize_thread_summary().await;
                self.subagent_report = Some(SubagentTurnReport {
                    status: SubagentReportStatus::Cancelled,
                    summary,
                    reason: Some("cancelled".to_string()),
                });
                self.terminated_for_budget = false;
                self.exit_report_back_phase();
            }
            FinishSubagentSynthesis::BudgetTextReport => {
                let summary = self.synthesize_thread_summary().await;
                self.capture_text_report_back(&summary).await;
            }
            FinishSubagentSynthesis::BudgetErrorReport => {
                let summary = self.synthesize_thread_summary().await;
                self.subagent_report = Some(SubagentTurnReport {
                    status: SubagentReportStatus::Error,
                    summary,
                    reason: Some("zorai_budget".to_string()),
                });
            }
            FinishSubagentSynthesis::None => {}
        }
    }

    fn grant_continuation_loops_after_budget_extend(&mut self) {
        self.max_loops = max_loops_after_successful_budget_extend(
            self.max_loops,
            self.loop_count,
            self.config.max_tool_loops,
        );
    }

    pub(super) fn apply_successful_subagent_budget_tool(
        &mut self,
        tool_name: &str,
        arguments: &str,
        result_content: &str,
    ) -> Option<ToolCallDisposition> {
        if tool_name == zorai_protocol::tool_names::REPORT_SUBAGENT_OUTCOME {
            let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
            let status = args
                .get("status")
                .and_then(|value| value.as_str())
                .and_then(parse_subagent_report_status)
                .unwrap_or(SubagentReportStatus::Error);
            let summary = args
                .get("summary")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("subagent reported without a summary")
                .to_string();
            self.subagent_report = Some(SubagentTurnReport {
                status,
                summary,
                reason: self.report_back_phase.map(|_| "zorai_budget".to_string()),
            });
            self.terminated_for_budget = false;
            self.exit_report_back_phase();
            if terminal_subagent_report_requires_turn_done(self.turn_done_emitted) {
                self.needs_turn_done = true;
            }
            return Some(ToolCallDisposition::BreakLoop);
        }
        if tool_name == zorai_protocol::tool_names::EXTEND_SUBAGENT_BUDGET {
            let extension = parse_successful_budget_extension(arguments, result_content);
            if !budget_extension_applies_to_runner(
                self.task_id,
                &self.tid,
                extension.target_task_id.as_deref(),
                extension.target_thread_id.as_deref(),
            ) {
                return None;
            }
            if let Some(budget) = self.task_context_budget.as_mut() {
                if let Some(new_max) = extension.new_max_tokens {
                    budget.raise_max_to(new_max);
                } else if extension.additional_tokens > 0 {
                    budget.extend_max(extension.additional_tokens);
                }
                if let Some(task) = self.current_task_snapshot.as_mut() {
                    task.context_budget_tokens = Some(budget.max_tokens());
                }
            }
            self.terminated_for_budget = false;
            self.exit_report_back_phase();
            self.grant_continuation_loops_after_budget_extend();
        }
        None
    }

    async fn live_task_context_budget_tokens(&self) -> Option<u32> {
        let task_id = self.task_id.or_else(|| {
            self.current_task_snapshot
                .as_ref()
                .map(|task| task.id.as_str())
        });
        let tasks = self.engine.tasks.lock().await;
        if let Some(task_id) = task_id {
            if let Some(max_tokens) = tasks
                .iter()
                .find(|task| task.id == task_id)
                .and_then(|task| task.context_budget_tokens)
            {
                return Some(max_tokens);
            }
        }
        tasks
            .iter()
            .find(|task| {
                task.thread_id.as_deref() == Some(self.tid.as_str()) && task.is_spawned_subagent()
            })
            .and_then(|task| task.context_budget_tokens)
    }

    pub(super) async fn sync_task_context_budget_from_live_task(&mut self) {
        let Some(live_max) = self.live_task_context_budget_tokens().await else {
            return;
        };
        let Some(budget) = self.task_context_budget.as_mut() else {
            return;
        };
        let previous = budget.max_tokens();
        budget.raise_max_to(live_max);
        let ceiling_raised = budget.max_tokens() > previous;
        if let Some(task) = self.current_task_snapshot.as_mut() {
            task.context_budget_tokens = Some(budget.max_tokens());
        }
        let still_exceeded = matches!(
            budget.check(),
            crate::agent::subagent::context_budget::BudgetStatus::Exceeded { .. }
        );
        if should_exit_report_back_after_live_ceiling_sync(
            self.report_back_phase.is_some(),
            ceiling_raised,
            still_exceeded,
        ) {
            self.terminated_for_budget = false;
            self.exit_report_back_phase();
            self.grant_continuation_loops_after_budget_extend();
        }
    }

    pub(super) async fn capture_text_report_back(&mut self, content: &str) {
        if self.was_cancelled || self.report_back_phase.is_none() || self.subagent_report.is_some()
        {
            return;
        }
        let summary = {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                trimmed.to_string()
            } else {
                self.synthesize_thread_summary().await
            }
        };
        self.subagent_report = Some(SubagentTurnReport {
            status: SubagentReportStatus::Error,
            summary,
            reason: Some("zorai_budget".to_string()),
        });
        self.terminated_for_budget = true;
        self.exit_report_back_phase();
    }

    pub(super) async fn synthesize_thread_summary(&self) -> String {
        let threads = self.engine.threads.read().await;
        threads
            .get(&self.tid)
            .map(|thread| {
                crate::agent::subagent::context_budget::synthesize_visible_assistant_summary(
                    &thread.messages,
                )
            })
            .unwrap_or_else(|| "(no usable assistant summary was recorded)".to_string())
    }

    pub(super) async fn mark_last_assistant_report_back(&self) {
        let mut threads = self.engine.threads.write().await;
        if let Some(thread) = threads.get_mut(&self.tid) {
            if let Some(message) = thread
                .messages
                .iter_mut()
                .rev()
                .find(|message| message.role == crate::agent::types::MessageRole::Assistant)
            {
                message.message_kind = crate::agent::types::AgentMessageKind::ReportBack;
            }
        }
    }

    pub(super) async fn finish_spawned_subagent_on_unrecoverable_error(
        &mut self,
        error: &anyhow::Error,
    ) -> bool {
        if !self.spawned_subagent_turn_active() {
            return false;
        }
        let message = error.to_string();
        let provider_quota = crate::agent::llm_client::parse_structured_upstream_failure(&message)
            .is_some_and(|failure| {
                let combined = format!(
                    "{}\n{}",
                    failure.summary.to_ascii_lowercase(),
                    failure.diagnostics.to_string().to_ascii_lowercase()
                );
                combined.contains("quota")
                    || combined.contains("insufficient credit")
                    || combined.contains("insufficient credits")
                    || combined.contains("billing hard limit")
                    || combined.contains("payment required")
            })
            || {
                let lower = message.to_ascii_lowercase();
                lower.contains("quota exceeded")
                    || lower.contains("insufficient credit")
                    || lower.contains("insufficient credits")
                    || lower.contains("billing hard limit")
                    || lower.contains("payment required")
            };
        if self.report_back_phase.is_none() && !provider_quota {
            return false;
        }
        let summary = self.synthesize_thread_summary().await;
        let reason = if provider_quota {
            "provider_quota"
        } else {
            "zorai_budget"
        };
        self.subagent_report = Some(SubagentTurnReport {
            status: SubagentReportStatus::Error,
            summary,
            reason: Some(reason.to_string()),
        });
        self.terminated_for_budget = self.report_back_phase.is_some() && !provider_quota;
        self.exit_report_back_phase();
        true
    }
}
#[cfg(test)]
#[path = "report_back_tests.rs"]
mod tests;
