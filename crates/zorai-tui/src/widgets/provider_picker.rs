use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use crate::providers::{ProviderDef, PROVIDERS};
use crate::state::auth::AuthState;
use crate::state::config::ConfigState;
use crate::state::modal::ModalState;
use crate::theme::ThemeTokens;
use zorai_shared::providers::{
    provider_supports_audio_tool, AudioToolKind, PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_CUSTOM,
    PROVIDER_ID_ELEVENLABS, PROVIDER_ID_GEMINI, PROVIDER_ID_MINIMAX, PROVIDER_ID_OPENAI,
    PROVIDER_ID_OPENROUTER, PROVIDER_ID_QWEN,
};

pub fn available_provider_defs(auth: &AuthState) -> Vec<&'static ProviderDef> {
    let mut providers = PROVIDERS
        .iter()
        .filter(|provider| {
            provider.id == PROVIDER_ID_CUSTOM
                || auth
                    .entries
                    .iter()
                    .any(|entry| entry.authenticated && entry.provider_id == provider.id)
        })
        .collect::<Vec<_>>();
    providers.extend(auth.entries.iter().filter_map(|entry| {
        let is_builtin = PROVIDERS
            .iter()
            .any(|provider| provider.id == entry.provider_id);
        if is_builtin || entry.provider_id.trim().is_empty() {
            return None;
        }
        Some(Box::leak(Box::new(ProviderDef {
            id: Box::leak(entry.provider_id.clone().into_boxed_str()),
            name: Box::leak(entry.provider_name.clone().into_boxed_str()),
            default_base_url: "",
            default_model: Box::leak(entry.model.clone().into_boxed_str()),
            supported_transports: crate::providers::CHAT_ONLY_TRANSPORTS,
            default_transport: "chat_completions",
            supported_auth_sources: crate::providers::API_KEY_ONLY_AUTH_SOURCES,
            default_auth_source: "api_key",
            native_base_url: None,
        })) as &'static ProviderDef)
    }));
    providers
}

pub fn available_audio_provider_defs(
    auth: &AuthState,
    audio_tool_kind: AudioToolKind,
) -> Vec<&'static ProviderDef> {
    available_provider_defs(auth)
        .into_iter()
        .filter(|provider| provider_supports_audio_tool(provider.id, audio_tool_kind))
        .collect()
}

pub fn available_embedding_provider_defs(auth: &AuthState) -> Vec<&'static ProviderDef> {
    available_provider_defs(auth)
        .into_iter()
        .filter(|provider| {
            matches!(
                provider.id,
                PROVIDER_ID_OPENAI
                    | PROVIDER_ID_AZURE_OPENAI
                    | PROVIDER_ID_OPENROUTER
                    | PROVIDER_ID_CUSTOM
            )
        })
        .collect()
}

pub fn available_video_generation_provider_defs(auth: &AuthState) -> Vec<&'static ProviderDef> {
    available_provider_defs(auth)
        .into_iter()
        .filter(|provider| {
            matches!(
                provider.id,
                PROVIDER_ID_MINIMAX
                    | PROVIDER_ID_QWEN
                    | PROVIDER_ID_GEMINI
                    | PROVIDER_ID_OPENROUTER
            )
        })
        .collect()
}

pub fn available_music_generation_provider_defs(auth: &AuthState) -> Vec<&'static ProviderDef> {
    available_provider_defs(auth)
        .into_iter()
        .filter(|provider| {
            matches!(
                provider.id,
                PROVIDER_ID_ELEVENLABS | PROVIDER_ID_GEMINI | PROVIDER_ID_MINIMAX
            )
        })
        .collect()
}

pub fn filtered_provider_defs(
    providers: Vec<&'static ProviderDef>,
    query: &str,
) -> Vec<&'static ProviderDef> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return providers;
    }
    let terms = query.split_whitespace().collect::<Vec<_>>();
    providers
        .into_iter()
        .filter(|provider| provider_matches_query(provider, &terms))
        .collect()
}

