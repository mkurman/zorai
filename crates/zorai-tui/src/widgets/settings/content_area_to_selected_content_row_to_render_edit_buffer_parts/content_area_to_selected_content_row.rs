use super::super::advanced_single_line_edit_layout_to_subagent_row_action_offsets::*;
use super::render_edit_buffer_with_cursor_to_editing_cursor_hit_test::settings_layout;
use crate::state::concierge::ConciergeState;
use crate::state::config::ConfigState;
use crate::state::modal::ModalState;
use crate::state::settings::{PluginSettingsState, SettingsState};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;
use ratatui::prelude::*;
use ratatui::text::Line;
pub(crate) fn content_area(area: Rect) -> Option<Rect> {
    settings_layout(area).map(|layout| layout.content)
}

pub(crate) fn max_scroll(
    area: Rect,
    settings: &SettingsState,
    config: &ConfigState,
    modal: &ModalState,
    auth: &crate::state::auth::AuthState,
    subagents: &SubAgentsState,
    concierge: &ConciergeState,
    plugin_settings: &PluginSettingsState,
    theme: &ThemeTokens,
) -> usize {
    let Some(content_area) = content_area(area) else {
        return 0;
    };
    let line_count = render_tab_content(
        content_area.width,
        settings,
        config,
        modal,
        auth,
        subagents,
        concierge,
        plugin_settings,
        theme,
    )
    .len();
    line_count.saturating_sub(content_area.height as usize)
}

pub(crate) fn scroll_for_selected_field(
    area: Rect,
    settings: &SettingsState,
    config: &ConfigState,
    modal: &ModalState,
    auth: &crate::state::auth::AuthState,
    subagents: &SubAgentsState,
    concierge: &ConciergeState,
    plugin_settings: &PluginSettingsState,
    current_scroll: usize,
    theme: &ThemeTokens,
) -> usize {
    let Some(content_area) = content_area(area) else {
        return 0;
    };
    let content_lines = render_tab_content(
        content_area.width,
        settings,
        config,
        modal,
        auth,
        subagents,
        concierge,
        plugin_settings,
        theme,
    );
    let line_count = content_lines.len();
    let max_scroll = line_count.saturating_sub(content_area.height as usize);
    let Some(selected_row) = selected_content_row(&content_lines) else {
        return current_scroll.min(max_scroll);
    };

    let mut scroll = current_scroll.min(max_scroll);
    let viewport_height = content_area.height as usize;
    if selected_row < scroll {
        scroll = selected_row;
    } else if selected_row >= scroll.saturating_add(viewport_height) {
        scroll = selected_row
            .saturating_add(1)
            .saturating_sub(viewport_height);
    }
    scroll.min(max_scroll)
}

pub(crate) fn selected_content_row(lines: &[Line<'_>]) -> Option<usize> {
    lines.iter().position(|line| {
        let text = line.to_string();
        let trimmed = text.trim_start_matches(' ');
        let indent = text.len().saturating_sub(trimmed.len());
        indent <= 4 && trimmed.starts_with("> ")
    })
}
