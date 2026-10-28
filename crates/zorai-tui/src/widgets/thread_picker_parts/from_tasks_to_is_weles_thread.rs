use super::is_svarog_agent_name_to_hit_test::*;

use ratatui::prelude::*;

use zorai_protocol::{AGENT_NAME_RAROG, AGENT_NAME_SWAROG};

use crate::state::chat::{AgentThread, ChatState};
use crate::state::modal::ThreadPickerTab;
use crate::state::subagents::SubAgentsState;
use crate::state::task::{GoalRunStatus, TaskState, TaskStatus};
use crate::state::workspace::WorkspaceState;

pub(super) const TAB_GAP: u16 = 1;
pub(super) const INTERNAL_DM_THREAD_PREFIX: &str = "dm:";
pub(super) const INTERNAL_DM_TITLE_PREFIX: &str = "Internal DM";
pub(super) const HIDDEN_HANDOFF_THREAD_PREFIX: &str = "handoff:";
pub(super) const GOAL_THREAD_PREFIX: &str = "goal:";
pub(super) const WORKSPACE_THREAD_PREFIX: &str = "workspace-thread:";
pub(super) const PLAYGROUND_THREAD_PREFIX: &str = "playground:";
pub(super) const PLAYGROUND_THREAD_TITLE_PREFIX: &str = "Participant Playground";
pub(super) const WELES_THREAD_TITLE: &str = "WELES";
pub(super) const GATEWAY_THREAD_TITLE_PREFIXES: [&str; 4] =
    ["slack ", "discord ", "telegram ", "whatsapp "];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadPickerHitTarget {
    Tab(ThreadPickerTab),
    Item(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThreadPickerTabSpec {
    pub(crate) tab: ThreadPickerTab,
    pub(crate) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ThreadPickerTabCell {
    pub(super) tab: ThreadPickerTab,
    pub(super) label: String,
    pub(super) start: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThreadPickerStatus {
    Running,
    Paused,
    Stopped,
    Idle,
}

impl ThreadPickerStatus {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
            Self::Idle => "idle",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ThreadPickerStatusIndex {
    by_thread_id: std::collections::HashMap<String, ThreadPickerStatus>,
}

impl ThreadPickerStatusIndex {
    pub(super) fn from_state_for_threads<'a>(
        chat: &ChatState,
        tasks: &TaskState,
        threads: impl IntoIterator<Item = &'a AgentThread>,
    ) -> Self {
        let mut index = Self::from_tasks(tasks);
        for thread in threads {
            if chat.is_thread_streaming(&thread.id) {
                index.set_status(&thread.id, ThreadPickerStatus::Running);
            } else if latest_assistant_message_is_stopped(thread) {
                index.set_status(&thread.id, ThreadPickerStatus::Stopped);
            }
        }
        index
    }

    #[cfg(test)]
    pub(super) fn from_state(chat: &ChatState, tasks: &TaskState) -> Self {
        Self::from_state_for_threads(chat, tasks, chat.threads())
    }

    pub(super) fn status_for(&self, thread: &AgentThread) -> ThreadPickerStatus {
        self.by_thread_id
            .get(&thread.id)
            .copied()
            .unwrap_or_else(|| {
                if latest_assistant_message_is_stopped(thread) {
                    ThreadPickerStatus::Stopped
                } else {
                    ThreadPickerStatus::Idle
                }
            })
    }

    pub(super) fn from_tasks(tasks: &TaskState) -> Self {
        let mut index = Self::default();

        for goal_run in tasks.goal_runs() {
            let goal_status = status_from_goal_run(goal_run.status);
            for thread_id in goal_run
                .thread_id
                .iter()
                .chain(goal_run.root_thread_id.iter())
                .chain(goal_run.active_thread_id.iter())
            {
                index.set_status(thread_id, goal_status);
            }
            for thread_id in &goal_run.execution_thread_ids {
                index.set_status(thread_id, goal_status);
            }
        }

        for task in tasks.tasks() {
            if let Some(thread_id) = task.thread_id.as_deref() {
                index.set_status(thread_id, status_from_task(task.status));
            }
        }

        index
    }

    fn set_status(&mut self, thread_id: &str, status: ThreadPickerStatus) {
        if thread_id.is_empty() || status == ThreadPickerStatus::Idle {
            return;
        }
        let entry = self
            .by_thread_id
            .entry(thread_id.to_string())
            .or_insert(ThreadPickerStatus::Idle);
        if status_precedence(status) > status_precedence(*entry) {
            *entry = status;
        }
    }
}

pub(super) fn status_from_goal_run(status: Option<GoalRunStatus>) -> ThreadPickerStatus {
    match status {
        Some(
            GoalRunStatus::Queued
            | GoalRunStatus::Planning
            | GoalRunStatus::Running
            | GoalRunStatus::AwaitingApproval
            | GoalRunStatus::AwaitingReview,
        ) => ThreadPickerStatus::Running,
        Some(GoalRunStatus::Paused | GoalRunStatus::Blocked) => ThreadPickerStatus::Paused,
        Some(
            GoalRunStatus::Completed
            | GoalRunStatus::Failed
            | GoalRunStatus::Cancelled
            | GoalRunStatus::Contained
            | GoalRunStatus::Compensated
            | GoalRunStatus::PartiallyCompensated
            | GoalRunStatus::BreakGlass,
        ) => ThreadPickerStatus::Stopped,
        None => ThreadPickerStatus::Idle,
    }
}

pub(super) fn status_from_task(status: Option<TaskStatus>) -> ThreadPickerStatus {
    match status {
        Some(TaskStatus::Queued | TaskStatus::InProgress | TaskStatus::FailedAnalyzing) => {
            ThreadPickerStatus::Running
        }
        Some(TaskStatus::AwaitingApproval | TaskStatus::Blocked) => ThreadPickerStatus::Paused,
        Some(
            TaskStatus::BudgetExceeded
            | TaskStatus::Completed
            | TaskStatus::Failed
            | TaskStatus::Cancelled,
        ) => ThreadPickerStatus::Stopped,
        None => ThreadPickerStatus::Idle,
    }
}

pub(super) fn status_precedence(status: ThreadPickerStatus) -> u8 {
    match status {
        ThreadPickerStatus::Idle => 0,
        ThreadPickerStatus::Stopped => 1,
        ThreadPickerStatus::Paused => 2,
        ThreadPickerStatus::Running => 3,
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct GoalThreadIndex {
    ids: std::collections::BTreeSet<String>,
}

impl GoalThreadIndex {
    pub(super) fn from_tasks(tasks: &TaskState) -> Self {
        let ids = tasks.all_goal_thread_ids().into_iter().collect();
        Self { ids }
    }

    pub(super) fn contains_id(&self, thread_id: &str) -> bool {
        self.ids.contains(thread_id)
    }

    fn contains_thread(&self, thread: &AgentThread) -> bool {
        is_goal_thread(thread) || self.contains_id(&thread.id)
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct WorkspaceThreadIndex {
    ids: std::collections::BTreeSet<String>,
}

impl WorkspaceThreadIndex {
    pub(super) fn from_workspace(workspace: &WorkspaceState, tasks: &TaskState) -> Self {
        let mut ids = workspace
            .all_runtime_thread_ids()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        for goal_run_id in workspace.all_runtime_goal_run_ids() {
            if let Some(run) = tasks.goal_run_by_id(&goal_run_id) {
                for thread_id in run
                    .active_thread_id
                    .iter()
                    .chain(run.root_thread_id.iter())
                    .chain(run.thread_id.iter())
                {
                    if !thread_id.is_empty() {
                        ids.insert(thread_id.clone());
                    }
                }
                for thread_id in &run.execution_thread_ids {
                    if !thread_id.is_empty() {
                        ids.insert(thread_id.clone());
                    }
                }
            }
            ids.extend(tasks.goal_thread_ids(&goal_run_id));
        }
        Self { ids }
    }

    pub(super) fn contains_id(&self, thread_id: &str) -> bool {
        self.ids.contains(thread_id)
    }

    fn contains_thread(&self, thread: &AgentThread) -> bool {
        is_workspace_thread(thread) || self.contains_id(&thread.id)
    }
}

pub(super) const THREAD_PICKER_SIDEBAR_WIDTH: u16 = 24;
pub(super) const THREAD_PICKER_SIDEBAR_MIN_INNER_WIDTH: u16 = 76;

pub(super) struct ThreadPickerLayout {
    pub(super) sidebar: Option<Rect>,
    pub(super) divider: Option<Rect>,
    pub(super) tabs: Option<Rect>,
    pub(super) search: Rect,
    pub(super) separator: Rect,
    pub(super) list: Rect,
    pub(super) hints: Rect,
}

pub(super) fn thread_picker_layout(inner: Rect) -> ThreadPickerLayout {
    if inner.width >= THREAD_PICKER_SIDEBAR_MIN_INNER_WIDTH {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(THREAD_PICKER_SIDEBAR_WIDTH),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(inner);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(columns[2]);
        return ThreadPickerLayout {
            sidebar: Some(columns[0]),
            divider: Some(columns[1]),
            tabs: None,
            search: right[0],
            separator: right[1],
            list: right[2],
            hints: right[3],
        };
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    ThreadPickerLayout {
        sidebar: None,
        divider: None,
        tabs: Some(chunks[0]),
        search: chunks[1],
        separator: chunks[2],
        list: chunks[3],
        hints: chunks[4],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ThreadPickerSidebarRow {
    Blank,
    Group(&'static str),
    Tab(ThreadPickerTab, String),
}

fn sidebar_group(tab: &ThreadPickerTab) -> &'static str {
    match tab {
        ThreadPickerTab::Swarog | ThreadPickerTab::Rarog | ThreadPickerTab::Weles => "AGENTS",
        ThreadPickerTab::Agent(_) => "SUB-AGENTS",
        ThreadPickerTab::Goals
        | ThreadPickerTab::Workspace
        | ThreadPickerTab::Playgrounds
        | ThreadPickerTab::Internal
        | ThreadPickerTab::Gateway => "SOURCES",
    }
}

pub(super) fn sidebar_rows(specs: Vec<ThreadPickerTabSpec>) -> Vec<ThreadPickerSidebarRow> {
    let mut rows = Vec::new();
    let mut current_group = None;
    for spec in specs {
        let group = sidebar_group(&spec.tab);
        if current_group != Some(group) {
            if current_group.is_some() {
                rows.push(ThreadPickerSidebarRow::Blank);
            }
            rows.push(ThreadPickerSidebarRow::Group(group));
            current_group = Some(group);
        }
        let label = spec
            .label
            .strip_prefix('[')
            .and_then(|label| label.strip_suffix(']'))
            .unwrap_or(&spec.label)
            .to_string();
        rows.push(ThreadPickerSidebarRow::Tab(spec.tab, label));
    }
    rows
}

pub(super) fn sidebar_scroll(
    height: u16,
    rows: &[ThreadPickerSidebarRow],
    selected: &ThreadPickerTab,
) -> usize {
    let height = height as usize;
    if height == 0 || rows.len() <= height {
        return 0;
    }
    let selected_row = rows
        .iter()
        .position(|row| matches!(row, ThreadPickerSidebarRow::Tab(tab, _) if tab == selected))
        .unwrap_or(0);
    selected_row
        .saturating_sub(height.saturating_sub(1))
        .min(rows.len() - height)
}

pub(super) fn fixed_tab_specs() -> Vec<ThreadPickerTabSpec> {
    vec![
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Swarog,
            label: format!(
                "[{}]",
                crate::agent_aliases::display_agent_name(AGENT_NAME_SWAROG)
            ),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Rarog,
            label: format!(
                "[{}]",
                crate::agent_aliases::display_agent_name(AGENT_NAME_RAROG)
            ),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Weles,
            label: format!("[{}]", crate::agent_aliases::display_agent_name("Weles")),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Goals,
            label: "[Goals]".to_string(),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Workspace,
            label: "[Workspace]".to_string(),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Playgrounds,
            label: "[Playgrounds]".to_string(),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Internal,
            label: "[Internal]".to_string(),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Gateway,
            label: "[Gateway]".to_string(),
        },
    ]
}

pub(super) fn latest_assistant_message_is_stopped(thread: &AgentThread) -> bool {
    thread
        .messages
        .iter()
        .rev()
        .find(|message| message.role == crate::state::chat::MessageRole::Assistant)
        .is_some_and(|message| message.content.trim_end().ends_with("[stopped]"))
}

pub(super) fn normalize_agent_tab_id(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "svarog" | "swarog" | "main" | "rarog" | "concierge" | "weles" => None,
        other => Some(other.to_string()),
    }
}

pub(super) fn display_name_for_agent_id(
    agent_id: &str,
    chat: &ChatState,
    subagents: &SubAgentsState,
) -> String {
    if let Some(entry) = subagents.entries.iter().find(|entry| {
        entry.id.eq_ignore_ascii_case(agent_id)
            || entry
                .id
                .strip_suffix("_builtin")
                .is_some_and(|alias| alias.eq_ignore_ascii_case(agent_id))
    }) {
        return entry.name.clone();
    }

    chat.threads()
        .iter()
        .filter_map(|thread| thread.agent_name.as_deref())
        .find(|name| name.eq_ignore_ascii_case(agent_id))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let mut chars = agent_id.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => agent_id.to_string(),
            }
        })
}

pub(super) fn tab_specs_inner(
    chat: &ChatState,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
    workspace_index: Option<&WorkspaceThreadIndex>,
) -> Vec<ThreadPickerTabSpec> {
    let mut specs = fixed_tab_specs();
    let mut dynamic_agents = std::collections::BTreeSet::new();

    for entry in &subagents.entries {
        if !entry.builtin {
            if let Some(agent_id) = normalize_agent_tab_id(&entry.id) {
                dynamic_agents.insert(agent_id);
            }
        }
    }

    for thread in chat.threads() {
        if is_hidden_handoff_thread(thread)
            || is_internal_thread(thread)
            || is_gateway_thread(thread, chat)
            || is_workspace_thread_with_index(thread, workspace_index)
            || is_goal_thread_with_index(thread, goal_index)
            || is_playground_thread(thread)
            || is_rarog_thread(thread)
            || is_weles_thread(thread)
        {
            continue;
        }

        if let Some(agent_name) = thread.agent_name.as_deref() {
            if let Some(agent_id) = normalize_agent_tab_id(agent_name) {
                dynamic_agents.insert(agent_id);
            }
        }
    }

    specs.extend(
        dynamic_agents
            .into_iter()
            .map(|agent_id| ThreadPickerTabSpec {
                label: format!(
                    "[{}]",
                    display_name_for_agent_id(&agent_id, chat, subagents)
                ),
                tab: ThreadPickerTab::Agent(agent_id),
            }),
    );
    specs
}

#[cfg(test)]
pub(crate) fn tab_specs(chat: &ChatState, subagents: &SubAgentsState) -> Vec<ThreadPickerTabSpec> {
    tab_specs_inner(chat, subagents, None, None)
}

#[cfg(test)]
pub(crate) fn tab_specs_for_tasks(
    chat: &ChatState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
) -> Vec<ThreadPickerTabSpec> {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    tab_specs_inner(chat, subagents, Some(&goal_index), None)
}

#[cfg(test)]
pub(crate) fn tab_specs_for_workspace(
    chat: &ChatState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
    workspace: &WorkspaceState,
) -> Vec<ThreadPickerTabSpec> {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    let workspace_index = WorkspaceThreadIndex::from_workspace(workspace, tasks);
    tab_specs_inner(chat, subagents, Some(&goal_index), Some(&workspace_index))
}

pub(super) fn thread_matches_agent_tab(
    thread: &AgentThread,
    chat: &ChatState,
    agent_id: &str,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
    workspace_index: Option<&WorkspaceThreadIndex>,
) -> bool {
    if is_hidden_handoff_thread(thread)
        || is_internal_thread(thread)
        || is_gateway_thread(thread, chat)
        || is_workspace_thread_with_index(thread, workspace_index)
        || is_goal_thread_with_index(thread, goal_index)
        || is_playground_thread(thread)
        || is_rarog_thread(thread)
        || is_weles_thread(thread)
    {
        return false;
    }

    let normalized_agent = agent_id.trim().to_ascii_lowercase();
    thread
        .agent_name
        .as_deref()
        .and_then(normalize_agent_tab_id)
        .is_some_and(|thread_agent| thread_agent == normalized_agent)
        || subagents.entries.iter().any(|entry| {
            normalize_agent_tab_id(&entry.id).is_some_and(|entry_id| entry_id == normalized_agent)
                && thread
                    .agent_name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&entry.name))
        })
}

pub(crate) fn resolve_thread_picker_tab(
    agent_alias: &str,
    chat: &ChatState,
    subagents: &SubAgentsState,
) -> Option<ThreadPickerTab> {
    let normalized = agent_alias.trim().to_ascii_lowercase();
    let fixed = match normalized.as_str() {
        "svarog" | "swarog" | "main" => Some(ThreadPickerTab::Swarog),
        "rarog" | "concierge" => Some(ThreadPickerTab::Rarog),
        "weles" => Some(ThreadPickerTab::Weles),
        "goals" | "goal" => Some(ThreadPickerTab::Goals),
        "workspace" | "workspaces" => Some(ThreadPickerTab::Workspace),
        "playgrounds" | "playground" => Some(ThreadPickerTab::Playgrounds),
        "internal" => Some(ThreadPickerTab::Internal),
        "gateway" => Some(ThreadPickerTab::Gateway),
        _ => None,
    };
    if fixed.is_some() {
        return fixed;
    }

    if let Some(entry) = subagents.entries.iter().find(|entry| {
        entry.id.eq_ignore_ascii_case(agent_alias)
            || entry.name.eq_ignore_ascii_case(agent_alias)
            || entry
                .id
                .strip_suffix("_builtin")
                .is_some_and(|alias| alias.eq_ignore_ascii_case(agent_alias))
    }) {
        if let Some(agent_id) =
            normalize_agent_tab_id(entry.id.strip_suffix("_builtin").unwrap_or(&entry.id))
        {
            return Some(ThreadPickerTab::Agent(agent_id));
        }
    }

    chat.threads()
        .iter()
        .filter_map(|thread| thread.agent_name.as_deref())
        .find(|name| name.eq_ignore_ascii_case(agent_alias))
        .and_then(normalize_agent_tab_id)
        .map(ThreadPickerTab::Agent)
}

pub(crate) fn adjacent_thread_picker_tab_for_workspace(
    current: &ThreadPickerTab,
    chat: &ChatState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
    workspace: &WorkspaceState,
    direction: i32,
) -> ThreadPickerTab {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    let workspace_index = WorkspaceThreadIndex::from_workspace(workspace, tasks);
    adjacent_thread_picker_tab_inner(
        current,
        chat,
        subagents,
        Some(&goal_index),
        Some(&workspace_index),
        direction,
    )
}

pub(super) fn adjacent_thread_picker_tab_inner(
    current: &ThreadPickerTab,
    chat: &ChatState,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
    workspace_index: Option<&WorkspaceThreadIndex>,
    direction: i32,
) -> ThreadPickerTab {
    let specs = tab_specs_inner(chat, subagents, goal_index, workspace_index);
    let current_index = specs
        .iter()
        .position(|spec| &spec.tab == current)
        .unwrap_or(0);
    let next_index = if direction < 0 {
        current_index
            .checked_sub(1)
            .unwrap_or(specs.len().saturating_sub(1))
    } else {
        (current_index + 1) % specs.len().max(1)
    };
    specs
        .get(next_index)
        .map(|spec| spec.tab.clone())
        .unwrap_or_default()
}

pub(super) fn thread_matches_query(
    thread: &AgentThread,
    query: &str,
    goal_index: Option<&GoalThreadIndex>,
    workspace_index: Option<&WorkspaceThreadIndex>,
) -> bool {
    if query.is_empty() {
        return true;
    }
    let lower = query.to_lowercase();
    thread.title.to_lowercase().contains(&lower)
        || thread_display_title_inner(thread, goal_index, workspace_index)
            .to_lowercase()
            .contains(&lower)
        || thread
            .agent_name
            .as_deref()
            .is_some_and(|name| name.to_lowercase().contains(&lower))
}

pub(crate) fn is_rarog_thread(thread: &AgentThread) -> bool {
    thread.id == "concierge"
        || thread
            .agent_name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(AGENT_NAME_RAROG))
        || thread.title.eq_ignore_ascii_case("concierge")
        || thread.title.starts_with("HEARTBEAT SYNTHESIS")
        || thread.title.starts_with("Heartbeat check:")
}

pub(crate) fn is_internal_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(INTERNAL_DM_THREAD_PREFIX)
        || thread.title.starts_with(INTERNAL_DM_TITLE_PREFIX)
}

pub(crate) fn is_gateway_thread(thread: &AgentThread, chat: &ChatState) -> bool {
    !is_internal_thread(thread)
        && (chat.is_gateway_linked_thread(&thread.id)
            || GATEWAY_THREAD_TITLE_PREFIXES
                .iter()
                .any(|prefix| thread.title.trim().to_ascii_lowercase().starts_with(prefix)))
}

pub(crate) fn is_playground_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(PLAYGROUND_THREAD_PREFIX)
        || thread.title.starts_with(PLAYGROUND_THREAD_TITLE_PREFIX)
}

pub(crate) fn is_goal_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(GOAL_THREAD_PREFIX)
}

