#![allow(dead_code)]

//! Task prompt building and session resolution.

use std::sync::Arc;

use super::agent_identity::{
    is_main_agent_scope, MAIN_AGENT_ID, MAIN_AGENT_NAME, WELES_AGENT_NAME,
};
use super::task_scheduler::describe_scheduled_time;
use super::types::*;

pub(super) struct TaskPromptHandles {
    pub task: String,
    pub goal: Option<String>,
    pub parent_task: Option<String>,
    pub dependencies: Vec<String>,
}

impl TaskPromptHandles {
    fn from_task(task: &AgentTask) -> Self {
        Self {
            task: task.id.clone(),
            goal: task.goal_run_id.clone(),
            parent_task: task.parent_task_id.clone(),
            dependencies: task.dependencies.clone(),
        }
    }
}

pub(super) fn task_dispatch_prompt_marker(label: &str) -> String {
    format!("Current task handle: {label}")
}

pub(super) fn content_has_task_dispatch_prompt(content: &str, task_id: &str) -> bool {
    content.contains(&task_dispatch_prompt_marker(task_id))
        || content.contains(&format!("Current task ID: {task_id}"))
}

pub(super) async fn build_dispatched_task_prompt(
    engine: &crate::agent::AgentEngine,
    task: &AgentTask,
) -> String {
    let task_handle = crate::agent::tool_executor::ensure_task_slug(engine, task)
        .await
        .unwrap_or_else(|_| task.id.clone());
    let goal = match task.goal_run_id.as_deref() {
        Some(goal_id) => {
            let goal_run = engine.get_goal_run(goal_id).await;
            let title = goal_run
                .as_ref()
                .map(|goal| goal.title.clone())
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| goal_id.to_string());
            let thread_id = goal_run.as_ref().and_then(|goal| goal.thread_id.clone());
            Some(
                crate::agent::tool_executor::ensure_goal_slug(
                    engine,
                    goal_id,
                    &title,
                    thread_id.as_deref(),
                )
                .await
                .unwrap_or_else(|_| goal_id.to_string()),
            )
        }
        None => None,
    };
    let parent_task = match task.parent_task_id.as_deref() {
        Some(parent_id) => match engine.task_by_id_for_dispatcher(parent_id).await {
            Some(parent) => Some(
                crate::agent::tool_executor::ensure_task_slug(engine, &parent)
                    .await
                    .unwrap_or_else(|_| parent_id.to_string()),
            ),
            None => Some(parent_id.to_string()),
        },
        None => None,
    };
    let mut dependencies = Vec::with_capacity(task.dependencies.len());
    for dependency_id in &task.dependencies {
        let handle = match engine.task_by_id_for_dispatcher(dependency_id).await {
            Some(dependency) => crate::agent::tool_executor::ensure_task_slug(engine, &dependency)
                .await
                .unwrap_or_else(|_| dependency_id.clone()),
            None => dependency_id.clone(),
        };
        dependencies.push(handle);
    }
    let mut prompt = build_task_prompt_with_handles(
        task,
        &TaskPromptHandles {
            task: task_handle,
            goal,
            parent_task,
            dependencies,
        },
    );
    append_goal_run_context(engine, &mut prompt, task).await;
    prompt
}

pub(super) fn build_task_prompt(task: &AgentTask) -> String {
    build_task_prompt_with_handles(task, &TaskPromptHandles::from_task(task))
}

