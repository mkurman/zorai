use super::render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content::{
    render_edit_buffer_with_cursor, render_edit_line_with_cursor,
};
use super::render_plugins_tab_to_connector_readiness_style::mask_api_key;
use crate::state::config::ConfigState;
use crate::state::settings::SettingsState;
use crate::state::settings_registry::{
    SettingsFieldKind, SettingsFieldValue, SettingsPageRow, SettingsPageSpec,
    SETTINGS_PAGE_LABEL_WIDTH,
};
use crate::theme::ThemeTokens;
use ratatui::text::{Line, Span};

pub(crate) fn render_settings_page<'a>(
    page: &'static SettingsPageSpec,
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let (cursor_line, cursor_col) = settings.edit_cursor_line_col();
    page.rows(config, settings.editing_textarea())
        .into_iter()
        .map(|row| match row {
            SettingsPageRow::Blank => Line::raw(""),
            SettingsPageRow::Title(title) => {
                Line::from(Span::styled(format!("  {title}"), theme.fg_active))
            }
            SettingsPageRow::Description(description) => {
                Line::from(Span::styled(format!("  {description}"), theme.fg_dim))
            }
            SettingsPageRow::Section(title) => Line::from(Span::styled(
                format!("  {}", title.to_uppercase()),
                theme.accent_secondary,
            )),
            SettingsPageRow::Note(note) => {
                Line::from(Span::styled(format!("  {note}"), theme.fg_dim))
            }
            SettingsPageRow::TextareaLine(line) => {
                let text = settings.edit_buffer().split('\n').nth(line).unwrap_or("");
                let text = if line == cursor_line {
                    render_edit_line_with_cursor(text, cursor_col)
                } else {
                    text.to_string()
                };
                Line::from(vec![
                    Span::styled("  \u{2502} ", theme.fg_dim),
                    Span::styled(text, theme.fg_active),
                ])
            }
            SettingsPageRow::Field { index, spec } => {
                let is_selected = settings.field_cursor() == index;
                let is_editing =
                    settings.is_editing() && settings.editing_field() == Some(spec.name);
                let (value, value_style) = if is_editing && spec.kind == SettingsFieldKind::Textarea
                {
                    (String::new(), theme.fg_active)
                } else if is_editing {
                    (
                        render_edit_buffer_with_cursor(
                            settings.edit_buffer(),
                            settings.edit_cursor(),
                        ),
                        theme.fg_active,
                    )
                } else {
                    match ((spec.value)(config), spec.kind) {
                        (SettingsFieldValue::Bool(enabled), _) => (
                            if enabled { "[x]" } else { "[ ]" }.to_string(),
                            if enabled {
                                theme.accent_success
                            } else {
                                theme.fg_dim
                            },
                        ),
                        (SettingsFieldValue::Text(text), SettingsFieldKind::Secret) => (
                            if text.is_empty() {
                                "(not set)".to_string()
                            } else {
                                mask_api_key(&text)
                            },
                            theme.fg_active,
                        ),
                        (SettingsFieldValue::Text(text), _) => (text, theme.fg_active),
                    }
                };
                let label_style = if is_selected {
                    theme.accent_primary
                } else if spec.kind == SettingsFieldKind::Action {
                    theme.fg_active
                } else {
                    theme.fg_dim
                };
                let mut spans = vec![
                    Span::styled(
                        if is_selected { "> " } else { "  " },
                        if is_selected {
                            theme.accent_primary
                        } else {
                            theme.fg_dim
                        },
                    ),
                    Span::styled(
                        format!("{:<width$} ", spec.label, width = SETTINGS_PAGE_LABEL_WIDTH),
                        label_style,
                    ),
                    Span::styled(value, value_style),
                ];
                let hint = if is_editing && spec.kind == SettingsFieldKind::Textarea {
                    Some("[Ctrl+S: save, Esc: cancel]")
                } else if is_selected && !is_editing {
                    Some(match spec.kind {
                        SettingsFieldKind::Toggle => "  [Space: toggle]",
                        SettingsFieldKind::Choice => "  [Enter: cycle]",
                        SettingsFieldKind::Picker => "  [Enter: pick]",
                        SettingsFieldKind::Action => "  [Enter: run]",
                        SettingsFieldKind::Text
                        | SettingsFieldKind::Secret
                        | SettingsFieldKind::Number
                        | SettingsFieldKind::Textarea => "  [Enter: edit]",
                    })
                } else {
                    None
                };
                if let Some(hint) = hint {
                    spans.push(Span::styled(hint, theme.fg_dim));
                }
                Line::from(spans)
            }
        })
        .collect()
}
