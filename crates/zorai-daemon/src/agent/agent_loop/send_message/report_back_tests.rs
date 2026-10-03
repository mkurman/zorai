use super::{
    budget_overflow_decision, should_emit_deferred_turn_done,
    should_emit_tool_execution_limit_error, should_emit_turn_done_on_text_completion,
    should_force_budget_report_back, terminal_subagent_report_requires_turn_done,
    BudgetOverflowDecision,
};
use crate::agent::types::ContextOverflowAction;

#[test]
fn spawned_error_budget_forces_report_back_instead_of_stop() {
    assert_eq!(
        budget_overflow_decision(true, false, ContextOverflowAction::Error),
        BudgetOverflowDecision::EnterReportBack
    );
}

#[test]
fn top_level_error_budget_still_stops() {
    assert_eq!(
        budget_overflow_decision(false, false, ContextOverflowAction::Error),
        BudgetOverflowDecision::Stop
    );
}

#[test]
fn report_back_turn_is_budget_exempt() {
    assert_eq!(
        budget_overflow_decision(true, true, ContextOverflowAction::Error),
        BudgetOverflowDecision::Continue
    );
}

#[test]
fn captured_text_report_must_not_reenter_while_budget_still_exceeded() {
    assert!(
        !should_force_budget_report_back(false, true, true),
        "text report-back clears the phase but already recorded a terminal child report; re-entering would burn another turn and emit Done before more model work"
    );
}

#[test]
fn budget_exceeded_without_report_still_enters_report_back() {
    assert!(should_force_budget_report_back(false, false, true));
}

#[test]
fn already_in_report_back_does_not_force_another_entry() {
    assert!(!should_force_budget_report_back(true, false, true));
}

#[test]
fn continuing_into_budget_report_back_must_not_emit_done() {
    assert!(
        !should_emit_turn_done_on_text_completion(true),
        "listeners treat Done as the end of the turn; emitting it before the extra report-back model call makes UI/stream go idle then resume"
    );
    assert!(
        should_emit_turn_done_on_text_completion(false),
        "a finished text completion still ends the turn"
    );
}

#[test]
fn successful_report_break_still_owes_turn_done() {
    assert!(
        terminal_subagent_report_requires_turn_done(false),
        "report_subagent_outcome breaks the tool loop after the UI has already opened a streaming placeholder on the tool result. The task is already terminal, but without a later Done that placeholder keeps the working indicator up. The text-completion report-back path sets this before continuing; the tool-loop budget path does not, so the report itself has to request the deferred Done"
    );
    assert!(
        !terminal_subagent_report_requires_turn_done(true),
        "a turn that already emitted Done must not request a second one when the report breaks the loop"
    );
}

#[test]
fn deferred_done_fires_after_report_back_continue_unless_approval_paused() {
    assert!(
        should_emit_deferred_turn_done(true, false),
        "skipping Done on continue still requires a terminal Done when the turn actually ends"
    );
    assert!(
        !should_emit_deferred_turn_done(true, true),
        "approval pauses the turn; Done would clear activity while waiting"
    );
    assert!(!should_emit_deferred_turn_done(false, false));
}

#[test]
fn successful_report_after_last_loop_must_not_emit_tool_limit_error() {
    assert!(
        !should_emit_tool_execution_limit_error(false, 9, 9, true),
        "report-back bumps max_loops by one; that extra iteration ends with loop_count >= max_loops, but a successful report is the real outcome. Listeners treat AgentEvent::Error as a failed thread while the dispatcher records the report"
    );
    assert!(
        should_emit_tool_execution_limit_error(false, 8, 8, false),
        "hitting the configured tool-loop cap without a child report is still a tool-limit failure"
    );
    assert!(!should_emit_tool_execution_limit_error(true, 8, 8, false));
    assert!(!should_emit_tool_execution_limit_error(false, 0, 12, false));
    assert!(!should_emit_tool_execution_limit_error(false, 8, 7, false));
}
