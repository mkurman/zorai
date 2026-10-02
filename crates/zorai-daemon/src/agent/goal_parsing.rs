//! Goal plan/reflection response parsing, JSON repair, and goal utility helpers.

use std::collections::HashSet;
use uuid::Uuid;

use super::now_millis;
use super::types::*;

#[path = "goal_parsing/parsing.rs"]
mod parsing;
#[cfg(test)]
pub(super) use parsing::goal_plan_json_schema;
pub(super) use parsing::parse_json_block;

#[cfg(test)]
#[path = "goal_parsing/tests.rs"]
mod tests;

#[cfg(test)]
use super::SatisfactionAdaptationMode;

#[cfg(test)]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct GoalPlanResponse {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub steps: Vec<GoalPlanStepResponse>,
    /// Alternatives the LLM considered but rejected during planning (EXPL-03).
    #[serde(default)]
    pub rejected_alternatives: Vec<String>,
}

#[cfg(test)]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct GoalPlanStepResponse {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub kind: GoalRunStepKind,
    #[serde(default, alias = "_criteria", alias = "criteria")]
    pub success_criteria: String,
    #[serde(default)]
    pub execution_binding: Option<String>,
    #[serde(default)]
    pub verification_binding: Option<String>,
    #[serde(default)]
    pub proof_checks: Vec<GoalProofCheck>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub llm_confidence: Option<String>,
    #[serde(default)]
    pub llm_confidence_rationale: Option<String>,
}

/// Check a plan for issues that the model should fix. Returns a list of human-readable problems.
#[cfg(test)]
pub(super) fn collect_plan_issues(plan: &GoalPlanResponse) -> Vec<String> {
    let mut issues = Vec::new();

    if plan.summary.trim().is_empty() {
        issues.push("Plan summary is empty — provide a brief description of the plan.".into());
    }
    if plan.steps.is_empty() {
        issues.push("Plan has no steps — provide at least 2 steps.".into());
    }
    if plan.steps.len() > SatisfactionAdaptationMode::Normal.max_goal_plan_steps() {
        issues.push(format!(
            "Plan has {} steps — reduce to {} or fewer.",
            plan.steps.len(),
            SatisfactionAdaptationMode::Normal.max_goal_plan_steps()
        ));
    }

    for (i, step) in plan.steps.iter().enumerate() {
        let n = i + 1;
        if step.title.trim().is_empty() {
            issues.push(format!("Step {n}: missing title."));
        }
        if step.instructions.trim().is_empty() {
            issues.push(format!("Step {n}: missing instructions."));
        }
        if step.success_criteria.trim().is_empty() {
            issues.push(format!("Step {n}: missing success_criteria."));
        }
        if step.kind == GoalRunStepKind::Unknown {
            issues.push(format!("Step {n}: kind is empty or unknown — must be one of: command, research, reason, memory, skill, specialist, divergent, debate."));
        }
    }

    issues
}

