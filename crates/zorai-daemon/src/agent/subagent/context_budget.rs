//! Context budget enforcement for sub-agents.
//!
//! Each sub-agent can have a token budget that limits how much context it
//! may accumulate. When exceeded, the configured overflow action determines
//! whether to compress, truncate, or error out.

use crate::agent::types::ContextOverflowAction;
use crate::agent::types::{AgentMessage, MessageRole};

/// Tracks and enforces a context token budget for a sub-agent.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    max_tokens: u32,
    overflow_action: ContextOverflowAction,
    consumed: u32,
}

/// Result of a budget check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetStatus {
    /// Under budget — proceed normally.
    Ok,
    /// Over budget — the overflow action should be taken.
    Exceeded {
        consumed: u32,
        max: u32,
        overflow_action: ContextOverflowAction,
    },
    /// Approaching the limit (> 90% consumed).
    Warning { consumed: u32, max: u32 },
}

impl ContextBudget {
    /// Create a new context budget.
    pub fn new(max_tokens: u32, overflow_action: ContextOverflowAction) -> Self {
        Self {
            max_tokens: max_tokens.max(256),
            overflow_action,
            consumed: 0,
        }
    }

    /// Record token consumption. Returns the new budget status.
    pub fn record(&mut self, tokens: u32) -> BudgetStatus {
        self.consumed = self.consumed.saturating_add(tokens);
        self.check()
    }

    /// Set the consumed count directly (e.g. after re-estimating from thread).
    pub fn set_consumed(&mut self, tokens: u32) {
        self.consumed = tokens;
    }

    /// Check current budget status without recording new tokens.
    pub fn check(&self) -> BudgetStatus {
        if self.consumed > self.max_tokens {
            BudgetStatus::Exceeded {
                consumed: self.consumed,
                max: self.max_tokens,
                overflow_action: self.overflow_action,
            }
        } else if self.consumed > self.max_tokens * 9 / 10 {
            BudgetStatus::Warning {
                consumed: self.consumed,
                max: self.max_tokens,
            }
        } else {
            BudgetStatus::Ok
        }
    }

    /// Remaining tokens before the budget is exceeded.
    pub fn remaining(&self) -> u32 {
        self.max_tokens.saturating_sub(self.consumed)
    }

    /// Current utilization as a percentage (0–100+).
    pub fn utilization_pct(&self) -> u32 {
        if self.max_tokens == 0 {
            return 100;
        }
        (self.consumed as u64 * 100 / self.max_tokens as u64) as u32
    }

    /// The configured maximum.
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens
    }

    /// Currently consumed tokens.
    pub fn consumed(&self) -> u32 {
        self.consumed
    }

    /// The configured overflow action.
    pub fn overflow_action(&self) -> ContextOverflowAction {
        self.overflow_action
    }

    /// Raise the token ceiling without resetting consumed work.
    pub fn extend_max(&mut self, additional: u32) {
        self.max_tokens = self.max_tokens.saturating_add(additional.max(256));
    }

    pub fn raise_max_to(&mut self, max_tokens: u32) {
        if max_tokens > self.max_tokens {
            self.max_tokens = max_tokens.max(256);
        }
    }
}

/// Count the visible assistant output tokens that apply to a sub-agent's
/// generated-token budget.
///
/// This intentionally ignores prompt/input tokens, stored reasoning text, and
/// provider aggregate totals that may fold hidden reasoning into output usage.
/// Enforcement is based on visible assistant text plus emitted tool-call
/// names/arguments.
pub(crate) fn visible_output_budget_tokens(messages: &[AgentMessage]) -> u32 {
    messages
        .iter()
        .filter(|message| {
            message.role == MessageRole::Assistant
                && message.message_kind != crate::agent::types::AgentMessageKind::ReportBack
        })
        .map(visible_assistant_message_tokens)
        .fold(0u32, u32::saturating_add)
}

