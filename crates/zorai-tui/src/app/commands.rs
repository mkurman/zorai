use super::*;
use crate::state::sidebar;
use std::path::{Path, PathBuf};

#[path = "commands_goal_targets.rs"]
mod goal_targets;

use super::target_goal_run_id;

#[derive(Debug, Clone)]
enum GoalSidebarCommandItem {
    Step { step_id: String },
    Checkpoint { step_id: Option<String> },
    Task { target: sidebar::SidebarItemTarget },
    File { thread_id: String, path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoalActionPickerItem {
    PauseGoal,
    ResumeGoal,
    StopGoal,
    DeleteGoal,
    AcceptReview,
    SoftReject,
    HardReject,
    CycleRuntimeAssignment,
    EditRuntimeProvider,
    EditRuntimeModel,
    EditRuntimeReasoning,
    EditRuntimeRole,
    ToggleRuntimeEnabled,
    ToggleRuntimeInherit,
    ApplyRuntimeNextTurn,
    ApplyRuntimeReassignActiveStep,
    ApplyRuntimeRestartActiveStep,
}

impl GoalActionPickerItem {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::PauseGoal => "Pause Goal",
            Self::ResumeGoal => "Resume Goal",
            Self::StopGoal => "Stop Goal",
            Self::DeleteGoal => "Delete Goal",
            Self::AcceptReview => "Accept Review",
            Self::SoftReject => "Soft Reject",
            Self::HardReject => "Hard Reject",
            Self::CycleRuntimeAssignment => "Select Next Runtime Agent",
            Self::EditRuntimeProvider => "Edit Runtime Provider",
            Self::EditRuntimeModel => "Edit Runtime Model",
            Self::EditRuntimeReasoning => "Edit Runtime Reasoning",
            Self::EditRuntimeRole => "Edit Runtime Role",
            Self::ToggleRuntimeEnabled => "Toggle Runtime Enabled",
            Self::ToggleRuntimeInherit => "Toggle Runtime Inherit",
            Self::ApplyRuntimeNextTurn => "Apply Next Turn",
            Self::ApplyRuntimeReassignActiveStep => "Reassign Active Step",
            Self::ApplyRuntimeRestartActiveStep => "Restart Active Step",
        }
    }
}

fn parse_workspace_status(raw: &str) -> Option<zorai_protocol::WorkspaceTaskStatus> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "todo" | "to-do" => Some(zorai_protocol::WorkspaceTaskStatus::Todo),
        "progress" | "in-progress" | "in_progress" | "inprogress" => {
            Some(zorai_protocol::WorkspaceTaskStatus::InProgress)
        }
        "review" | "in-review" | "in_review" | "inreview" => {
            Some(zorai_protocol::WorkspaceTaskStatus::InReview)
        }
        "done" => Some(zorai_protocol::WorkspaceTaskStatus::Done),
        _ => None,
    }
}

fn next_workspace_status_for_commands(
    status: &zorai_protocol::WorkspaceTaskStatus,
) -> zorai_protocol::WorkspaceTaskStatus {
    match status {
        zorai_protocol::WorkspaceTaskStatus::Todo => {
            zorai_protocol::WorkspaceTaskStatus::InProgress
        }
        zorai_protocol::WorkspaceTaskStatus::InProgress => {
            zorai_protocol::WorkspaceTaskStatus::InReview
        }
        zorai_protocol::WorkspaceTaskStatus::InReview => zorai_protocol::WorkspaceTaskStatus::Done,
        zorai_protocol::WorkspaceTaskStatus::Done => zorai_protocol::WorkspaceTaskStatus::Done,
    }
}

pub(super) fn parse_workspace_priority(raw: &str) -> Option<zorai_protocol::WorkspacePriority> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "low" => Some(zorai_protocol::WorkspacePriority::Low),
        "normal" | "medium" => Some(zorai_protocol::WorkspacePriority::Normal),
        "high" => Some(zorai_protocol::WorkspacePriority::High),
        "urgent" => Some(zorai_protocol::WorkspacePriority::Urgent),
        _ => None,
    }
}

pub(super) fn parse_workspace_actor(raw: &str) -> Option<zorai_protocol::WorkspaceActor> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("user") {
        return Some(zorai_protocol::WorkspaceActor::User);
    }
    if raw.eq_ignore_ascii_case("svarog") || raw.eq_ignore_ascii_case("swarog") {
        return Some(zorai_protocol::WorkspaceActor::Agent(
            zorai_protocol::AGENT_ID_SWAROG.to_string(),
        ));
    }
    if let Some(id) = raw
        .strip_prefix("agent:")
        .or_else(|| raw.strip_prefix("agent/"))
    {
        let id = id.trim();
        return (!id.is_empty()).then(|| zorai_protocol::WorkspaceActor::Agent(id.to_string()));
    }
    if let Some(id) = raw
        .strip_prefix("subagent:")
        .or_else(|| raw.strip_prefix("subagent/"))
        .or_else(|| raw.strip_prefix("sub:"))
    {
        let id = id.trim();
        return (!id.is_empty()).then(|| zorai_protocol::WorkspaceActor::Subagent(id.to_string()));
    }
    (!raw.is_empty()).then(|| zorai_protocol::WorkspaceActor::Agent(raw.to_string()))
}

