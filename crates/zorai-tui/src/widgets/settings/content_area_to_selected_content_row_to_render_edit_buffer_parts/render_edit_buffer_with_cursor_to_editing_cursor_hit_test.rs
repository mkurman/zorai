use super::super::advanced_single_line_edit_layout_to_subagent_row_action_offsets::*;
use crate::state::concierge::ConciergeState;
use crate::state::config::ConfigState;
use crate::state::modal::ModalState;
use crate::state::settings::{PluginSettingsState, SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHitTarget {
    Tab(SettingsTab),
    Field(usize),
    AuthProviderItem(usize),
    AuthAction {
        index: usize,
        action: AuthTabAction,
    },
    SubAgentListItem(usize),
    SubAgentAction(SubAgentTabAction),
    SubAgentRowAction {
        index: usize,
        action: SubAgentTabAction,
    },
    EditCursor {
        line: usize,
        col: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthTabAction {
    Primary,
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubAgentTabAction {
    Add,
    Edit,
    Delete,
    Toggle,
}

pub(crate) const TAB_LABELS: [&str; 22] = [
    "Auth", "Svar", "Rar", "Sub", "Tools", "Skill", "Web", "Media", "Mem", "Beat", "Pro", "Flow",
    "GW", "Chat", "Cmp", "Exec", "Safe", "ML", "Store", "Plug", "DB", "About",
];
pub(crate) const TAB_DIVIDER: &str = " | ";

#[derive(Debug, Clone, Copy)]
pub(crate) struct VisibleTab {
    pub(crate) tab: SettingsTab,
    pub(crate) index: usize,
    pub(crate) start_x: u16,
    pub(crate) end_x: u16,
}

pub(crate) fn render_edit_buffer_with_cursor(text: &str, cursor: usize) -> String {
    let cursor = cursor.min(text.chars().count());
    let mut out = String::with_capacity(text.len() + 3);
    let byte_cursor = text
        .char_indices()
        .nth(cursor)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len());
    out.push_str(&text[..byte_cursor]);
    out.push('\u{2588}');
    out.push_str(&text[byte_cursor..]);
    out
}

pub(crate) fn render_edit_line_with_cursor(text: &str, cursor_col: usize) -> String {
    let mut out = String::with_capacity(text.len() + 3);
    let mut inserted = false;
    for (col, ch) in text.chars().enumerate() {
        if col == cursor_col {
            out.push('\u{2588}');
            inserted = true;
        }
        out.push(ch);
    }
    if !inserted {
        out.push('\u{2588}');
    }
    out
}

pub(crate) fn clip_inline_text(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    let tail: String = chars[chars.len().saturating_sub(max_chars)..]
        .iter()
        .collect();
    format!("…{}", tail)
}

pub(crate) fn render(
    frame: &mut Frame,
    area: Rect,
    settings: &SettingsState,
    config: &ConfigState,
    modal: &ModalState,
    auth: &crate::state::auth::AuthState,
    subagents: &SubAgentsState,
    concierge: &ConciergeState,
    plugin_settings: &PluginSettingsState,
    scroll: usize,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" SETTINGS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    frame.render_widget(block, area);
    let Some(layout) = settings_layout(area) else {
        return;
    };

    let active = settings.active_tab();
    if let (Some(sidebar), Some(divider)) = (layout.sidebar, layout.divider) {
        render_settings_sidebar(frame, sidebar, divider, active, theme);
    }
    if let (Some(tabs_line), Some(separator)) = (layout.tabs_line, layout.separator) {
        let tabs = visible_tabs(tabs_line, active_tab_index(active));
        frame.render_widget(
            Paragraph::new(render_tabs_line(&tabs, settings, theme)),
            tabs_line,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "\u{2500}".repeat(separator.width as usize),
                theme.fg_dim,
            ))),
            separator,
        );
    }

    let content_lines = render_tab_content(
        layout.content.width,
        settings,
        config,
        modal,
        auth,
        subagents,
        concierge,
        plugin_settings,
        theme,
    );
    let paragraph = Paragraph::new(content_lines).scroll((scroll.min(u16::MAX as usize) as u16, 0));
    frame.render_widget(paragraph, layout.content);

    let hints = if settings.is_editing() {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Enter", theme.fg_active),
            Span::styled(" confirm  ", theme.fg_dim),
            Span::styled("Esc", theme.fg_active),
            Span::styled(" cancel", theme.fg_dim),
        ])
    } else {
        let mut spans = vec![
            Span::raw(" "),
            Span::styled("↑↓", theme.fg_active),
            Span::styled(" navigate  ", theme.fg_dim),
        ];
        if settings_field_can_activate(settings, config) {
            spans.push(Span::styled("Enter", theme.fg_active));
            spans.push(Span::styled(" edit/select  ", theme.fg_dim));
        }
        if settings_field_can_toggle(settings, config) {
            spans.push(Span::styled("Space", theme.fg_active));
            spans.push(Span::styled(" toggle  ", theme.fg_dim));
        }
        spans.extend([
            Span::styled("Tab", theme.fg_active),
            Span::styled(" switch tab  ", theme.fg_dim),
            Span::styled("Esc", theme.fg_active),
            Span::styled(" close", theme.fg_dim),
        ]);
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(hints), layout.hints);
}