pub(crate) fn synthesize_visible_assistant_summary(messages: &[AgentMessage]) -> String {
    let mut parts = Vec::new();
    for message in messages.iter().rev() {
        if message.role != MessageRole::Assistant {
            continue;
        }
        let content = message.content.trim();
        if content.is_empty() {
            continue;
        }
        parts.push(content.to_string());
        if parts.len() >= 3 {
            break;
        }
    }
    parts.reverse();
    let summary = parts.join("\n\n");
    if summary.is_empty() {
        return "(no usable assistant summary was recorded)".to_string();
    }
    if summary.chars().count() <= 4_000 {
        summary
    } else {
        let truncated: String = summary.chars().take(4_000).collect();
        format!("{truncated}…")
    }
}

fn visible_assistant_message_tokens(message: &AgentMessage) -> u32 {
    let mut chars = message.content.chars().count();
    if let Some(tool_calls) = &message.tool_calls {
        chars = chars.saturating_add(
            tool_calls
                .iter()
                .map(|call| {
                    call.function.name.chars().count() + call.function.arguments.chars().count()
                })
                .sum::<usize>(),
        );
    }
    if chars == 0 {
        0
    } else {
        chars
            .div_ceil(crate::agent::APPROX_CHARS_PER_TOKEN)
            .saturating_add(12)
            .min(u32::MAX as usize) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{
        AgentMessage, AgentMessageKind, MessageRole, ToolCall, ToolFunction,
    };

    #[test]
    fn new_budget_starts_at_zero() {
        let budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        assert_eq!(budget.consumed(), 0);
        assert_eq!(budget.remaining(), 10_000);
        assert_eq!(budget.utilization_pct(), 0);
    }

    #[test]
    fn minimum_budget_is_256() {
        let budget = ContextBudget::new(10, ContextOverflowAction::Compress);
        assert_eq!(budget.max_tokens(), 256);
    }

    #[test]
    fn record_tracks_consumption() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.record(3000);
        assert_eq!(budget.consumed(), 3000);
        assert_eq!(budget.remaining(), 7000);
        assert_eq!(budget.utilization_pct(), 30);
    }

    #[test]
    fn check_ok_when_under_budget() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.record(5000);
        assert_eq!(budget.check(), BudgetStatus::Ok);
    }

    #[test]
    fn check_warning_above_90_percent() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.record(9100);
        assert!(matches!(budget.check(), BudgetStatus::Warning { .. }));
    }

    #[test]
    fn check_exceeded_over_budget() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        let status = budget.record(10_001);
        assert!(matches!(status, BudgetStatus::Exceeded { .. }));
    }

    #[test]
    fn exceeded_carries_overflow_action_compress() {
        let mut budget = ContextBudget::new(1000, ContextOverflowAction::Compress);
        let status = budget.record(1500);
        match status {
            BudgetStatus::Exceeded {
                overflow_action, ..
            } => {
                assert_eq!(overflow_action, ContextOverflowAction::Compress);
            }
            _ => panic!("expected Exceeded"),
        }
    }

    #[test]
    fn exceeded_carries_overflow_action_error() {
        let mut budget = ContextBudget::new(1000, ContextOverflowAction::Error);
        let status = budget.record(2000);
        match status {
            BudgetStatus::Exceeded {
                overflow_action, ..
            } => {
                assert_eq!(overflow_action, ContextOverflowAction::Error);
            }
            _ => panic!("expected Exceeded"),
        }
    }

    #[test]
    fn record_returns_ok_then_warning_then_exceeded() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Truncate);
        assert_eq!(budget.record(5000), BudgetStatus::Ok);
        assert!(matches!(budget.record(4200), BudgetStatus::Warning { .. }));
        assert!(matches!(budget.record(1000), BudgetStatus::Exceeded { .. }));
    }

    #[test]
    fn set_consumed_updates_state() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.set_consumed(8000);
        assert_eq!(budget.consumed(), 8000);
        assert_eq!(budget.remaining(), 2000);
    }

    #[test]
    fn utilization_pct_at_exact_limit() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.set_consumed(10_000);
        assert_eq!(budget.utilization_pct(), 100);
    }

    #[test]
    fn utilization_pct_over_limit() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.set_consumed(15_000);
        assert_eq!(budget.utilization_pct(), 150);
    }

    #[test]
    fn saturating_addition_on_record() {
        let mut budget = ContextBudget::new(10_000, ContextOverflowAction::Compress);
        budget.record(u32::MAX - 10);
        budget.record(100);
        assert_eq!(budget.consumed(), u32::MAX);
    }

    #[test]
    fn extend_max_raises_ceiling_without_resetting_consumed() {
        let mut budget = ContextBudget::new(1_000, ContextOverflowAction::Error);
        budget.set_consumed(1_400);
        assert!(matches!(budget.check(), BudgetStatus::Exceeded { .. }));
        budget.extend_max(500);
        assert_eq!(budget.max_tokens(), 1_500);
        assert_eq!(budget.consumed(), 1_400);
        assert!(matches!(budget.check(), BudgetStatus::Warning { .. }));
    }

    #[test]
    fn raise_max_to_adopts_higher_persisted_ceiling_without_resetting_consumed() {
        let mut budget = ContextBudget::new(1_000, ContextOverflowAction::Error);
        budget.set_consumed(1_400);
        budget.raise_max_to(1_512);
        assert_eq!(budget.max_tokens(), 1_512);
        assert_eq!(budget.consumed(), 1_400);
        assert!(matches!(budget.check(), BudgetStatus::Warning { .. }));
    }

    #[test]
    fn raise_max_to_does_not_lower_an_already_higher_ceiling() {
        let mut budget = ContextBudget::new(2_000, ContextOverflowAction::Error);
        budget.raise_max_to(1_500);
        assert_eq!(budget.max_tokens(), 2_000);
    }

    #[test]
    fn visible_output_budget_tokens_ignore_report_back_messages() {
        let counted = assistant_message("visible work output", None, 7, 0, None);
        let mut report = assistant_message(
            "forced report-back summary should not count",
            None,
            7,
            0,
            None,
        );
        report.message_kind = crate::agent::types::AgentMessageKind::ReportBack;
        let without_report = visible_output_budget_tokens(&[counted.clone()]);
        assert_eq!(
            visible_output_budget_tokens(&[counted, report]),
            without_report
        );
    }

    #[test]
    fn visible_output_budget_tokens_ignore_input_and_reasoning() {
        let mut user = AgentMessage::user("large prompt text should not count", 1);
        user.input_tokens = 50_000;

        let assistant = assistant_message(
            "visible answer",
            Some("private reasoning should not count".to_string()),
            7,
            100_000,
            None,
        );
        let tool_calling_assistant = assistant_message(
            "using a tool",
            None,
            3,
            1_000,
            Some(vec![ToolCall {
                id: "call-1".to_string(),
                function: ToolFunction {
                    name: "read_file".to_string(),
                    arguments: "{\"path\":\"README.md\"}".to_string(),
                },
                weles_review: None,
            }]),
        );

        let first_count = visible_output_budget_tokens(&[
            user.clone(),
            assistant.clone(),
            tool_calling_assistant.clone(),
        ]);
        assert!(first_count > 0);

        user.input_tokens = 1_000_000;
        let mut inflated_assistant = assistant;
        inflated_assistant.output_tokens = 1_000_000;
        inflated_assistant.reasoning = Some("much more hidden reasoning".repeat(100));

        assert_eq!(
            visible_output_budget_tokens(&[user, inflated_assistant, tool_calling_assistant]),
            first_count
        );
    }

    fn assistant_message(
        content: &str,
        reasoning: Option<String>,
        output_tokens: u64,
        input_tokens: u64,
        tool_calls: Option<Vec<ToolCall>>,
    ) -> AgentMessage {
        AgentMessage {
            reasoning_tokens: None,
            thinking_ms: None,
            id: crate::agent::types::generate_message_id(),
            role: MessageRole::Assistant,
            content: content.to_string(),
            content_blocks: Vec::new(),
            tool_calls,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens,
            output_tokens,
            cost: None,
            provider: None,
            model: None,
            api_transport: None,
            response_id: None,
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 1,
            feedback: None,
        }
    }
}
