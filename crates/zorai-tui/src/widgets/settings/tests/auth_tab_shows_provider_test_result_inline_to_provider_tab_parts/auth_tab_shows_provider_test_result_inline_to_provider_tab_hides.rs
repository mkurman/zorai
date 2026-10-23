use crate::state::config::ConfigState;
use crate::state::modal::ModalState;
use crate::state::settings::{SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;
use crate::widgets::settings::render_gateway_tab;
use crate::widgets::settings::render_tab_content;
use crate::widgets::settings::{render_provider_tab, render_settings_page};
use zorai_shared::providers::*;
#[test]
fn auth_tab_shows_provider_test_result_inline() {
    let settings = SettingsState::new();
    let config = ConfigState::new();
    let modal = ModalState::new();
    let mut auth = crate::state::auth::AuthState::new();
    auth.loaded = true;
    auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    auth.reduce(crate::state::auth::AuthAction::ValidationResult {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        valid: true,
        error: None,
    });
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("Connection OK"),
        "expected inline test result in auth tab, got: {text}"
    );
}

#[test]
fn gateway_tab_shows_connected_whatsapp_status_and_split_actions() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Gateway,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(17));
    let mut config = ConfigState::new();
    config.whatsapp_allowed_contacts = "+48663977535".to_string();
    let mut modal = ModalState::new();
    modal.set_whatsapp_link_connected(Some("+48663977535".to_string()));

    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Link Status"));
    assert!(text.contains("Re-link Device"));
    assert!(text.contains("Linked: +48663977535"));
    assert!(text.contains("Only allowed numbers will be forwarded and can receive replies."));
}

#[test]
fn gateway_tab_renders_whatsapp_allowlist_as_textarea_when_editing() {
    let mut settings = SettingsState::new();
    settings.start_editing("whatsapp_allowed_contacts", "+15551234567\n+15557654321");
    let mut config = ConfigState::new();
    config.whatsapp_allowed_contacts = "+15551234567\n+15557654321".to_string();
    let modal = ModalState::new();

    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Allowed Contacts [Ctrl+S/Ctrl+Enter: save, Esc: cancel]"));
    assert!(text.contains("+15551234567"));
    assert!(text.contains("+15557654321"));
    assert!(text.contains("╭"));
    assert!(text.contains("╰"));
}

#[test]
fn custom_provider_model_field_invites_inline_edit() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Provider,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(3));
    let mut config = ConfigState::new();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.model = "my-model".to_string();

    let lines = render_provider_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Model           my-model [Enter: edit]"));
    assert!(!text.contains("> Model           my-model [Enter: pick]"));
}

#[test]
fn custom_provider_model_row_shows_active_edit_buffer() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Provider,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(3));
    settings.start_editing("custom_model_entry", "my-model");
    let mut config = ConfigState::new();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.model = "my-model".to_string();

    let lines = render_provider_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Model           my-model█"));
    assert!(!text.contains("> Provider        custom█"));
}

#[test]
fn memory_embedding_dimensions_row_shows_active_edit_buffer() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Memory,
    ));
    settings.start_editing("feat_embedding_dimensions", "3072");
    let mut config = ConfigState::new();
    config.agent_config_raw = Some(serde_json::json!({
        "semantic": {
            "embedding": {
                "dimensions": 1536
            }
        }
    }));

    let lines = render_settings_page(
        crate::state::settings_registry::settings_page_spec(SettingsTab::Memory)
            .expect("memory page"),
        &settings,
        &config,
        &ThemeTokens::default(),
    );
    let row = lines
        .iter()
        .map(|line| line.to_string())
        .find(|line| line.contains("Dimensions"))
        .expect("dimensions row");

    assert!(row.contains("3072\u{2588}"), "{row}");
    assert!(!row.contains("1536"), "{row}");
}

#[test]
fn custom_model_context_row_invites_edit_for_non_custom_provider() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Provider,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(7));
    let mut config = ConfigState::new();
    config.provider = "openrouter".to_string();
    config.auth_source = "api_key".to_string();
    config.model = "openrouter/custom-preview".to_string();
    config.custom_model_name = "Custom Preview".to_string();
    config.context_window_tokens = 333_000;
    config.custom_context_window_tokens = Some(333_000);

    let lines = render_provider_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Ctx Length      333000 tok [Enter: edit]"));
    assert!(!text.contains("[derived]"));
}