pub(crate) fn settings_field_can_activate(settings: &SettingsState, config: &ConfigState) -> bool {
    let field = settings.current_field_name_with_config(config);
    if field.is_empty() || matches!(field, "snapshot_stats") {
        return false;
    }
    !matches!(
        field,
        "managed_sandbox_enabled"
            | "gateway_enabled"
            | "web_search_enabled"
            | "enable_streaming"
            | "enable_conversation_memory"
            | "anticipatory_enabled"
            | "anticipatory_morning_brief"
            | "anticipatory_predictive_hydration"
            | "anticipatory_stuck_detection"
            | "operator_model_enabled"
            | "operator_model_allow_message_statistics"
            | "operator_model_allow_approval_learning"
            | "operator_model_allow_attention_tracking"
            | "operator_model_allow_implicit_feedback"
            | "collaboration_enabled"
            | "compliance_sign_all_events"
            | "tool_synthesis_enabled"
            | "tool_synthesis_require_activation"
            | "auto_compact_context"
            | "auto_retry"
            | "snapshot_auto_cleanup"
            | "workspace_repo_monitor_enabled"
            | "feat_check_stale_todos"
            | "feat_check_stuck_goals"
            | "feat_check_unreplied_messages"
            | "feat_check_repo_changes"
            | "feat_check_plugin_auth"
            | "feat_heartbeat_enabled"
            | "desktop_notifications"
            | "feat_audio_tts_auto_speak"
            | "feat_consolidation_enabled"
            | "feat_skill_recommendation_enabled"
            | "feat_skill_background_community_search"
            | "feat_audio_stt_enabled"
            | "feat_audio_tts_enabled"
            | "feat_embedding_enabled"
    ) && !crate::state::settings::is_tool_toggle_field(field)
}

pub(crate) fn settings_field_can_toggle(settings: &SettingsState, config: &ConfigState) -> bool {
    let field = settings.current_field_name_with_config(config);
    matches!(
        field,
        "managed_sandbox_enabled"
            | "managed_security_level"
            | "gateway_enabled"
            | "web_search_enabled"
            | "enable_streaming"
            | "enable_conversation_memory"
            | "enable_honcho_memory"
            | "anticipatory_enabled"
            | "anticipatory_morning_brief"
            | "anticipatory_predictive_hydration"
            | "anticipatory_stuck_detection"
            | "operator_model_enabled"
            | "operator_model_allow_message_statistics"
            | "operator_model_allow_approval_learning"
            | "operator_model_allow_attention_tracking"
            | "operator_model_allow_implicit_feedback"
            | "collaboration_enabled"
            | "compliance_sign_all_events"
            | "tool_synthesis_enabled"
            | "tool_synthesis_require_activation"
            | "auto_compact_context"
            | "compaction_strategy"
            | "compaction_weles_provider"
            | "compaction_weles_reasoning_effort"
            | "compaction_weles_api_transport"
            | "compaction_custom_provider"
            | "compaction_custom_auth_source"
            | "compaction_custom_api_transport"
            | "compaction_custom_reasoning_effort"
            | "auto_retry"
            | "snapshot_auto_cleanup"
            | "workspace_repo_monitor_enabled"
            | "feat_tier_override"
            | "feat_check_stale_todos"
            | "feat_check_stuck_goals"
            | "feat_check_unreplied_messages"
            | "feat_check_repo_changes"
            | "feat_check_plugin_auth"
            | "feat_heartbeat_enabled"
            | "desktop_notifications"
            | "feat_audio_tts_auto_speak"
            | "feat_consolidation_enabled"
            | "feat_skill_recommendation_enabled"
            | "feat_skill_background_community_search"
            | "feat_audio_stt_enabled"
            | "feat_audio_tts_enabled"
            | "feat_embedding_enabled"
            | "feat_auto_thread_title"
            | "whatsapp_link_device"
            | "whatsapp_relink_device"
    ) || crate::state::settings::is_tool_toggle_field(field)
}

