use std::collections::HashMap;

use super::*;
use crate::agent::types::{SubAgentDefinition, ThreadExecutionProfile};

pub(in crate::agent) fn project_task_runs_with_runtime(
    tasks: &[AgentTask],
    sessions: &[zorai_protocol::SessionInfo],
    execution_profiles: &HashMap<String, ThreadExecutionProfile>,
    sub_agents: &[SubAgentDefinition],
) -> Vec<AgentRun> {
    let task_titles = tasks
        .iter()
        .map(|task| (task.id.as_str(), task.title.as_str()))
        .collect::<HashMap<_, _>>();
    let session_workspaces = sessions
        .iter()
        .map(|session| (session.id.to_string(), session.workspace_id.clone()))
        .collect::<HashMap<_, _>>();

    tasks
        .iter()
        .map(|task| {
            let session_id = task
                .session_id
                .clone()
                .filter(|value| !value.trim().is_empty());
            let (provider, model, reasoning_effort) =
                resolve_run_runtime(task, execution_profiles, sub_agents);
            let workspace_id = session_id
                .as_deref()
                .and_then(|value| session_workspaces.get(value))
                .cloned()
                .flatten();
            let kind = if task.source == "subagent"
                || task
                    .parent_task_id
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                || task
                    .parent_thread_id
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
            {
                AgentRunKind::Subagent
            } else {
                AgentRunKind::Task
            };

            AgentRun {
                id: task.id.clone(),
                task_id: task.id.clone(),
                kind,
                classification: classify_task(task).to_string(),
                title: task.title.clone(),
                description: preview_run_text(task.description.clone()),
                status: task.status,
                priority: task.priority,
                progress: task.progress,
                created_at: task.created_at,
                started_at: task.started_at,
                completed_at: task.completed_at,
                thread_id: task.thread_id.clone(),
                session_id,
                provider,
                model,
                reasoning_effort,
                workspace_id,
                source: task.source.clone(),
                runtime: task.runtime.clone(),
                goal_run_id: task.goal_run_id.clone(),
                goal_run_title: task.goal_run_title.clone(),
                goal_step_id: task.goal_step_id.clone(),
                goal_step_title: task.goal_step_title.clone(),
                parent_run_id: task.parent_task_id.clone(),
                parent_task_id: task.parent_task_id.clone(),
                parent_thread_id: task.parent_thread_id.clone(),
                parent_title: task
                    .parent_task_id
                    .as_deref()
                    .and_then(|value| task_titles.get(value))
                    .map(|value| (*value).to_string()),
                blocked_reason: task.blocked_reason.clone(),
                error: task.error.clone().map(preview_run_text),
                result: task.result.clone().map(preview_run_text),
                last_error: task.last_error.clone().map(preview_run_text),
            }
        })
        .collect()
}

const RUN_LIST_TEXT_CHARS: usize = 500;

fn preview_run_text(value: String) -> String {
    if value.len() <= RUN_LIST_TEXT_CHARS {
        return value;
    }
    let mut end = RUN_LIST_TEXT_CHARS;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn resolve_run_runtime(
    task: &AgentTask,
    execution_profiles: &HashMap<String, ThreadExecutionProfile>,
    sub_agents: &[SubAgentDefinition],
) -> (Option<String>, Option<String>, Option<String>) {
    let nonempty = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    };
    let profile = task
        .thread_id
        .as_deref()
        .and_then(|thread_id| execution_profiles.get(thread_id));
    let sub_agent = task.sub_agent_def_id.as_deref().and_then(|def_id| {
        sub_agents.iter().find(|definition| definition.id == def_id)
    });
    let provider = nonempty(profile.and_then(|profile| profile.provider.as_deref()))
        .or_else(|| nonempty(task.override_provider.as_deref()))
        .or_else(|| sub_agent.and_then(|definition| nonempty(Some(definition.provider.as_str()))));
    let model = nonempty(profile.and_then(|profile| profile.model.as_deref()))
        .or_else(|| nonempty(task.override_model.as_deref()))
        .or_else(|| sub_agent.and_then(|definition| nonempty(Some(definition.model.as_str()))));
    let reasoning_effort = nonempty(profile.and_then(|profile| profile.reasoning_effort.as_deref()))
        .or_else(|| {
            sub_agent.and_then(|definition| nonempty(definition.reasoning_effort.as_deref()))
        });
    (provider, model, reasoning_effort)
}

pub(in crate::agent) fn classify_task(task: &AgentTask) -> &'static str {
    let haystack = format!(
        "{} {} {} {}",
        task.title,
        task.description,
        task.command.as_deref().unwrap_or_default(),
        task.source
    )
    .to_ascii_lowercase();

    if contains_any(
        &haystack,
        &[
            "code",
            "coding",
            "repo",
            "git",
            "diff",
            "patch",
            "file",
            "test",
            "build",
            "compile",
            "rust",
            "typescript",
            "frontend",
            "backend",
            "refactor",
            "implement",
        ],
    ) {
        "coding"
    } else if contains_any(
        &haystack,
        &[
            "browser", "browse", "web", "page", "url", "search", "navigate",
        ],
    ) {
        "browser"
    } else if contains_any(
        &haystack,
        &[
            "slack", "discord", "telegram", "whatsapp", "message", "reply", "channel",
        ],
    ) {
        "messaging"
    } else if contains_any(
        &haystack,
        &[
            "terminal", "shell", "daemon", "deploy", "restart", "service", "ops", "infra",
        ],
    ) {
        "ops"
    } else if contains_any(
        &haystack,
        &[
            "research",
            "investigate",
            "analyze",
            "analyse",
            "explain",
            "read",
            "audit",
        ],
    ) {
        "research"
    } else {
        "mixed"
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}