#[test]
fn subagent_editor_shows_live_name_edit_buffer() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::SubAgents,
    ));
    settings.start_editing("subagent_name", "Draft Name");
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let mut subagents = SubAgentsState::new();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        0,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4".to_string(),
    );
    editor.name = "Old Name".to_string();
    editor.field = crate::state::subagents::SubAgentEditorField::Name;
    subagents.editor = Some(editor);
    let concierge = crate::state::concierge::ConciergeState::new();
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        80,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Draft Name█"));
    assert!(!text.contains("Old Name"));
}

#[test]
fn subagent_editor_shows_live_model_edit_buffer() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::SubAgents,
    ));
    settings.start_editing("subagent_model", "gpt-5.4-minix");
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let mut subagents = SubAgentsState::new();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        0,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4".to_string(),
    );
    editor.field = crate::state::subagents::SubAgentEditorField::Model;
    subagents.editor = Some(editor);
    let concierge = crate::state::concierge::ConciergeState::new();
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        80,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("gpt-5.4-minix█"));
    assert!(!text.contains("> Model          gpt-5.4"));
}

#[test]
fn subagent_editor_shows_openrouter_rows_only_for_openrouter_provider() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::SubAgents,
    ));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let mut subagents = SubAgentsState::new();
    let openai_editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        0,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4".to_string(),
    );
    subagents.editor = Some(openai_editor);
    let openai_text = render_tab_content(
        80,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &plugin_settings,
        &ThemeTokens::default(),
    )
    .iter()
    .map(|line| line.to_string())
    .collect::<Vec<_>>()
    .join("\n");
    assert!(!openai_text.contains("OR Prefer"));

    let mut openrouter_editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        0,
        PROVIDER_ID_OPENROUTER.to_string(),
        "openai/gpt-5.4".to_string(),
    );
    openrouter_editor.openrouter_provider_order = "openai".to_string();
    subagents.editor = Some(openrouter_editor);
    let openrouter_text = render_tab_content(
        80,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &plugin_settings,
        &ThemeTokens::default(),
    )
    .iter()
    .map(|line| line.to_string())
    .collect::<Vec<_>>()
    .join("\n");
    assert!(openrouter_text.contains("OR Prefer"));
    assert!(openrouter_text.contains("openai"));
}

#[test]
fn subagent_system_prompt_textarea_wraps_long_lines_to_content_width() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::SubAgents,
    ));
    settings.start_editing(
        "subagent_system_prompt",
        "You are zorai, an always-on agentic runtime assistant. You can execute terminal commands and coordinate subagents carefully.",
    );
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let mut subagents = SubAgentsState::new();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        0,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4".to_string(),
    );
    editor.field = crate::state::subagents::SubAgentEditorField::SystemPrompt;
    subagents.editor = Some(editor);
    let concierge = crate::state::concierge::ConciergeState::new();
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        60,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let rendered_lines = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let prompt_lines = rendered_lines
        .iter()
        .filter(|line| line.starts_with("  │ "))
        .collect::<Vec<_>>();

    assert!(
        prompt_lines.len() >= 2,
        "expected wrapped textarea content, got: {:?}",
        prompt_lines
    );
    assert!(
        prompt_lines
            .iter()
            .all(|line| unicode_width::UnicodeWidthStr::width(line.as_str()) <= 60),
        "wrapped textarea should stay within content width, got: {:?}",
        prompt_lines
    );
}

#[test]
fn provider_tab_hides_openrouter_rows_for_non_openrouter_provider() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Provider,
    ));
    let mut config = ConfigState::new();
    config.provider = PROVIDER_ID_OPENAI.to_string();

    let lines = render_provider_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(!text.contains("OR Prefer"));
    assert!(!text.contains("OR Exclude"));
    assert!(!text.contains("OR Fallbacks"));
}