pub(crate) fn hit_test(
    area: Rect,
    settings: &SettingsState,
    config: &ConfigState,
    auth: &crate::state::auth::AuthState,
    subagents: &SubAgentsState,
    scroll: usize,
    mouse: Position,
) -> Option<SettingsHitTarget> {
    let layout = settings_layout(area)?;
    if let Some(sidebar) = layout.sidebar {
        if sidebar.contains(mouse) {
            return sidebar_hit_test(sidebar, settings.active_tab(), mouse)
                .map(SettingsHitTarget::Tab);
        }
    }
    if let Some(tabs_line) = layout.tabs_line {
        if mouse.y == tabs_line.y && tabs_line.contains(mouse) {
            return tab_hit_test(tabs_line, settings.active_tab(), mouse.x)
                .map(SettingsHitTarget::Tab);
        }
    }
    if !layout.content.contains(mouse) {
        return None;
    }
    let content = layout.content;

    if let Some((line, col)) = editing_cursor_hit_test(content, settings, config, scroll, mouse) {
        return Some(SettingsHitTarget::EditCursor { line, col });
    }

    if matches!(settings.active_tab(), SettingsTab::Auth) {
        return auth_hit_test(content, auth, scroll, mouse);
    }

    if matches!(settings.active_tab(), SettingsTab::SubAgents) {
        return subagents_hit_test(content, subagents, scroll, mouse);
    }

    let row = mouse.y.saturating_sub(content.y) as usize + scroll;
    match settings_row_hit(settings, config, subagents, row) {
        Some((_, Some(subagent_index))) => {
            Some(SettingsHitTarget::SubAgentListItem(subagent_index))
        }
        Some((field, None)) => Some(SettingsHitTarget::Field(field)),
        None => None,
    }
}

pub(crate) fn tab_hit_test(
    tab_area: Rect,
    active_tab: SettingsTab,
    mouse_x: u16,
) -> Option<SettingsTab> {
    visible_tabs(tab_area, active_tab_index(active_tab))
        .into_iter()
        .find(|tab| mouse_x >= tab.start_x && mouse_x < tab.end_x)
        .map(|tab| tab.tab)
}

pub(crate) fn active_tab_index(tab: SettingsTab) -> usize {
    let tab = if tab == SettingsTab::Provider {
        SettingsTab::Agent
    } else {
        tab
    };
    SettingsTab::all()
        .iter()
        .position(|candidate| *candidate == tab)
        .unwrap_or(0)
}

pub(crate) const SETTINGS_SIDEBAR_WIDTH: u16 = 20;
pub(crate) const SETTINGS_SIDEBAR_MIN_INNER_WIDTH: u16 = 76;

