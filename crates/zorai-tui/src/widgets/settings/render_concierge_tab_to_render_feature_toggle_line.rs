use super::render_edit_buffer_with_cursor;
use crate::state::concierge::ConciergeState;
use crate::state::settings::SettingsState;
use crate::theme::ThemeTokens;
use ratatui::text::{Line, Span};
pub(crate) fn render_concierge_tab<'a>(
    settings: &'a SettingsState,
    concierge: &'a ConciergeState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let edit_value = |field: &str, current: String| {
        if settings.is_editing() && settings.editing_field() == Some(field) {
            render_edit_buffer_with_cursor(settings.edit_buffer(), settings.edit_cursor())
        } else {
            current
        }
    };
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        format!(
            "  {}",
            crate::agent_aliases::display_agent_name(zorai_protocol::AGENT_NAME_RAROG)
        ),
        theme.fg_active,
    )));
    lines.push(Line::from(Span::styled(
        "  Welcome agent and operational assistant",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    {
        let is_selected = settings.field_cursor() == 0;
        let marker = if is_selected { "> " } else { "  " };
        let check = if concierge.enabled { "[x]" } else { "[ ]" };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled(
                check,
                if concierge.enabled {
                    theme.accent_success
                } else {
                    theme.fg_dim
                },
            ),
            Span::raw(" "),
            Span::styled(
                "Enabled",
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
    }

    {
        let is_selected = settings.field_cursor() == 1;
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("Detail Level: ", theme.fg_dim),
            Span::styled(
                concierge.detail_level.clone(),
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
    }

    {
        let is_selected = settings.field_cursor() == 2;
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("Provider:     ", theme.fg_dim),
            Span::styled(
                concierge
                    .provider
                    .clone()
                    .unwrap_or_else(|| format!("(use {})", zorai_protocol::AGENT_NAME_SWAROG)),
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
    }

    {
        let is_selected = settings.field_cursor() == 3;
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("Model:        ", theme.fg_dim),
            Span::styled(
                edit_value(
                    "concierge_model",
                    concierge
                        .model
                        .clone()
                        .unwrap_or_else(|| format!("(use {})", zorai_protocol::AGENT_NAME_SWAROG)),
                ),
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
    }

    {
        let is_selected = settings.field_cursor() == 4;
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("Reasoning:    ", theme.fg_dim),
            Span::styled(
                concierge
                    .reasoning_effort
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
    }

    {
        let is_selected =
            settings.field_cursor() == concierge.settings_field_index("concierge_api_transport");
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("Transport:    ", theme.fg_dim),
            Span::styled(
                concierge
                    .api_transport
                    .clone()
                    .unwrap_or_else(|| "(inherit)".to_string()),
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
    }

    if concierge.provider.as_deref() != Some(zorai_shared::providers::PROVIDER_ID_OPENROUTER) {
        let is_selected = settings.field_cursor()
            == concierge.settings_field_index("concierge_claude_permission_mode");
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("Claude perms: ", theme.fg_dim),
            Span::styled(
                concierge
                    .claude_permission_mode
                    .clone()
                    .unwrap_or_else(|| "(default)".to_string()),
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
    }

    if concierge.provider.as_deref() == Some(zorai_shared::providers::PROVIDER_ID_HUGGINGFACE) {
        let is_selected = settings.field_cursor()
            == concierge.settings_field_index("concierge_huggingface_provider");
        let marker = if is_selected { "> " } else { "  " };
        let value = if concierge.huggingface_provider.trim().is_empty() {
            "auto"
        } else {
            concierge.huggingface_provider.trim()
        };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("HF Route:     ", theme.fg_dim),
            Span::styled(
                edit_value("concierge_huggingface_provider", value.to_string()),
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
    }

    if concierge.provider.as_deref() == Some(zorai_shared::providers::PROVIDER_ID_OPENROUTER) {
        let openrouter_fields = [
            (
                concierge.settings_field_index("concierge_openrouter_provider_order"),
                "OR Prefer:   ",
                if concierge.openrouter_provider_order.trim().is_empty() {
                    "(none)".to_string()
                } else {
                    concierge.openrouter_provider_order.clone()
                },
            ),
            (
                concierge.settings_field_index("concierge_openrouter_provider_ignore"),
                "OR Exclude:  ",
                if concierge.openrouter_provider_ignore.trim().is_empty() {
                    "(none)".to_string()
                } else {
                    concierge.openrouter_provider_ignore.clone()
                },
            ),
            (
                concierge.settings_field_index("concierge_openrouter_allow_fallbacks"),
                "OR Fallback: ",
                if concierge.openrouter_allow_fallbacks {
                    "allowed".to_string()
                } else {
                    "strict".to_string()
                },
            ),
        ];
        for (idx, label, value) in openrouter_fields {
            let is_selected = settings.field_cursor() == idx;
            let marker = if is_selected { "> " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(
                    marker,
                    if is_selected {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::styled(label, theme.fg_dim),
                Span::styled(
                    value,
                    if is_selected {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
            ]));
        }
    }

    {
        let base_url_idx = concierge.settings_field_index("concierge_base_url");
        let is_selected = settings.field_cursor() == base_url_idx;
        let marker = if is_selected { "> " } else { "  " };
        let value = if concierge.base_url.trim().is_empty() {
            "(provider default)".to_string()
        } else {
            concierge.base_url.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("Base URL:     ", theme.fg_dim),
            Span::styled(
                edit_value("concierge_base_url", value),
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
        let is_selected = settings.field_cursor() == base_url_idx + 1;
        let marker = if is_selected { "> " } else { "  " };
        let alias = crate::agent_aliases::current()
            .get("rarog")
            .cloned()
            .unwrap_or_default();
        let (value, value_style) = if alias.is_empty() {
            (zorai_protocol::AGENT_NAME_RAROG.to_string(), theme.fg_dim)
        } else if is_selected {
            (alias, theme.fg_active)
        } else {
            (alias, theme.fg_dim)
        };
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                if is_selected {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("Alias:        ", theme.fg_dim),
            Span::styled(edit_value("rarog_alias", value), value_style),
        ]));
    }

    lines
}