/// Apply safe defaults to a plan after all fix attempts are exhausted.
#[cfg(test)]
pub(super) fn apply_plan_defaults(plan: &mut GoalPlanResponse) {
    plan.summary = plan.summary.trim().to_string();
    if plan.summary.is_empty() {
        plan.summary = plan
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "Goal plan".to_string());
    }
    if plan.steps.is_empty() {
        plan.steps.push(GoalPlanStepResponse {
            title: plan.summary.clone(),
            instructions: plan.summary.clone(),
            kind: GoalRunStepKind::Command,
            success_criteria: "Step completed".to_string(),
            execution_binding: None,
            verification_binding: None,
            proof_checks: Vec::new(),
            session_id: None,
            llm_confidence: None,
            llm_confidence_rationale: None,
        });
    }
    if plan.steps.len() > SatisfactionAdaptationMode::Normal.max_goal_plan_steps() {
        plan.steps
            .truncate(SatisfactionAdaptationMode::Normal.max_goal_plan_steps());
    }
    if plan.rejected_alternatives.len()
        > SatisfactionAdaptationMode::Normal.max_rejected_alternatives()
    {
        plan.rejected_alternatives
            .truncate(SatisfactionAdaptationMode::Normal.max_rejected_alternatives());
    }
    for (i, step) in plan.steps.iter_mut().enumerate() {
        step.title = step.title.trim().to_string();
        step.instructions = step.instructions.trim().to_string();
        step.success_criteria = step.success_criteria.trim().to_string();
        step.execution_binding = step
            .execution_binding
            .take()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        step.verification_binding = step
            .verification_binding
            .take()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        step.session_id = step
            .session_id
            .take()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        step.llm_confidence = step
            .llm_confidence
            .take()
            .map(|v| v.trim().to_lowercase())
            .filter(|v| !v.is_empty());
        step.llm_confidence_rationale = step
            .llm_confidence_rationale
            .take()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        for (proof_index, proof_check) in step.proof_checks.iter_mut().enumerate() {
            proof_check.id = proof_check.id.trim().to_string();
            proof_check.title = proof_check.title.trim().to_string();
            if proof_check.id.is_empty() {
                proof_check.id = format!("proof-{}-{}", i + 1, proof_index + 1);
            }
            if proof_check.title.is_empty() {
                proof_check.title = format!("Proof check {}", proof_index + 1);
            }
        }
        if step.title.is_empty() {
            step.title = format!("Step {}", i + 1);
        }
        if step.instructions.is_empty() {
            step.instructions = step.title.clone();
        }
        if step.success_criteria.is_empty() {
            step.success_criteria = "Step completed successfully".to_string();
        }
        if step.kind == GoalRunStepKind::Unknown {
            step.kind = GoalRunStepKind::Command;
        }
    }
}

pub(super) fn parse_priority_str(value: &str) -> TaskPriority {
    match value {
        "low" => TaskPriority::Low,
        "high" => TaskPriority::High,
        "urgent" => TaskPriority::Urgent,
        _ => TaskPriority::Normal,
    }
}

pub(super) fn task_priority_to_str(value: TaskPriority) -> &'static str {
    match value {
        TaskPriority::Low => "low",
        TaskPriority::Normal => "normal",
        TaskPriority::High => "high",
        TaskPriority::Urgent => "urgent",
    }
}

pub(super) fn summarize_goal_title(goal: &str) -> String {
    let trimmed = goal.trim();
    if trimmed.is_empty() {
        return "Untitled Goal".into();
    }
    summarize_text(trimmed, 72)
}

pub(super) fn normalize_goal_key(goal: &str) -> String {
    goal.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub(super) fn normalized_tool_signature(tool_call: &ToolCall) -> String {
    let normalized_args = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| tool_call.function.arguments.trim().to_string());
    format!("{}:{}", tool_call.function.name, normalized_args)
}

pub(in crate::agent) fn summarize_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let truncated = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

pub(super) fn latest_goal_run_failure(goal_run: &GoalRun, tasks: &[AgentTask]) -> Option<String> {
    goal_run
        .last_error
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            goal_run
                .steps
                .iter()
                .rev()
                .find_map(|step| step.error.clone().filter(|value| !value.trim().is_empty()))
        })
        .or_else(|| {
            tasks.iter().rev().find_map(|task| {
                task.last_error
                    .clone()
                    .or_else(|| task.error.clone())
                    .filter(|value| !value.trim().is_empty())
            })
        })
}

pub(super) fn approval_count_for_tasks(tasks: &[AgentTask]) -> u32 {
    tasks
        .iter()
        .flat_map(|task| task.logs.iter())
        .filter(|log| {
            log.phase == "approval"
                && log
                    .message
                    .to_ascii_lowercase()
                    .contains("managed command paused for operator approval")
        })
        .count() as u32
}

