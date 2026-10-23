use super::*;
use crate::providers;
use crate::state::config::ConfigState;
use crate::state::settings::SettingsState;
use crate::theme::ThemeTokens;
use ratatui::text::{Line, Span};
pub(crate) fn render_provider_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        format!("  {} Provider", zorai_protocol::AGENT_NAME_SWAROG),
        theme.fg_active,
    )));
    lines.push(Line::from(Span::styled(
        format!(
            "  Select {}'s LLM provider and runtime settings. Credentials are managed in Auth.",
            zorai_protocol::AGENT_NAME_SWAROG,
        ),
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    let provider_val = if config.provider().is_empty() {
        "(not set)".to_string()
    } else {
        config.provider().to_string()
    };
    let base_url_val = if config.base_url().is_empty() {
        "(not set)".to_string()
    } else {
        config.base_url().to_string()
    };
    let model_val = if config.model().is_empty() {
        "(not set)".to_string()
    } else {
        config.model().to_string()
    };
    let auth_source_val = match config.auth_source.as_str() {
        "chatgpt_subscription" => "ChatGPT subscription".to_string(),
        "github_copilot" => "GitHub browser login".to_string(),
        _ => "API key".to_string(),
    };
    let fixed_transport = providers::fixed_transport_for_model(&config.provider, &config.model);
    let uses_fixed_anthropic_messages =
        providers::uses_fixed_anthropic_messages(&config.provider, &config.model);
    let transport_val = if let Some(fixed_transport) = fixed_transport {
        match fixed_transport {
            "native_assistant" => "native assistant".to_string(),
            "anthropic_messages" => "anthropic messages".to_string(),
            "responses" => "responses".to_string(),
            _ => "chat completions".to_string(),
        }
    } else if uses_fixed_anthropic_messages {
        "anthropic messages".to_string()
    } else if config.api_transport().is_empty() {
        providers::default_transport_for(&config.provider).to_string()
    } else {
        match config.api_transport() {
            "native_assistant" => "native assistant".to_string(),
            "anthropic_messages" => "anthropic messages".to_string(),
            "responses" => "responses".to_string(),
            _ => "chat completions".to_string(),
        }
    };
    let assistant_id_val = if config.assistant_id.is_empty() {
        "(not set)".to_string()
    } else {
        config.assistant_id.clone()
    };
    use zorai_shared::providers::{
        PROVIDER_ID_CUSTOM, PROVIDER_ID_HUGGINGFACE, PROVIDER_ID_OPENROUTER,
    };

    let effort_val = if config.reasoning_effort().is_empty() {
        "off".to_string()
    } else {
        config.reasoning_effort().to_string()
    };
    let context_window_val = format!("{} tok", config.context_window_tokens);
    let openrouter_provider_order_val = if config.openrouter_provider_order.trim().is_empty() {
        "(none)".to_string()
    } else {
        config.openrouter_provider_order.clone()
    };
    let openrouter_provider_ignore_val = if config.openrouter_provider_ignore.trim().is_empty() {
        "(none)".to_string()
    } else {
        config.openrouter_provider_ignore.clone()
    };
    let openrouter_fallbacks_val = if config.openrouter_allow_fallbacks {
        "allowed".to_string()
    } else {
        "strict".to_string()
    };
    let openrouter_cache_val = if config.openrouter_response_cache_enabled {
        "enabled".to_string()
    } else {
        "disabled".to_string()
    };
    let huggingface_provider_val = if config.huggingface_provider.trim().is_empty() {
        "auto".to_string()
    } else {
        config.huggingface_provider.clone()
    };
    let context_hint = " [Enter: edit]";
    let transport_hint = if fixed_transport.is_some() || uses_fixed_anthropic_messages {
        ""
    } else if providers::supported_transports_for(&config.provider).len() <= 1 {
        match providers::supported_transports_for(&config.provider)
            .first()
            .copied()
            .unwrap_or("chat_completions")
        {
            "native_assistant" => " [native assistant only]",
            "anthropic_messages" => " [anthropic messages only]",
            "responses" => " [responses only]",
            _ => " [chat completions only]",
        }
    } else {
        " [Enter: cycle]"
    };

    let mut fields: Vec<(usize, &str, String, &str, &str)> = vec![
        (0, "Provider", provider_val, "provider", " [Enter: pick]"),
        (1, "Base URL", base_url_val, "base_url", " [Enter: edit]"),
        (2, "Auth", auth_source_val, "auth_source", " [Enter: cycle]"),
        (
            3,
            "Model",
            model_val,
            "model",
            if config.provider == PROVIDER_ID_CUSTOM {
                " [Enter: edit]"
            } else {
                " [Enter: pick]"
            },
        ),
        (
            4,
            "Transport",
            transport_val,
            "api_transport",
            transport_hint,
        ),
        (
            5,
            "Assistant ID",
            assistant_id_val,
            "assistant_id",
            " [Enter: edit]",
        ),
        (
            6,
            "Effort",
            effort_val,
            "reasoning_effort",
            " [Enter: pick]",
        ),
        (
            7,
            "Ctx Length",
            context_window_val,
            "context_window_tokens",
            context_hint,
        ),
    ];
    if config.provider == PROVIDER_ID_OPENROUTER {
        fields.extend([
            (
                8,
                "OR Prefer",
                openrouter_provider_order_val,
                "openrouter_provider_order",
                " [Enter: pick]",
            ),
            (
                9,
                "OR Exclude",
                openrouter_provider_ignore_val,
                "openrouter_provider_ignore",
                " [Enter: pick]",
            ),
            (
                10,
                "OR Fallbacks",
                openrouter_fallbacks_val,
                "openrouter_allow_fallbacks",
                " [Enter: toggle]",
            ),
            (
                11,
                "OR Cache",
                openrouter_cache_val,
                "openrouter_response_cache_enabled",
                " [Enter: toggle]",
            ),
        ]);
    }
    if config.provider == PROVIDER_ID_HUGGINGFACE {
        fields.push((
            8,
            "HF Route",
            huggingface_provider_val,
            "huggingface_provider",
            " [Enter: edit]",
        ));
    }

    for (idx, label, value, field_name, hint) in &fields {
        let is_selected = settings.field_cursor() == *idx;
        let is_editing = settings.is_editing()
            && (settings.editing_field() == Some(*field_name)
                || (*field_name == "model"
                    && matches!(
                        settings.editing_field(),
                        Some("custom_model_entry" | "target_agent_model")
                    ))
                || (*field_name == "context_window_tokens"
                    && settings.editing_field() == Some("target_agent_context_window")));

        let marker = if is_selected { ">" } else { " " };

        let display_value: String = if is_editing {
            clip_inline_text(&format!("{}\u{2588}", settings.edit_buffer()), 52)
        } else {
            clip_inline_text(value, 52)
        };

        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(format!(" {} ", marker), marker_style),
            Span::styled(format!("{:<15} ", label), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];

        if is_selected && !is_editing {
            spans.push(Span::styled(*hint, theme.fg_dim));
        }

        lines.push(Line::from(spans));
    }

    lines
}
