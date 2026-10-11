use super::render_plugins_tab_to_connector_readiness_style::*;
use super::*;
use crate::state::config::ConfigState;
use crate::state::modal::{ModalState, WhatsAppLinkPhase};
use crate::state::settings::SettingsState;
use crate::theme::ThemeTokens;
use ratatui::text::{Line, Span};
use zorai_protocol::has_whatsapp_allowed_contacts;
pub(crate) fn render_gateway_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    modal: &'a ModalState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Gateway", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Messaging platform connections",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    {
        let is_selected = settings.field_cursor() == 0;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check = if config.gateway_enabled { "[x]" } else { "[ ]" };
        let check_style = if config.gateway_enabled {
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
            Span::styled(marker, marker_style),
            Span::styled(check, check_style),
            Span::raw(" "),
            Span::styled("Enable Gateway", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        1,
        "Default Agent",
        &config.gateway_default_agent,
        "gateway_default_agent",
        false,
    );

    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        2,
        "Command Prefix",
        &config.gateway_prefix,
        "gateway_prefix",
        false,
    );

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Slack \u{2500}\u{2500}",
        theme.fg_dim,
    )));
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        3,
        "Bot Token",
        &config.slack_token,
        "slack_token",
        true,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        4,
        "Channel Filter",
        &config.slack_channel_filter,
        "slack_channel_filter",
        false,
    );
    render_gateway_agent_field(settings, theme, &mut lines, 5, &config.slack_default_agent);

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Telegram \u{2500}\u{2500}",
        theme.fg_dim,
    )));
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        6,
        "Bot Token",
        &config.telegram_token,
        "telegram_token",
        true,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        7,
        "Allowed Chats",
        &config.telegram_allowed_chats,
        "telegram_allowed_chats",
        false,
    );
    render_gateway_agent_field(
        settings,
        theme,
        &mut lines,
        8,
        &config.telegram_default_agent,
    );

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Discord \u{2500}\u{2500}",
        theme.fg_dim,
    )));
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        9,
        "Bot Token",
        &config.discord_token,
        "discord_token",
        true,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        10,
        "Channel Filter",
        &config.discord_channel_filter,
        "discord_channel_filter",
        false,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        11,
        "Allowed Users",
        &config.discord_allowed_users,
        "discord_allowed_users",
        false,
    );
    render_gateway_agent_field(
        settings,
        theme,
        &mut lines,
        12,
        &config.discord_default_agent,
    );

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} WhatsApp \u{2500}\u{2500}",
        theme.fg_dim,
    )));
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        13,
        "Allowed Contacts",
        &config.whatsapp_allowed_contacts,
        "whatsapp_allowed_contacts",
        false,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        14,
        "API Token",
        &config.whatsapp_token,
        "whatsapp_token",
        true,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        15,
        "Phone Number ID",
        &config.whatsapp_phone_id,
        "whatsapp_phone_id",
        false,
    );
    render_gateway_agent_field(
        settings,
        theme,
        &mut lines,
        16,
        &config.whatsapp_default_agent,
    );
    let whatsapp_link = modal.whatsapp_link();
    let linked = whatsapp_link.phase() == WhatsAppLinkPhase::Connected;
    let whatsapp_allowlist_ready = has_whatsapp_allowed_contacts(&config.whatsapp_allowed_contacts);
    {
        let is_selected = settings.field_cursor() == 17;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let label_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let action_label = if linked { "Link Status" } else { "Link Device" };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(action_label, label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Enter]", theme.fg_dim));
        }
        if !linked && !whatsapp_allowlist_ready {
            spans.push(Span::styled("  (requires allowed contacts)", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }
    {
        let is_selected = settings.field_cursor() == 18;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let enabled = linked;
        let label_style = if is_selected && enabled {
            theme.accent_primary
        } else if enabled {
            theme.fg_active
        } else {
            theme.fg_dim
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled("Re-link Device", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Enter]", theme.fg_dim));
        }
        if !enabled {
            spans.push(Span::styled("  (link first)", theme.fg_dim));
        } else if !whatsapp_allowlist_ready {
            spans.push(Span::styled("  (requires allowed contacts)", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled(
        if whatsapp_allowlist_ready {
            "  Only allowed numbers will be forwarded and can receive replies."
        } else {
            "  Add at least one allowed phone number before QR linking."
        },
        theme.fg_dim,
    )));
    lines.push(Line::from(Span::styled(
        match whatsapp_link.phase() {
            WhatsAppLinkPhase::Connected => {
                format!("  Linked: {}", whatsapp_link.phone().unwrap_or("device"))
            }
            WhatsAppLinkPhase::AwaitingScan => "  Status: awaiting QR scan".to_string(),
            WhatsAppLinkPhase::Starting => "  Status: starting link workflow".to_string(),
            WhatsAppLinkPhase::Error => format!(
                "  Status: error — {}",
                whatsapp_link.last_error().unwrap_or("unknown")
            ),
            WhatsAppLinkPhase::Disconnected => format!(
                "  Status: disconnected — {}",
                whatsapp_link.last_error().unwrap_or("none")
            ),
            WhatsAppLinkPhase::Idle => "  Status: not linked".to_string(),
        },
        if linked {
            theme.accent_success
        } else {
            theme.fg_dim
        },
    )));
    lines.push(Line::from(Span::styled(
        "  Allowed Contacts accepts comma or newline separated phone numbers.",
        theme.fg_dim,
    )));
    lines.push(Line::from(Span::styled(
        "  Configure WhatsApp Cloud API token + phone number ID for send tools.",
        theme.fg_dim,
    )));

    lines
}

/// Render a single editable gateway field row.
/// `password` — if true and value is non-empty, the stored value is masked (dots).
pub(crate) fn render_gateway_text_field<'a>(
    settings: &SettingsState,
    theme: &ThemeTokens,
    lines: &mut Vec<Line<'a>>,
    field_idx: usize,
    label: &'a str,
    value: &str,
    field_name: &'a str,
    password: bool,
) {
    let is_selected = settings.field_cursor() == field_idx;
    let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
    let marker = if is_selected { "> " } else { "  " };
    let marker_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };

    if is_editing && settings.is_textarea() {
        lines.push(Line::from(vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16}", label), theme.fg_dim),
            Span::styled(" [Ctrl+S/Ctrl+Enter: save, Esc: cancel]", theme.fg_dim),
        ]));
        lines.push(Line::from(Span::styled(
            "  ╭──────────────────────────────────────────╮",
            theme.fg_dim,
        )));
        for (idx, buf_line) in settings.edit_buffer().split('\n').enumerate() {
            let rendered = if idx == settings.edit_cursor_line_col().0 {
                render_edit_line_with_cursor(buf_line, settings.edit_cursor_line_col().1)
            } else {
                buf_line.to_string()
            };
            lines.push(Line::from(vec![
                Span::styled("  │ ", theme.fg_dim),
                Span::styled(rendered, theme.fg_active),
            ]));
        }
        lines.push(Line::from(Span::styled(
            "  ╰──────────────────────────────────────────╯",
            theme.fg_dim,
        )));
        return;
    }

    let display_value: String = if is_editing {
        format!("{}\u{2588}", settings.edit_buffer())
    } else if value.is_empty() {
        "(not set)".to_string()
    } else if password {
        mask_api_key(value)
    } else if field_name == "whatsapp_allowed_contacts" {
        let first_lines: Vec<&str> = value.lines().take(2).collect();
        let preview = first_lines.join(", ");
        if preview.is_empty() {
            "(not set)".to_string()
        } else if value.lines().count() > 2 || preview.chars().count() > 40 {
            let truncated: String = preview.chars().take(37).collect();
            format!("{}...", truncated)
        } else {
            preview
        }
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
        Span::styled(marker, marker_style),
        Span::styled(format!("{:<16} ", label), theme.fg_dim),
        Span::styled(display_value, value_style),
    ];
    if is_selected && !is_editing {
        spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
    }
    lines.push(Line::from(spans));
}

fn render_gateway_agent_field<'a>(
    settings: &SettingsState,
    theme: &ThemeTokens,
    lines: &mut Vec<Line<'a>>,
    field_idx: usize,
    value: &str,
) {
    let is_selected = settings.field_cursor() == field_idx;
    let marker = if is_selected { "> " } else { "  " };
    let marker_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let display_value = if value.trim().is_empty() {
        "(gateway default)".to_string()
    } else {
        crate::agent_aliases::display_agent_name(value)
    };
    let value_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_active
    };
    let mut spans = vec![
        Span::styled(marker, marker_style),
        Span::styled(format!("{:<16} ", "Agent"), theme.fg_dim),
        Span::styled(display_value, value_style),
    ];
    if is_selected {
        spans.push(Span::styled("  [Enter: pick]", theme.fg_dim));
    }
    lines.push(Line::from(spans));
}