pub(super) fn parse_workspace_actor_field(
    raw: &str,
) -> Option<Option<zorai_protocol::WorkspaceActor>> {
    if matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "none" | "clear" | "-"
    ) {
        Some(None)
    } else {
        parse_workspace_actor(raw).map(Some)
    }
}

#[path = "commands_parts/activate_workspace_toolbar_action_to_submit_image_prompt.rs"]
mod activate_workspace_toolbar_action_to_submit_image_prompt;
#[path = "commands_parts/active_goal_sidebar_item_to_speak_latest_assistant_message.rs"]
mod active_goal_sidebar_item_to_speak_latest_assistant_message;
#[path = "commands_parts/copy_work_context_content_to_regenerate_from_message.rs"]
mod copy_work_context_content_to_regenerate_from_message;
#[path = "commands_parts/export_thread.rs"]
mod export_thread;
#[path = "commands_parts/focus_goal_composer_section.rs"]
mod focus_goal_composer_section;
#[path = "commands_parts/focus_next_goal_workspace_pane_to_select_goal_sidebar_row.rs"]
mod focus_next_goal_workspace_pane_to_select_goal_sidebar_row;
#[path = "commands_parts/fork_message.rs"]
mod fork_message;
#[path = "commands_parts/go_back_thread_to_execute_selected_queued_prompt_action.rs"]
mod go_back_thread_to_execute_selected_queued_prompt_action;
#[path = "commands_parts/goal_composer_defaults.rs"]
mod goal_composer_defaults;
#[path = "commands_parts/known_agent_directive_aliases_to_selected_runtime_assignment_preview.rs"]
mod known_agent_directive_aliases_to_selected_runtime_assignment_preview;
#[path = "commands_parts/mission_control_navigation_state_to_collapse_goal_workspace_selection.rs"]
mod mission_control_navigation_state_to_collapse_goal_workspace_selection;
#[path = "commands_parts/move_workspace_task_from_args_to_activate_workspace_board_target.rs"]
mod move_workspace_task_from_args_to_activate_workspace_board_target;
#[path = "commands_parts/open_new_goal_view_to_delete_workspace_task_from_args.rs"]
mod open_new_goal_view_to_delete_workspace_task_from_args;
#[path = "commands_parts/slash_context.rs"]
mod slash_context;
#[path = "commands_parts/slash_model.rs"]
mod slash_model;
pub(crate) use slash_model::{
    apply_active_thread_provider_model_to_daemon, apply_target_agent_custom_model_locally,
    push_active_thread_execution_profile_to_daemon,
};
#[path = "commands_parts/stage_mission_control_assignment_modal_edit_to_open_selected_spawned.rs"]
mod stage_mission_control_assignment_modal_edit_to_open_selected_spawned;
#[path = "commands_parts/submit_prompt_to_copy_message.rs"]
mod submit_prompt_to_copy_message;

fn builtin_participant_display_name(agent_alias: &str) -> Option<String> {
    let normalized = agent_alias.trim().to_ascii_lowercase();
    if normalized == zorai_protocol::AGENT_ID_SWAROG {
        return Some("Swarog".to_string());
    }
    if normalized == zorai_protocol::AGENT_ID_RAROG {
        return Some(zorai_protocol::AGENT_NAME_RAROG.to_string());
    }
    let canonical = match normalized.as_str() {
        "veles" => "weles",
        "weles" | "swarozyc" | "radogost" | "domowoj" | "swietowit" | "perun" | "mokosh"
        | "dazhbog" | "rod" | "kikimora" | "bannik" | "ovinnik" | "dziad" => normalized.as_str(),
        _ => return None,
    };
    Some(ascii_title_case(canonical))
}

fn ascii_title_case(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::with_capacity(value.len());
    out.push(first.to_ascii_uppercase());
    out.push_str(chars.as_str());
    out
}

fn format_queued_prompt_viewer_body(prompt: &QueuedPrompt) -> String {
    let mut body = String::new();

    if let Some(agent_name) = prompt.participant_agent_name.as_deref() {
        body.push_str(&format!("Participant: {agent_name}\n"));
    }
    if let Some(agent_id) = prompt.participant_agent_id.as_deref() {
        body.push_str(&format!("Agent ID: {agent_id}\n"));
    }
    if let Some(thread_id) = prompt.thread_id.as_deref() {
        body.push_str(&format!("Thread ID: {thread_id}\n"));
    }
    if prompt.force_send {
        body.push_str("Dispatch: forced after interrupting the current stream\n");
    }
    if !body.is_empty() {
        body.push_str("\n--------------------\n\n");
    }

    body.push_str(prompt.text.trim_end());
    body
}