fn build_task_prompt_with_handles(task: &AgentTask, handles: &TaskPromptHandles) -> String {
    let mut prompt = format!(
        "Execute the following queued task.\n\nCurrent task handle: {}\nTitle: {}\nDescription: {}",
        handles.task, task.title, task.description
    );

    prompt.push_str(
        "\nUse the shell execution tools available in the current turn when work needs a shell, a real PTY, or operator-governed command execution.",
    );
    prompt.push_str(
        "\nFor browser-readable pages, search/discovery work, or HTTP reads where you need text or JSON content, prefer web_search and fetch_url. For direct reachability checks, HEAD or range requests, large or binary downloads, or streaming transfers, prefer curl or wget through shell execution instead of fetch_url.",
    );
    prompt.push_str(
        "\nIf the task is more than a one-shot action, call update_todo immediately with a concise plan and keep it current as steps advance.",
    );
    let is_top_level_task = !task.is_spawned_subagent();
    if is_top_level_task {
        prompt.push_str(
            "\nIf this task is large and parallelizable, use spawn_subagent for bounded child work items and monitor them with list_subagents.",
        );
    }
    prompt.push_str(
        "\nWhen this task involves work on a directory or repository path, check for `AGENTS.md` in that directory and its nearest relevant parent when repo-specific guidance is likely relevant. If present, apply the instructions that matter, but do not turn reading `AGENTS.md` into a standalone plan step unless the task specifically requires it.",
    );
    if is_top_level_task {
        prompt.push_str(
            "\nWhen delegating directory or repository work with spawn_subagent, include that `AGENTS.md` path in the child assignment when one exists and pass along any applicable guidance; do not frame reading it as the child's first goal.",
        );
        prompt.push_str(
            "\nDo not use list_agents to check spawned child progress, and do not busy-wait on active subagents. If delegation is in flight and no other useful work remains, send a concise progress update and stop so zorai can resume you when children finish.",
        );
    }

    if let Some(command) = task.command.as_deref() {
        prompt.push_str(&format!("\nPreferred command or entrypoint: {command}"));
    }

    if let Some(session_id) = task.session_id.as_deref() {
        prompt.push_str(&format!("\nPreferred terminal session: {session_id}"));
    }

    if let Some(goal_handle) = handles.goal.as_deref() {
        prompt.push_str(&format!("\nGoal handle: {goal_handle}"));
        prompt.push_str(
            "\nYou are the sole worker for this goal. Work until the objective is met, then call request_goal_review with a concrete report. Do not claim the goal is complete in prose.",
        );
        prompt.push_str(
            "\nIn goal work, ask_questions is unavailable. The owner supervisor verdicts completeness; do not call submit_goal_review from this worker.",
        );
        if task.parent_task_id.is_some() {
            prompt.push_str(&format!(
                "\nIf you need clarification or a narrow decision from the main executor, use message_agent targeting {MAIN_AGENT_NAME} and ask for the smallest concrete answer needed to continue."
            ));
        } else {
            prompt.push_str(&format!(
                "\nIf you need a review, challenge, or second opinion before proceeding, use message_agent targeting {WELES_AGENT_NAME} so the review happens on its own internal thread instead of the operator thread."
            ));
        }
        if task.source == "goal_run" && task.parent_task_id.is_none() {
            prompt.push_str(
                "\nWhen you believe the work is done, call request_goal_review. Do not cancel this task to force progress.",
            );
        }
    }

    if task.is_spawned_subagent() {
        if let Some(parent_handle) = handles.parent_task.as_deref() {
            prompt.push_str(&format!("\nParent task handle: {parent_handle}"));
        }
        prompt.push_str(
            "\nYou are running as a supervised subagent. Stay tightly scoped to this assignment, avoid duplicating sibling work, and report concise results back through your normal response.",
        );
    }

    prompt.push_str(&format!("\nAssigned runtime: {}", task.runtime));

    if let Some(scheduled_at) = task.scheduled_at {
        prompt.push_str(&format!(
            "\nOriginal schedule: {}",
            describe_scheduled_time(scheduled_at)
        ));
    }

    if !handles.dependencies.is_empty() {
        prompt.push_str(&format!(
            "\nResolved dependencies: {}",
            handles.dependencies.join(", ")
        ));
    }

    let recent_subagent_logs = task
        .logs
        .iter()
        .rev()
        .filter(|log| log.phase == "subagent")
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|log| {
            let details = log
                .details
                .as_deref()
                .map(|value| format!(" ({value})"))
                .unwrap_or_default();
            format!("- {}", log.message) + &details
        })
        .collect::<Vec<_>>();
    if !recent_subagent_logs.is_empty() {
        prompt.push_str("\nRecent subagent updates:\n");
        prompt.push_str(&recent_subagent_logs.join("\n"));
    }

    if task.retry_count > 0 {
        prompt.push_str(&format!(
            "\n\nThis is self-healing retry attempt {} of {}.",
            task.retry_count, task.max_retries
        ));
        if let Some(last_error) = task.last_error.as_deref() {
            prompt.push_str(&format!("\nLast failure: {last_error}"));
        }
        let recent_logs = task
            .logs
            .iter()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|log| format!("- [{}] {}", log.phase, log.message))
            .collect::<Vec<_>>();
        if !recent_logs.is_empty() {
            prompt.push_str("\nRecent task log:\n");
            prompt.push_str(&recent_logs.join("\n"));
        }
        prompt.push_str(
            "\nAnalyze the root cause, adapt the approach, and retry with the smallest viable correction.",
        );
    } else {
        prompt.push_str(
            "\n\nWork through this step by step. Use your tools as needed and report your progress clearly.",
        );
    }

    prompt
}

