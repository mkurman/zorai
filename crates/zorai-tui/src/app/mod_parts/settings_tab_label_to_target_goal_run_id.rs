use super::*;
pub(crate) fn settings_tab_label(tab: SettingsTab) -> &'static str {
    match tab {
        SettingsTab::Provider => "provider",
        SettingsTab::Tools => "tools",
        SettingsTab::WebSearch => zorai_protocol::tool_names::WEB_SEARCH,
        SettingsTab::Chat => "chat",
        SettingsTab::Mlflow => "mlflow",
        SettingsTab::Gateway => "gateway",
        SettingsTab::Auth => "auth",
        SettingsTab::Agent => "agent",
        SettingsTab::SubAgents => "subagents",
        SettingsTab::Concierge => "concierge",
        SettingsTab::Skills => "skills",
        SettingsTab::Media => "media",
        SettingsTab::Memory => "memory",
        SettingsTab::Heartbeat => "heartbeat",
        SettingsTab::Proactivity => "proactivity",
        SettingsTab::Workflows => "workflows",
        SettingsTab::Compaction => "compaction",
        SettingsTab::Execution => "execution",
        SettingsTab::Safety => "safety",
        SettingsTab::Storage => "storage",
        SettingsTab::Plugins => "plugins",
        SettingsTab::Database => "database",
        SettingsTab::About => "about",
    }
}

pub(crate) fn sidebar_tab_label(tab: SidebarTab) -> &'static str {
    match tab {
        SidebarTab::Files => "files",
        SidebarTab::Todos => "todos",
        SidebarTab::Spawned => "spawned",
        SidebarTab::Pinned => "pinned",
    }
}

pub(crate) fn target_goal_run_id(model: &TuiModel, target: &SidebarItemTarget) -> Option<String> {
    match target {
        SidebarItemTarget::GoalRun { goal_run_id, .. } => Some(goal_run_id.clone()),
        SidebarItemTarget::Task { task_id } => model
            .tasks
            .task_by_id(task_id)
            .and_then(|task| task.goal_run_id.clone()),
    }
}