fn provider_matches_query(provider: &ProviderDef, terms: &[&str]) -> bool {
    let searchable = format!(
        "{} {} {}",
        provider.name, provider.id, provider.default_model
    )
    .to_ascii_lowercase();
    terms.iter().all(|term| searchable.contains(term))
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    modal: &ModalState,
    config: &ConfigState,
    auth: &AuthState,
    audio_tool_kind: Option<AudioToolKind>,
    embedding_only: bool,
    video_generation_only: bool,
    music_generation_only: bool,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" PROVIDER ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let cursor = modal.picker_cursor();
    let active_provider = config.provider();
    let providers = if embedding_only {
        available_embedding_provider_defs(auth)
    } else if video_generation_only {
        available_video_generation_provider_defs(auth)
    } else if music_generation_only {
        available_music_generation_provider_defs(auth)
    } else {
        audio_tool_kind
            .map(|kind| available_audio_provider_defs(auth, kind))
            .unwrap_or_else(|| available_provider_defs(auth))
    };
    let providers = filtered_provider_defs(providers, modal.command_query());

    if providers.is_empty() {
        let message = if modal.command_query().trim().is_empty() {
            "No authenticated providers. Configure one in Auth.".to_string()
        } else {
            format!("No providers match '{}'.", modal.command_query().trim())
        };
        frame.render_widget(Paragraph::new(message).style(theme.fg_dim), chunks[0]);

        let hints = Line::from(vec![
            Span::raw(" "),
            Span::styled("Esc", theme.fg_active),
            Span::styled(" close", theme.fg_dim),
        ]);
        frame.render_widget(Paragraph::new(hints), chunks[1]);
        return;
    }

    let list_items: Vec<ListItem> = providers
        .iter()
        .enumerate()
        .map(|(i, provider_def)| {
            let is_selected = i == cursor;
            let is_active = provider_def.id == active_provider
                || provider_def.name.eq_ignore_ascii_case(active_provider);

            if is_selected {
                ListItem::new(Line::from(vec![
                    Span::raw(" > "),
                    Span::raw(provider_def.name),
                ]))
                .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
            } else if is_active && !active_provider.is_empty() {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("\u{2022} {}", provider_def.name),
                        theme.accent_secondary,
                    ),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(provider_def.name, theme.fg_active),
                ]))
            }
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, chunks[0]);

    let hints = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" nav  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" sel  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::auth::{AuthState, ProviderAuthEntry};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use zorai_shared::providers::{
        AudioToolKind, PROVIDER_ID_ANTHROPIC, PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_GROQ,
        PROVIDER_ID_OPENAI, PROVIDER_ID_OPENROUTER, PROVIDER_ID_XAI,
    };

    fn rendered_picker_text(modal: &ModalState, config: &ConfigState, auth: &AuthState) -> String {
        let theme = ThemeTokens::default();
        let backend = TestBackend::new(72, 10);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| {
                render(
                    frame,
                    Rect::new(0, 0, 72, 10),
                    modal,
                    config,
                    auth,
                    None,
                    false,
                    false,
                    false,
                    &theme,
                );
            })
            .expect("render should succeed");

        let buffer = terminal.backend().buffer();
        (0..10)
            .map(|y| {
                (0..72)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn available_provider_defs_filters_to_authenticated_entries_plus_custom() {
        let mut auth = AuthState::new();
        auth.entries = vec![
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_OPENAI.to_string(),
                provider_name: "OpenAI".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "gpt-5.4".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_GROQ.to_string(),
                provider_name: "Groq".to_string(),
                authenticated: false,
                auth_source: "api_key".to_string(),
                model: "llama".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_AZURE_OPENAI.to_string(),
                provider_name: "Azure OpenAI".to_string(),
                authenticated: false,
                auth_source: "api_key".to_string(),
                model: String::new(),
            },
        ];

        let defs = available_provider_defs(&auth);
        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_OPENAI));
        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_CUSTOM));
        assert!(!defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_AZURE_OPENAI));
        assert!(!defs.iter().any(|provider| provider.id == PROVIDER_ID_GROQ));
    }

    #[test]
    fn available_provider_defs_include_unauthenticated_custom_catalog_providers() {
        let mut auth = AuthState::new();
        auth.entries = vec![
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_GROQ.to_string(),
                provider_name: "Groq".to_string(),
                authenticated: false,
                auth_source: "api_key".to_string(),
                model: "llama".to_string(),
            },
            ProviderAuthEntry {
                provider_id: "local-openai".to_string(),
                provider_name: "Local OpenAI-Compatible".to_string(),
                authenticated: false,
                auth_source: "api_key".to_string(),
                model: "llama3.3".to_string(),
            },
        ];

        let entries = available_provider_defs(&auth);
        assert!(entries
            .iter()
            .any(|provider| provider.id == "local-openai"
                && provider.name == "Local OpenAI-Compatible"));
        assert!(!entries
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_GROQ));
    }

    #[test]
    fn provider_picker_handles_empty_state() {
        let modal = ModalState::new();
        let config = ConfigState::new();
        let _theme = ThemeTokens::default();
        assert_eq!(modal.picker_cursor(), 0);
        assert_eq!(config.provider(), PROVIDER_ID_OPENAI);
    }

    #[test]
    fn provider_picker_filters_visible_rows_by_query() {
        let mut auth = AuthState::new();
        auth.entries = vec![
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_OPENAI.to_string(),
                provider_name: "OpenAI".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "gpt-5.4".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_GROQ.to_string(),
                provider_name: "Groq".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "llama".to_string(),
            },
        ];
        let config = ConfigState::new();
        let mut modal = ModalState::new();
        modal.reduce(crate::state::modal::ModalAction::Push(
            crate::state::modal::ModalKind::ProviderPicker,
        ));
        modal.reduce(crate::state::modal::ModalAction::SetQuery("groq".into()));

        let screen = rendered_picker_text(&modal, &config, &auth);

        assert!(screen.contains("Groq"));
        assert!(!screen.contains("OpenAI"));
    }

    #[test]
    fn audio_provider_defs_filter_to_supported_authenticated_entries() {
        let mut auth = AuthState::new();
        auth.entries = vec![
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_OPENAI.to_string(),
                provider_name: "OpenAI".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "gpt-5.4".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_GROQ.to_string(),
                provider_name: "Groq".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "llama".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_OPENROUTER.to_string(),
                provider_name: "OpenRouter".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "openai/gpt-4o-mini-transcribe".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_XAI.to_string(),
                provider_name: "xAI".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "grok-4".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_ANTHROPIC.to_string(),
                provider_name: "Anthropic".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "claude".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_AZURE_OPENAI.to_string(),
                provider_name: "Azure OpenAI".to_string(),
                authenticated: false,
                auth_source: "api_key".to_string(),
                model: String::new(),
            },
        ];

        let defs = available_audio_provider_defs(&auth, AudioToolKind::SpeechToText);
        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_OPENAI));
        assert!(defs.iter().any(|provider| provider.id == PROVIDER_ID_GROQ));
        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_OPENROUTER));
        assert!(defs.iter().any(|provider| provider.id == PROVIDER_ID_XAI));
        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_CUSTOM));
        assert!(!defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_ANTHROPIC));
        assert!(!defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_AZURE_OPENAI));
    }

    #[test]
    fn embedding_provider_defs_include_authenticated_openrouter() {
        let mut auth = AuthState::new();
        auth.entries = vec![
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_OPENAI.to_string(),
                provider_name: "OpenAI".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "gpt-5.4".to_string(),
            },
            ProviderAuthEntry {
                provider_id: PROVIDER_ID_OPENROUTER.to_string(),
                provider_name: "OpenRouter".to_string(),
                authenticated: true,
                auth_source: "api_key".to_string(),
                model: "openai/text-embedding-3-small".to_string(),
            },
        ];

        let defs = available_embedding_provider_defs(&auth);

        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_OPENAI));
        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_OPENROUTER));
        assert!(defs
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_CUSTOM));
    }

    #[test]
    fn provider_filter_matches_name_id_and_default_model() {
        let providers = PROVIDERS.iter().collect::<Vec<_>>();

        assert!(filtered_provider_defs(providers.clone(), "openai")
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_OPENAI));
        assert!(filtered_provider_defs(providers.clone(), "gpt")
            .iter()
            .any(|provider| provider.id == PROVIDER_ID_OPENAI));
        assert!(filtered_provider_defs(providers, "no-such-provider").is_empty());
    }
}