fn goal_run_task_context(goal_run: &GoalRun, task: &AgentTask) -> String {
    let completed_steps = goal_run
        .steps
        .iter()
        .enumerate()
        .filter(|(_, step)| step.status == GoalRunStepStatus::Completed)
        .collect::<Vec<_>>();
    let total_steps = goal_run.steps.len();
    let current_progress = if total_steps == 0 {
        "planning pending; no goal steps have been completed yet".to_string()
    } else {
        let mut line = format!(
            "{} of {} steps completed",
            completed_steps.len(),
            total_steps
        );
        if let Some(step) = goal_run.steps.get(goal_run.current_step_index) {
            line.push_str(&format!(
                "; current focus: step {} `{}`",
                goal_run.current_step_index + 1,
                step.title
            ));
        } else {
            line.push_str("; all planned steps are currently marked complete");
        }
        line
    };

    let mut context = format!(
        "\n\n## Goal Big Picture\nTitle: {}\nObjective: {}\nCurrent progress: {}",
        goal_run.title, goal_run.goal, current_progress
    );
    if let Some(plan_summary) = goal_run
        .plan_summary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        context.push_str(&format!("\nPlan summary: {plan_summary}"));
    }
    if let Some(step_title) = task.goal_step_title.as_deref() {
        context.push_str(&format!(
            "\nYour exact assignment in this goal: {step_title}"
        ));
    }
    if !completed_steps.is_empty() {
        context.push_str("\nCompleted steps so far:");
        for (index, step) in completed_steps.into_iter().take(5) {
            let summary = step
                .summary
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("completed without a recorded summary");
            context.push_str(&format!(
                "\n- Step {} `{}`: {}",
                index + 1,
                step.title,
                summary
            ));
        }
    }

    context
}

pub(super) async fn append_goal_run_context(
    engine: &crate::agent::AgentEngine,
    prompt: &mut String,
    task: &AgentTask,
) {
    let Some(goal_run_id) = task.goal_run_id.as_deref() else {
        return;
    };
    let Some(goal_run) = engine.get_goal_run(goal_run_id).await else {
        return;
    };
    prompt.push_str(&goal_run_task_context(&goal_run, task));
}

pub(super) async fn resolve_preferred_session_id(
    session_manager: &Arc<crate::session_manager::SessionManager>,
    session_hint: Option<&str>,
) -> Option<zorai_protocol::SessionId> {
    let hint = session_hint?.trim();
    if hint.is_empty() {
        return None;
    }

    session_manager
        .list()
        .await
        .into_iter()
        .find(|session| {
            let session_id = session.id.to_string();
            session_id == hint || session_id.contains(hint)
        })
        .map(|session| session.id)
}

pub(crate) fn agent_data_dir() -> std::path::PathBuf {
    zorai_protocol::zorai_root_dir().join("agent")
}

pub(super) fn ordered_memory_dirs(agent_data_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let root = agent_data_dir.parent().unwrap_or(std::path::Path::new("."));
    let legacy_dir = root.join("agent-mission");
    let prefers_legacy = agent_data_dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("agent"));

    let mut dirs = if prefers_legacy {
        vec![legacy_dir, agent_data_dir.to_path_buf()]
    } else {
        vec![agent_data_dir.to_path_buf(), legacy_dir]
    };
    dirs.dedup();
    dirs
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryPaths {
    pub memory_dir: std::path::PathBuf,
    pub memory_path: std::path::PathBuf,
    pub soul_path: std::path::PathBuf,
    pub user_path: std::path::PathBuf,
}