pub(super) fn project_goal_run_snapshot(
    mut goal_run: GoalRun,
    related_tasks: &[AgentTask],
    now: u64,
) -> GoalRun {
    goal_run.current_step_title = goal_run
        .steps
        .get(goal_run.current_step_index)
        .map(|step| step.title.clone());
    goal_run.current_step_kind = goal_run
        .steps
        .get(goal_run.current_step_index)
        .map(|step| step.kind.clone());
    goal_run.awaiting_approval_id = related_tasks
        .iter()
        .find_map(|task| task.awaiting_approval_id.clone());
    if goal_run.status == GoalRunStatus::AwaitingApproval && goal_run.awaiting_approval_id.is_none()
    {
        goal_run.status = if goal_run.steps.is_empty() {
            GoalRunStatus::Queued
        } else {
            GoalRunStatus::Running
        };
    }
    goal_run.child_task_count = if goal_run.child_task_ids.is_empty() {
        related_tasks.len() as u32
    } else {
        goal_run.child_task_ids.iter().collect::<HashSet<_>>().len() as u32
    };
    goal_run.approval_count = approval_count_for_tasks(related_tasks);
    goal_run.failure_cause = latest_goal_run_failure(&goal_run, related_tasks);
    goal_run.duration_ms = goal_run.started_at.map(|started_at| {
        goal_run
            .completed_at
            .unwrap_or(now)
            .saturating_sub(started_at)
    });
    goal_run
}

pub(super) fn make_goal_run_event(
    phase: &str,
    message: &str,
    details: Option<String>,
) -> GoalRunEvent {
    make_goal_run_event_with_todos(phase, message, details, None, Vec::new())
}

pub(super) fn make_goal_run_event_with_todos(
    phase: &str,
    message: &str,
    details: Option<String>,
    step_index: Option<usize>,
    todo_snapshot: Vec<TodoItem>,
) -> GoalRunEvent {
    GoalRunEvent {
        id: format!("goal_event_{}", Uuid::new_v4()),
        timestamp: now_millis(),
        phase: phase.to_string(),
        message: message.to_string(),
        details,
        step_index,
        todo_snapshot,
    }
}

pub(super) fn goal_run_status_message(goal_run: &GoalRun) -> &'static str {
    match goal_run.status {
        GoalRunStatus::Queued => "Goal queued",
        GoalRunStatus::Planning => "Goal planning",
        GoalRunStatus::Running => "Goal running",
        GoalRunStatus::AwaitingApproval => "Goal awaiting approval",
        GoalRunStatus::AwaitingReview => "Goal awaiting supervisor review",
        GoalRunStatus::Paused => "Goal paused",
        GoalRunStatus::Blocked => "Goal blocked by governance gate",
        GoalRunStatus::Completed => "Goal completed",
        GoalRunStatus::Failed => "Goal failed",
        GoalRunStatus::Cancelled => "Goal cancelled",
        GoalRunStatus::Contained => "Goal contained for review",
        GoalRunStatus::Compensated => "Goal compensated",
        GoalRunStatus::PartiallyCompensated => "Goal partially compensated",
        GoalRunStatus::BreakGlass => "Goal completed under break-glass override",
    }
}

pub(super) fn planner_required_for_message(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_lowercase();
    let word_count = lower.split_whitespace().count();
    if word_count >= 24 || trimmed.len() >= 160 {
        return true;
    }

    if trimmed.lines().count() >= 3 {
        return true;
    }

    if trimmed.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("- ")
            || line.starts_with("* ")
            || (line.len() >= 2
                && line.as_bytes()[0].is_ascii_digit()
                && line.as_bytes()[1] == b'.')
    }) {
        return true;
    }

    [
        " then ",
        " also ",
        " after ",
        " before ",
        " next ",
        " first ",
        " second ",
        " third ",
        " plan ",
        " steps ",
        " todo ",
        " workflow ",
        " investigate ",
        " implement ",
        " migrate ",
        " refactor ",
        " compare ",
        " audit ",
        " analyze ",
        " long-running ",
        " autonomous ",
        " goal ",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}
