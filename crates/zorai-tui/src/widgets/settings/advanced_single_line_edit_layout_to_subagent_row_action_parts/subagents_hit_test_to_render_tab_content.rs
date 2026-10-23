use super::super::advanced_single_line_edit_layout_to_subagent_row_action_offsets::*;
use super::super::render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content::*;
use super::super::wrap_textarea_visual_line_to_render_wrapped_textarea_buffer_to_render::*;
use crate::state::concierge::ConciergeState;
use crate::state::config::ConfigState;
use crate::state::modal::ModalState;
use crate::state::settings::{PluginSettingsState, SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;
use crate::widgets::settings::render_mlflow_tab;
use ratatui::prelude::*;
use ratatui::text::Line;
pub(crate) fn subagents_hit_test(
    content_area: Rect,
    subagents: &SubAgentsState,
    scroll: usize,
    mouse: Position,
) -> Option<SettingsHitTarget> {
    let row = mouse.y.saturating_sub(content_area.y) as usize + scroll;
    let list_len = subagents.entries.len();
    if list_len > 0 && (4..4 + list_len).contains(&row) {
        let index = row - 4;
        if let Some(entry) = subagents.entries.get(index) {
            let (edit_start, delete_start, toggle_start, _, toggle_end) =
                subagent_row_action_offsets(content_area, entry);
            if mouse.x >= edit_start && mouse.x < delete_start.saturating_sub(1) {
                return Some(SettingsHitTarget::SubAgentRowAction {
                    index,
                    action: SubAgentTabAction::Edit,
                });
            }
            if mouse.x >= delete_start && mouse.x < toggle_start.saturating_sub(1) {
                return Some(SettingsHitTarget::SubAgentRowAction {
                    index,
                    action: SubAgentTabAction::Delete,
                });
            }
            if mouse.x >= toggle_start && mouse.x < toggle_end {
                return Some(SettingsHitTarget::SubAgentRowAction {
                    index,
                    action: SubAgentTabAction::Toggle,
                });
            }
        }
        return Some(SettingsHitTarget::SubAgentListItem(index));
    }
    match row {
        r if r == 5 + list_len => Some(SettingsHitTarget::SubAgentAction(SubAgentTabAction::Add)),
        _ => None,
    }
}

pub(crate) fn render_tab_content<'a>(
    content_width: u16,
    settings: &'a SettingsState,
    config: &'a ConfigState,
    modal: &'a ModalState,
    auth: &'a crate::state::auth::AuthState,
    subagents: &'a SubAgentsState,
    concierge: &'a ConciergeState,
    plugin_settings: &PluginSettingsState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    if let Some(page) = crate::state::settings_registry::settings_page_spec(settings.active_tab()) {
        let mut lines = render_settings_page(page, settings, config, theme);
        if settings.active_tab() == SettingsTab::Memory {
            lines.extend(render_honcho_editor_block(settings, config, theme));
        }
        return lines;
    }
    match settings.active_tab() {
        SettingsTab::Provider => render_provider_tab(settings, config, theme),
        SettingsTab::Tools
        | SettingsTab::Skills
        | SettingsTab::WebSearch
        | SettingsTab::Media
        | SettingsTab::Memory
        | SettingsTab::Heartbeat
        | SettingsTab::Proactivity
        | SettingsTab::Workflows
        | SettingsTab::Chat
        | SettingsTab::Compaction
        | SettingsTab::Execution
        | SettingsTab::Safety
        | SettingsTab::Storage => Vec::new(),
        SettingsTab::Mlflow => render_mlflow_tab(settings, config, theme),
        SettingsTab::Gateway => render_gateway_tab(settings, config, modal, theme),
        SettingsTab::Auth => render_auth_tab(content_width, auth, config, theme),
        SettingsTab::Agent => render_agent_tab(settings, config, theme),
        SettingsTab::SubAgents => render_subagents_tab(content_width, settings, subagents, theme),
        SettingsTab::Concierge => render_concierge_tab(settings, concierge, theme),
        SettingsTab::Plugins => render_plugins_tab(settings, plugin_settings, content_width, theme),
        SettingsTab::Database => render_database_tab(settings, config, theme),
        SettingsTab::About => render_about_tab(theme),
    }
}