pub(super) fn memory_paths_for_scope(
    agent_data_dir: &std::path::Path,
    scope_id: &str,
) -> MemoryPaths {
    let shared_user_dir = active_memory_dir_for_scope(agent_data_dir, MAIN_AGENT_ID);
    let memory_dir = active_memory_dir_for_scope(agent_data_dir, scope_id);
    MemoryPaths {
        memory_path: memory_dir.join("MEMORY.md"),
        soul_path: memory_dir.join("SOUL.md"),
        user_path: shared_user_dir.join("USER.md"),
        memory_dir,
    }
}

pub(crate) fn skills_dir(data_dir: &std::path::Path) -> std::path::PathBuf {
    let default_agent_dir = agent_data_dir();
    if data_dir == default_agent_dir {
        return zorai_protocol::zorai_skills_dir();
    }

    data_dir
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("skills")
}

pub(crate) fn guidelines_dir(data_dir: &std::path::Path) -> std::path::PathBuf {
    let default_agent_dir = agent_data_dir();
    if data_dir == default_agent_dir {
        return zorai_protocol::zorai_guidelines_dir();
    }

    data_dir
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("guidelines")
}

fn builtin_skills_source_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills")
}

fn builtin_guidelines_source_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../guidelines")
}

/// Seed built-in skill documents into `~/.zorai/skills/`.
pub(super) fn seed_builtin_skills(agent_data_dir: &std::path::Path) {
    let root = skills_dir(agent_data_dir);
    let source = builtin_skills_source_dir();
    let target = root;

    match seed_skills_tree(&source, &target) {
        Ok(count) => tracing::debug!("seeded {} built-in skills into {}", count, target.display()),
        Err(e) => tracing::warn!(
            "failed to seed built-in skills from {} to {}: {e}",
            source.display(),
            target.display()
        ),
    }
}

/// Seed missing built-in guideline documents into `~/.zorai/guidelines/`.
pub(super) fn seed_builtin_guidelines(agent_data_dir: &std::path::Path) {
    let root = guidelines_dir(agent_data_dir);
    let source = builtin_guidelines_source_dir();
    let target = root;

    match seed_guidelines_tree(&source, &target) {
        Ok(count) => tracing::debug!(
            "seeded {} missing built-in guidelines into {}",
            count,
            target.display()
        ),
        Err(e) => tracing::warn!(
            "failed to seed built-in guidelines from {} to {}: {e}",
            source.display(),
            target.display()
        ),
    }
}

fn seed_skills_tree(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(target)?;

    let mut count = 0usize;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if file_type.is_dir() {
            count += seed_skills_tree(&source_path, &target_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&source_path, &target_path)?;
            count += 1;
        }
    }

    Ok(count)
}

fn seed_guidelines_tree(
    source: &std::path::Path,
    target: &std::path::Path,
) -> std::io::Result<usize> {
    std::fs::create_dir_all(target)?;

    let mut count = 0usize;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if file_type.is_dir() {
            count += seed_guidelines_tree(&source_path, &target_path)?;
        } else if file_type.is_file() {
            if target_path.exists() {
                continue;
            }
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&source_path, &target_path)?;
            count += 1;
        }
    }

    Ok(count)
}

pub(super) fn dir_has_memory_files(dir: &std::path::Path) -> bool {
    ["MEMORY.md", "SOUL.md", "USER.md"]
        .iter()
        .any(|name| dir.join(name).exists())
}

fn persona_memory_dir(agent_data_dir: &std::path::Path, scope_id: &str) -> std::path::PathBuf {
    active_memory_dir_for_scope(agent_data_dir, MAIN_AGENT_ID)
        .join("personas")
        .join(scope_id)
}

pub(super) fn active_memory_dir_for_scope(
    agent_data_dir: &std::path::Path,
    scope_id: &str,
) -> std::path::PathBuf {
    if !is_main_agent_scope(scope_id) {
        return persona_memory_dir(agent_data_dir, scope_id);
    }

    let agent_dir_name = agent_data_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !agent_dir_name.eq_ignore_ascii_case("agent") {
        return agent_data_dir.to_path_buf();
    }

    let dirs = ordered_memory_dirs(agent_data_dir);
    if let Some(path) = dirs.iter().find(|dir| dir_has_memory_files(dir)) {
        return path.clone();
    }
    if let Some(path) = dirs.iter().find(|dir| dir.exists()) {
        return path.clone();
    }
    dirs.first()
        .cloned()
        .unwrap_or_else(|| agent_data_dir.to_path_buf())
}