pub(super) fn is_goal_thread_with_index(
    thread: &AgentThread,
    goal_index: Option<&GoalThreadIndex>,
) -> bool {
    is_goal_thread(thread) || goal_index.is_some_and(|index| index.contains_thread(thread))
}

pub(super) fn is_workspace_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(WORKSPACE_THREAD_PREFIX)
}

pub(super) fn is_workspace_thread_with_index(
    thread: &AgentThread,
    workspace_index: Option<&WorkspaceThreadIndex>,
) -> bool {
    is_workspace_thread(thread)
        || workspace_index.is_some_and(|index| index.contains_thread(thread))
}

pub(super) fn is_hidden_handoff_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(HIDDEN_HANDOFF_THREAD_PREFIX)
        || thread
            .title
            .trim()
            .to_ascii_lowercase()
            .starts_with("handoff ")
}

pub(crate) fn is_weles_thread(thread: &AgentThread) -> bool {
    !is_internal_thread(thread)
        && (thread
            .agent_name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("weles"))
            || thread.title.contains(WELES_THREAD_TITLE)
            || thread.messages.iter().any(|message| {
                message.content.lines().any(|line| {
                    let Some((marker, value)) = line.split_once(':') else {
                        return false;
                    };
                    if marker.trim() != "Agent persona id" {
                        return false;
                    }
                    matches!(
                        value.trim().to_ascii_lowercase().as_str(),
                        "weles" | "governance" | "vitality"
                    )
                })
            }))
}