const SETTINGS_SIDEBAR_GROUPS: &[(&str, &[(SettingsTab, &str)])] = &[
    (
        "AGENTS",
        &[
            (SettingsTab::Auth, "Providers"),
            (SettingsTab::Agent, "Svarog"),
            (SettingsTab::Concierge, "Rarog"),
            (SettingsTab::SubAgents, "Sub-agents"),
        ],
    ),
    (
        "CAPABILITIES",
        &[
            (SettingsTab::Tools, "Tools"),
            (SettingsTab::Skills, "Skills"),
            (SettingsTab::WebSearch, "Web"),
            (SettingsTab::Media, "Audio & Images"),
            (SettingsTab::Memory, "Memory"),
        ],
    ),
    (
        "AUTOMATION",
        &[
            (SettingsTab::Heartbeat, "Heartbeat"),
            (SettingsTab::Proactivity, "Proactivity"),
            (SettingsTab::Workflows, "Workflows"),
            (SettingsTab::Gateway, "Gateway"),
        ],
    ),
    (
        "RUNTIME",
        &[
            (SettingsTab::Chat, "Chat"),
            (SettingsTab::Compaction, "Compaction"),
            (SettingsTab::Execution, "Execution"),
            (SettingsTab::Safety, "Safety"),
            (SettingsTab::Mlflow, "MLflow"),
        ],
    ),
    (
        "SYSTEM",
        &[
            (SettingsTab::Storage, "Storage"),
            (SettingsTab::Plugins, "Plugins"),
            (SettingsTab::Database, "Database"),
            (SettingsTab::About, "About"),
        ],
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSidebarRow {
    Blank,
    Group(&'static str),
    Tab(SettingsTab, &'static str),
}

pub(crate) fn settings_sidebar_rows() -> Vec<SettingsSidebarRow> {
    let mut rows = Vec::new();
    for (index, (group, tabs)) in SETTINGS_SIDEBAR_GROUPS.iter().enumerate() {
        if index > 0 {
            rows.push(SettingsSidebarRow::Blank);
        }
        rows.push(SettingsSidebarRow::Group(group));
        rows.extend(
            tabs.iter()
                .map(|(tab, label)| SettingsSidebarRow::Tab(*tab, label)),
        );
    }
    rows
}

pub(crate) fn settings_sidebar_scroll(height: u16, active: SettingsTab) -> usize {
    let rows = settings_sidebar_rows();
    let active = if active == SettingsTab::Provider {
        SettingsTab::Agent
    } else {
        active
    };
    let active_row = rows
        .iter()
        .position(|row| matches!(row, SettingsSidebarRow::Tab(tab, _) if *tab == active))
        .unwrap_or(0);
    let height = height as usize;
    if height == 0 || rows.len() <= height {
        return 0;
    }
    active_row
        .saturating_sub(height.saturating_sub(1))
        .min(rows.len() - height)
}

pub(crate) struct SettingsLayout {
    pub(crate) sidebar: Option<Rect>,
    pub(crate) divider: Option<Rect>,
    pub(crate) tabs_line: Option<Rect>,
    pub(crate) separator: Option<Rect>,
    pub(crate) content: Rect,
    pub(crate) hints: Rect,
}

pub(crate) fn settings_layout(area: Rect) -> Option<SettingsLayout> {
    let inner = Block::default()
        .title(" SETTINGS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(area);
    if inner.height < 5 {
        return None;
    }
    if inner.width >= SETTINGS_SIDEBAR_MIN_INNER_WIDTH {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(SETTINGS_SIDEBAR_WIDTH),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(inner);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(columns[2]);
        return Some(SettingsLayout {
            sidebar: Some(columns[0]),
            divider: Some(columns[1]),
            tabs_line: None,
            separator: None,
            content: right[0],
            hints: right[1],
        });
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    Some(SettingsLayout {
        sidebar: None,
        divider: None,
        tabs_line: Some(rows[0]),
        separator: Some(rows[1]),
        content: rows[2],
        hints: rows[3],
    })
}

fn render_settings_sidebar(
    frame: &mut Frame,
    sidebar: Rect,
    divider: Rect,
    active: SettingsTab,
    theme: &ThemeTokens,
) {
    let active = if active == SettingsTab::Provider {
        SettingsTab::Agent
    } else {
        active
    };
    let lines = settings_sidebar_rows()
        .into_iter()
        .map(|row| match row {
            SettingsSidebarRow::Blank => Line::raw(""),
            SettingsSidebarRow::Group(title) => {
                Line::from(Span::styled(format!(" {title}"), theme.fg_dim))
            }
            SettingsSidebarRow::Tab(tab, label) if tab == active => Line::from(vec![
                Span::styled(" \u{25b8} ", theme.accent_primary),
                Span::styled(label, theme.fg_active),
            ]),
            SettingsSidebarRow::Tab(_, label) => {
                Line::from(vec![Span::raw("   "), Span::styled(label, theme.fg_dim)])
            }
        })
        .collect::<Vec<_>>();
    let scroll = settings_sidebar_scroll(sidebar.height, active);
    frame.render_widget(
        Paragraph::new(lines).scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        sidebar,
    );
    frame.render_widget(
        Paragraph::new(
            (0..divider.height)
                .map(|_| Line::from(Span::styled("\u{2502}", theme.fg_dim)))
                .collect::<Vec<_>>(),
        ),
        divider,
    );
}

pub(crate) fn sidebar_hit_test(
    sidebar: Rect,
    active: SettingsTab,
    mouse: Position,
) -> Option<SettingsTab> {
    if !sidebar.contains(mouse) {
        return None;
    }
    let row = mouse.y.saturating_sub(sidebar.y) as usize
        + settings_sidebar_scroll(sidebar.height, active);
    match settings_sidebar_rows().get(row)? {
        SettingsSidebarRow::Tab(tab, _) => Some(*tab),
        _ => None,
    }
}

pub(crate) fn visible_tabs(tab_area: Rect, active_index: usize) -> Vec<VisibleTab> {
    let all = SettingsTab::all();
    let divider_width = TAB_DIVIDER.chars().count() as u16;
    let total_width = |start: usize, end: usize| -> u16 {
        (start..=end)
            .map(|idx| TAB_LABELS[idx].chars().count() as u16)
            .sum::<u16>()
            .saturating_add(divider_width.saturating_mul((end - start) as u16))
    };

    let mut start = 0usize;
    let mut end = all.len().saturating_sub(1);
    let available = tab_area.width.saturating_sub(2);

    while end > active_index && total_width(start, end) > available {
        end = end.saturating_sub(1);
    }
    while start < active_index && total_width(start, end) > available {
        start += 1;
    }
    while total_width(start, end) > available && start < end {
        if active_index.saturating_sub(start) > end.saturating_sub(active_index) {
            start += 1;
        } else {
            end = end.saturating_sub(1);
        }
    }

    let prefix = if start > 0 { "« " } else { "" };
    let mut x = tab_area.x.saturating_add(prefix.chars().count() as u16);

    let mut visible = Vec::new();
    for idx in start..=end {
        let width = TAB_LABELS[idx].chars().count() as u16;
        visible.push(VisibleTab {
            tab: all[idx],
            index: idx,
            start_x: x,
            end_x: x.saturating_add(width),
        });
        x = x.saturating_add(width);
        if idx < end {
            x = x.saturating_add(divider_width);
        }
    }
    visible
}

pub(crate) fn render_tabs_line(
    tabs: &[VisibleTab],
    settings: &SettingsState,
    theme: &ThemeTokens,
) -> Line<'static> {
    let active_index = active_tab_index(settings.active_tab());
    let mut spans = Vec::new();
    if tabs.first().is_some_and(|tab| tab.index > 0) {
        spans.push(Span::styled("« ", theme.fg_dim));
    }
    for (idx, tab) in tabs.iter().enumerate() {
        let style = if tab.index == active_index {
            theme.fg_active
        } else {
            theme.fg_dim
        };
        spans.push(Span::styled(TAB_LABELS[tab.index], style));
        if idx + 1 < tabs.len() {
            spans.push(Span::styled(TAB_DIVIDER, theme.fg_dim));
        }
    }
    if tabs
        .last()
        .is_some_and(|tab| tab.index + 1 < TAB_LABELS.len())
    {
        spans.push(Span::styled(" »", theme.fg_dim));
    }
    Line::from(spans)
}

pub(crate) fn editing_cursor_hit_test(
    content_area: Rect,
    settings: &SettingsState,
    config: &ConfigState,
    scroll: usize,
    mouse: Position,
) -> Option<(usize, usize)> {
    let field = settings.editing_field()?;
    let row = mouse.y.saturating_sub(content_area.y) as usize + scroll;
    let rel_x = mouse.x.saturating_sub(content_area.x) as usize;

    if settings.is_textarea() {
        let (text_start_row, text_start_col) = textarea_edit_layout(settings, config, field)?;
        let line_count = settings.edit_buffer().split('\n').count().max(1);
        let row_end = text_start_row + line_count;
        if row < text_start_row || row > row_end {
            return None;
        }
        let line = (row - text_start_row).min(line_count.saturating_sub(1));
        let col = rel_x.saturating_sub(text_start_col);
        return Some((line, col));
    }

    let (field_row, start_col) = single_line_edit_layout(settings, config, field)?;
    if row == field_row {
        return Some((0, rel_x.saturating_sub(start_col)));
    }
    None
}