pub(super) fn active_memory_dir(agent_data_dir: &std::path::Path) -> std::path::PathBuf {
    active_memory_dir_for_scope(agent_data_dir, MAIN_AGENT_ID)
}

/// Read a string from a legacy frontend settings payload using either the nested
/// `/settings/<key>` path or a top-level `<key>` fallback.
pub(super) fn read_setting_str(v: &serde_json::Value, key: &str) -> String {
    let pointer = format!("/settings/{key}");
    v.pointer(&pointer)
        .or_else(|| v.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub(crate) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(super) async fn persist_json<T: serde::Serialize>(
    path: &std::path::Path,
    data: &T,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(data)?;
    tokio::fs::write(path, json).await?;
    Ok(())
}

/// Load agent config using a shared HistoryStore, returning defaults if not found.
pub async fn load_config_from_history(
    history: &crate::history::HistoryStore,
) -> anyhow::Result<AgentConfig> {
    let items = history.list_agent_config_items().await?;
    super::config::load_config_from_items(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_identity::{MAIN_AGENT_ID, RADOGOST_AGENT_ID};
    use crate::agent::AutonomyLevel;
    use tempfile::tempdir;

    fn sample_task() -> AgentTask {
        AgentTask {
            id: "task-1".to_string(),
            title: "Investigate search failures".to_string(),
            description: "Diagnose why remote research is failing.".to_string(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some("thread-1".to_string()),
            source: "goal_run".to_string(),
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
    fn build_task_prompt_requires_agents_md_lookup_for_directory_work() {
        let prompt = build_task_prompt(&sample_task());

        assert!(prompt.contains("AGENTS.md"));
        assert!(prompt.contains("directory or repository path"));
        assert!(prompt.contains("nearest relevant parent"));
        assert!(prompt.contains("spawn_subagent"));
        assert!(prompt.contains("do not turn reading `AGENTS.md` into a standalone plan step"));
        assert!(prompt.contains("do not frame reading it as the child's first goal"));
        assert!(!prompt.contains("before planning or editing"));
        assert!(!prompt.contains("tell the child to read it first"));
    }

    #[test]
    fn build_task_prompt_omits_spawn_guidance_for_supervised_subagents() {
        let mut task = sample_task();
        task.parent_task_id = Some("parent-1".to_string());

        let prompt = build_task_prompt(&task);

        assert!(
            !prompt.contains("spawn_subagent"),
            "a supervised subagent must not be told to spawn its own children"
        );
        assert!(
            !prompt.contains("list_subagents"),
            "a supervised subagent has no children to monitor"
        );
        assert!(
            prompt.contains("You are running as a supervised subagent"),
            "the subagent should still get its scoping note"
        );
        assert!(
            prompt.contains("check for `AGENTS.md`"),
            "general directory guidance must remain for subagents"
        );
    }

    #[test]
    fn build_task_prompt_omits_spawn_guidance_for_chat_spawned_subagents() {
        let mut task = sample_task();
        task.source = "subagent".to_string();
        task.parent_thread_id = Some("thread-parent".to_string());

        assert!(
            task.is_spawned_subagent(),
            "chat-spawned children are spawned even without a parent task id"
        );

        let prompt = build_task_prompt(&task);

        assert!(
            !prompt.contains("spawn_subagent"),
            "a chat-spawned subagent must not be told to spawn its own children"
        );
        assert!(
            !prompt.contains("list_subagents"),
            "a chat-spawned subagent must not receive child-monitoring guidance"
        );
        assert!(
            prompt.contains("You are running as a supervised subagent"),
            "chat-spawned children still need the supervised scoping note"
        );
    }

    #[test]
    fn build_task_prompt_distinguishes_web_reads_from_binary_downloads() {
        let prompt = build_task_prompt(&sample_task());

        assert!(!prompt.contains(zorai_protocol::tool_names::EXECUTE_MANAGED_COMMAND));
        assert!(prompt.contains(zorai_protocol::tool_names::WEB_SEARCH));
        assert!(prompt.contains(zorai_protocol::tool_names::FETCH_URL));
        assert!(prompt.contains("curl"));
        assert!(prompt.contains("wget"));
        assert!(prompt.contains("large or binary"));
    }

    #[test]
    fn build_task_prompt_for_goal_run_forbids_operator_questions_and_routes_autonomously() {
        let mut task = sample_task();
        task.goal_run_id = Some("goal-123".to_string());
        task.goal_step_id = Some("step-1".to_string());
        task.goal_step_title = Some("Review integration notes".to_string());

        let prompt = build_task_prompt(&task);

        assert!(
            prompt.contains("sole worker"),
            "goal-run prompts should tell the worker they own the whole goal"
        );
        assert!(
            prompt.contains("ask_questions is unavailable"),
            "goal-run prompts should forbid direct operator questions"
        );
        assert!(
            prompt.contains("request_goal_review"),
            "goal-run prompts should require an explicit supervisor review call"
        );
    }

    #[test]
    fn build_task_prompt_includes_current_task_id() {
        let task = sample_task();

        let prompt = build_task_prompt(&task);

        assert!(
            prompt.contains(&task_dispatch_prompt_marker("task-1")),
            "queued task prompts should expose the durable task id for task-scoped tools"
        );
    }

    #[test]
    fn content_has_task_dispatch_prompt_matches_durable_task_id() {
        let task = sample_task();
        let prompt = build_task_prompt(&task);

        assert!(content_has_task_dispatch_prompt(&prompt, "task-1"));
        assert!(!content_has_task_dispatch_prompt(&prompt, "task-2"));
    }

    #[test]
    fn goal_run_task_context_includes_big_picture_and_progress() {
        let mut task = sample_task();
        task.goal_run_id = Some("goal-123".to_string());
        task.goal_step_title = Some("Review integration notes".to_string());

        let goal_run = GoalRun {
            id: "goal-123".to_string(),
            title: "Release Goal".to_string(),
            goal: "Ship the release with full verification".to_string(),
            client_request_id: None,
            status: GoalRunStatus::Running,
            priority: TaskPriority::Normal,
            created_at: 1,
            updated_at: 1,
            started_at: Some(1),
            completed_at: None,
            thread_id: Some("thread-goal".to_string()),
            session_id: None,
            current_step_index: 1,
            current_step_title: Some("step-2".to_string()),
            current_step_kind: Some(GoalRunStepKind::Command),
            planner_owner_profile: None,
            current_step_owner_profile: None,
            step_owner_overrides: std::collections::BTreeMap::new(),
            replan_count: 0,
            max_replans: 2,
            plan_summary: Some("Implement, verify, and package the release.".to_string()),
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            step_failure_history: Vec::new(),
            stopped_reason: None,
            child_task_ids: Vec::new(),
            child_task_count: 0,
            approval_count: 0,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            active_task_id: None,
            pending_review_report: None,
            duration_ms: None,
            steps: vec![
                GoalRunStep {
                    id: "step-1".to_string(),
                    position: 0,
                    title: "step-1".to_string(),
                    instructions: "Implement".to_string(),
                    kind: GoalRunStepKind::Command,
                    success_criteria: "done".to_string(),
                    session_id: None,
                    status: GoalRunStepStatus::Completed,
                    task_id: None,
                    summary: Some("Implementation landed".to_string()),
                    error: None,
                    started_at: Some(1),
                    completed_at: Some(2),
                },
                GoalRunStep {
                    id: "step-2".to_string(),
                    position: 1,
                    title: "step-2".to_string(),
                    instructions: "Review".to_string(),
                    kind: GoalRunStepKind::Command,
                    success_criteria: "review passes".to_string(),
                    session_id: None,
                    status: GoalRunStepStatus::InProgress,
                    task_id: Some("task-2".to_string()),
                    summary: None,
                    error: None,
                    started_at: Some(3),
                    completed_at: None,
                },
            ],
            events: Vec::new(),
            dossier: None,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            model_usage: Vec::new(),
            autonomy_level: AutonomyLevel::Autonomous,
            authorship_tag: None,
            launch_assignment_snapshot: Vec::new(),
            runtime_assignment_list: Vec::new(),
            root_thread_id: Some("thread-goal".to_string()),
            supervision_thread_id: None,
            active_thread_id: Some("thread-goal".to_string()),
            execution_thread_ids: vec!["thread-goal".to_string()],
        };

        let context = goal_run_task_context(&goal_run, &task);
        assert!(context.contains("## Goal Big Picture"));
        assert!(context.contains("Ship the release with full verification"));
        assert!(context.contains("Implement, verify, and package the release."));
        assert!(context.contains("1 of 2 steps completed"));
        assert!(context.contains("Review integration notes"));
        assert!(context.contains("Implementation landed"));
    }

    #[test]
    fn memory_paths_for_main_scope_falls_back_to_legacy_root() {
        let root = std::path::Path::new("/tmp/zorai/agent");
        let paths = memory_paths_for_scope(root, MAIN_AGENT_ID);
        let legacy_root = root.parent().unwrap().join("agent-mission");
        assert_eq!(paths.memory_dir, legacy_root);
        assert_eq!(paths.memory_path, legacy_root.join("MEMORY.md"));
        assert_eq!(paths.soul_path, legacy_root.join("SOUL.md"));
        assert_eq!(paths.user_path, legacy_root.join("USER.md"));
    }

    #[test]
    fn memory_paths_for_persona_scope_use_persona_root_and_shared_user() {
        let root = std::path::Path::new("/tmp/zorai/agent");
        let paths = memory_paths_for_scope(root, RADOGOST_AGENT_ID);
        let shared_user_root = active_memory_dir_for_scope(root, MAIN_AGENT_ID);
        assert_eq!(
            paths.memory_dir,
            shared_user_root.join("personas").join(RADOGOST_AGENT_ID)
        );
        assert_eq!(
            paths.memory_path,
            shared_user_root
                .join("personas")
                .join(RADOGOST_AGENT_ID)
                .join("MEMORY.md")
        );
        assert_eq!(
            paths.soul_path,
            shared_user_root
                .join("personas")
                .join(RADOGOST_AGENT_ID)
                .join("SOUL.md")
        );
        assert_eq!(paths.user_path, shared_user_root.join("USER.md"));
    }

    #[test]
    fn seed_builtin_skills_copies_repo_skill_docs() {
        let temp = tempdir().expect("tempdir should succeed");
        let agent_data_dir = temp.path().join("agent");

        seed_builtin_skills(&agent_data_dir);

        let builtin_root = skills_dir(&agent_data_dir);
        assert!(
            builtin_root
                .join("development")
                .join("superpowers")
                .join("systematic-debugging")
                .join("SKILL.md")
                .exists(),
            "expected built-in skills seed to copy nested skill entrypoints"
        );
        assert!(
            builtin_root.join("zorai-mcp").join("README.md").exists(),
            "expected built-in skills seed to copy nested markdown docs"
        );
    }

    #[test]
    fn seed_builtin_guidelines_copies_missing_repo_guidelines_without_overwriting() {
        let temp = tempdir().expect("tempdir should succeed");
        let agent_data_dir = temp.path().join("agent");
        let guidelines_root = guidelines_dir(&agent_data_dir);
        std::fs::create_dir_all(&guidelines_root).expect("create guidelines root");
        std::fs::write(
            guidelines_root.join("coding-task.md"),
            "# Operator Custom Coding Guideline\n",
        )
        .expect("write existing guideline");

        seed_builtin_guidelines(&agent_data_dir);

        assert!(
            guidelines_root.join("research-task.md").exists(),
            "expected daemon guideline seed to copy missing bundled guideline docs"
        );
        assert_eq!(
            std::fs::read_to_string(guidelines_root.join("coding-task.md"))
                .expect("read existing guideline"),
            "# Operator Custom Coding Guideline\n",
            "daemon guideline seed must not overwrite operator customized guidelines"
        );
    }

    #[test]
    fn builtin_skills_source_dir_points_at_repo_skills_tree() {
        let expected = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
        assert_eq!(builtin_skills_source_dir(), expected);
        assert!(
            builtin_skills_source_dir()
                .join("development")
                .join("superpowers")
                .join("systematic-debugging")
                .join("SKILL.md")
                .exists(),
            "expected built-in skills source dir to resolve to the repo skills tree"
        );
    }
}
