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
        let is_selected = settings.field_cursor() == 5;
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
        let is_selected = settings.field_cursor() == 6;
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
        let is_selected = settings.field_cursor() == 7;
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
                5,
                "OR Prefer:   ",
                if concierge.openrouter_provider_order.trim().is_empty() {
                    "(none)".to_string()
                } else {
                    concierge.openrouter_provider_order.clone()
                },
            ),
            (
                6,
                "OR Exclude:  ",
                if concierge.openrouter_provider_ignore.trim().is_empty() {
                    "(none)".to_string()
                } else {
                    concierge.openrouter_provider_ignore.clone()
                },
            ),
            (
                7,
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
        let base_url_idx = match concierge.provider.as_deref() {
            Some(zorai_shared::providers::PROVIDER_ID_OPENROUTER)
            | Some(zorai_shared::providers::PROVIDER_ID_HUGGINGFACE) => 8,
            _ => 7,
        };
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

pub(crate) fn render_feature_field_line<'a>(
    lines: &mut Vec<Line<'a>>,
    settings: &'a SettingsState,
    field_idx: usize,
    label: &'a str,
    value: &str,
    hint: &'a str,
    theme: &ThemeTokens,
) {
    let is_selected = settings.field_cursor() == field_idx;
    let expected_field = match field_idx {
        0 => "feat_tier_override",
        1 => "feat_security_level",
        2 => "feat_heartbeat_cron",
        3 => "feat_heartbeat_quiet_start",
        4 => "feat_heartbeat_quiet_end",
        10 => "feat_decay_half_life_hours",
        11 => "feat_heuristic_promotion_threshold",
        14 => "feat_skill_community_preapprove_timeout_secs",
        15 => "feat_skill_suggest_global_enable_after_approvals",
        17 => "feat_audio_stt_provider",
        18 => "feat_audio_stt_model",
        20 => "feat_audio_tts_provider",
        21 => "feat_audio_tts_model",
        22 => "feat_audio_tts_voice",
        28 => "feat_embedding_dimensions",
        _ => "",
    };
    let is_editing = settings.is_editing()
        && !expected_field.is_empty()
        && settings.editing_field() == Some(expected_field);

    let marker = if is_selected { "> " } else { "  " };
    let marker_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let display_value: String = if is_editing {
        format!("{}\u{2588}", settings.edit_buffer())
    } else {
        value.to_string()
    };
    let value_style = if is_editing {
        theme.fg_active
    } else if is_selected {
        theme.accent_primary
    } else {
        theme.fg_active
    };
    let mut spans = vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(format!("{:<17} ", label), theme.fg_dim),
        Span::styled(display_value, value_style),
    ];
    if is_selected && !is_editing {
        spans.push(Span::styled(hint, theme.fg_dim));
    }
    lines.push(Line::from(spans));
}

pub(crate) fn render_feature_toggle_line<'a>(
    lines: &mut Vec<Line<'a>>,
    settings: &SettingsState,
    field_idx: usize,
    label: &'a str,
    enabled: bool,
    theme: &ThemeTokens,
) {
    let is_selected = settings.field_cursor() == field_idx;
    let marker = if is_selected { "> " } else { "  " };
    let marker_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let check = if enabled { "[x]" } else { "[ ]" };
    let check_style = if enabled {
        theme.accent_success
    } else {
        theme.fg_dim
    };
    let label_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_active
    };
    let mut spans = vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(check, check_style),
        Span::raw(" "),
        Span::styled(label, label_style),
    ];
    if is_selected {
        spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
    }
    lines.push(Line::from(spans));
}
