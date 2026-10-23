use super::render_plugins_tab_to_connector_readiness_style::*;
use crate::state::config::ConfigState;
use crate::state::settings::SettingsState;
use crate::theme::ThemeTokens;
use ratatui::text::{Line, Span};
pub(crate) fn render_honcho_editor_block<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    if let Some(editor) = config.honcho_editor.as_ref() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Honcho Memory Settings",
            theme.fg_dim,
        )));
        lines.push(render_honcho_editor_toggle(
            editor,
            crate::state::config::HonchoEditorField::Enabled,
            "Enabled",
            editor.enabled,
            theme,
        ));
        lines.push(render_honcho_editor_text_field(
            settings,
            editor,
            crate::state::config::HonchoEditorField::ApiKey,
            "API Key",
            &editor.api_key,
            "honcho_editor_api_key",
            true,
            theme,
        ));
        lines.push(render_honcho_editor_text_field(
            settings,
            editor,
            crate::state::config::HonchoEditorField::BaseUrl,
            "Base URL",
            &editor.base_url,
            "honcho_editor_base_url",
            false,
            theme,
        ));
        lines.push(render_honcho_editor_text_field(
            settings,
            editor,
            crate::state::config::HonchoEditorField::WorkspaceId,
            "Workspace",
            &editor.workspace_id,
            "honcho_editor_workspace_id",
            false,
            theme,
        ));
        lines.push(render_honcho_editor_actions(editor, theme));
        lines.push(Line::raw(""));
    }
    lines
}

pub(crate) fn render_honcho_editor_toggle<'a>(
    editor: &crate::state::config::HonchoEditorState,
    field: crate::state::config::HonchoEditorField,
    label: &'a str,
    enabled: bool,
    theme: &ThemeTokens,
) -> Line<'a> {
    let is_selected = editor.field == field;
    let marker = if is_selected { "  > " } else { "    " };
    let check = if enabled { "[x]" } else { "[ ]" };
    let mut spans = vec![
        Span::styled(
            marker,
            if is_selected {
                theme.accent_primary
            } else {
                theme.fg_dim
            },
        ),
        Span::styled(
            check,
            if enabled {
                theme.accent_success
            } else {
                theme.fg_dim
            },
        ),
        Span::raw(" "),
        Span::styled(
            label,
            if is_selected {
                theme.accent_primary
            } else {
                theme.fg_active
            },
        ),
    ];
    if is_selected {
        spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
    }
    Line::from(spans)
}

pub(crate) fn render_honcho_editor_text_field<'a>(
    settings: &SettingsState,
    editor: &crate::state::config::HonchoEditorState,
    field: crate::state::config::HonchoEditorField,
    label: &'a str,
    value: &str,
    field_name: &'a str,
    password: bool,
    theme: &ThemeTokens,
) -> Line<'a> {
    let is_selected = editor.field == field;
    let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
    let display_value = if is_editing {
        format!("{}\u{2588}", settings.edit_buffer())
    } else if value.is_empty() {
        "(not set)".to_string()
    } else if password {
        mask_api_key(value)
    } else {
        value.to_string()
    };
    let mut spans = vec![
        Span::styled(
            if is_selected { "  > " } else { "    " },
            if is_selected {
                theme.accent_primary
            } else {
                theme.fg_dim
            },
        ),
        Span::styled(format!("{label:<12} "), theme.fg_dim),
        Span::styled(
            display_value,
            if is_editing {
                theme.fg_active
            } else if is_selected {
                theme.accent_primary
            } else {
                theme.fg_active
            },
        ),
    ];
    if is_selected && !is_editing {
        spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
    }
    Line::from(spans)
}

pub(crate) fn render_honcho_editor_actions<'a>(
    editor: &crate::state::config::HonchoEditorState,
    theme: &ThemeTokens,
) -> Line<'a> {
    let save_selected = editor.field == crate::state::config::HonchoEditorField::Save;
    let cancel_selected = editor.field == crate::state::config::HonchoEditorField::Cancel;
    Line::from(vec![
        Span::styled(
            if save_selected || cancel_selected {
                "  > "
            } else {
                "    "
            },
            if save_selected || cancel_selected {
                theme.accent_primary
            } else {
                theme.fg_dim
            },
        ),
        Span::styled(
            "[Save]",
            if save_selected {
                theme.accent_primary
            } else {
                theme.fg_dim
            },
        ),
        Span::raw("  "),
        Span::styled(
            "[Cancel]",
            if cancel_selected {
                theme.accent_primary
            } else {
                theme.fg_dim
            },
        ),
    ])
}
