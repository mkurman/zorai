use crate::state::config::ConfigState;
use crate::state::modal::ModalState;
use crate::state::settings::{PluginSettingsState, SettingsAction, SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;
use crate::widgets::settings::render_tab_content;
use crate::widgets::settings::{active_tab_index, mask_api_key, visible_tabs};
use crate::widgets::settings::{render, render_tabs_line};
use crate::widgets::settings::{render_gateway_tab, tab_hit_test};
use ratatui::layout::Rect;
use zorai_shared::providers::PROVIDER_ID_OPENAI;

#[test]
fn settings_handles_empty_state() {
    let settings = SettingsState::new();
    let config = ConfigState::new();
    let _theme = ThemeTokens::default();
    assert_eq!(settings.active_tab(), SettingsTab::Auth);
    assert_eq!(config.model(), "gpt-5.5");
}

#[test]
fn settings_api_key_is_masked() {
    let masked = mask_api_key("sk-abcdefgh12345678abcd");
    assert!(!masked.contains("abcdefgh"));
    assert!(masked.contains("\u{2022}"));
}

#[test]
fn mask_api_key_short_returns_dots() {
    assert_eq!(
        mask_api_key("short"),
        "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
    );
}

#[test]
fn mask_api_key_empty_returns_not_set() {
    assert_eq!(mask_api_key(""), "(not set)");
}

#[test]
fn tab_hit_test_uses_rendered_label_positions() {
    let area = Rect::new(10, 3, 80, 1);
    let visible = visible_tabs(area, active_tab_index(SettingsTab::Concierge));
    assert!(visible.iter().any(|tab| tab.tab == SettingsTab::Concierge));
    for tab in visible {
        assert_eq!(
            tab_hit_test(area, SettingsTab::Concierge, tab.start_x),
            Some(tab.tab)
        );
    }
}

#[test]
fn gateway_tab_mentions_whatsapp_qr_linking_instructions() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Gateway,
    ));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Allowed Contacts accepts comma or newline separated phone numbers."),
        "Gateway tab should explain how to enter the WhatsApp allowlist"
    );
}

#[test]
fn gateway_tab_shows_allowlist_requirement_before_linking() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Gateway,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(17));
    let config = ConfigState::new();
    let modal = ModalState::new();

    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Link Device  [Enter]  (requires allowed contacts)"));
    assert!(text.contains("Add at least one allowed phone number before QR linking."));
}

#[test]
fn plugins_tab_shows_install_shortcut_in_list_mode() {
    let mut settings = SettingsState::new();
    settings.reduce(SettingsAction::SwitchTab(SettingsTab::Plugins));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("base");
    let plugin_settings = PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Press i to install a plugin."));
    assert!(text.contains("zorai plugin add <source>"));
}

#[test]
fn settings_footer_omits_select_hint_for_read_only_advanced_field() {
    let mut settings = SettingsState::new();
    settings.reduce(SettingsAction::SwitchTab(SettingsTab::Advanced));
    for _ in 0..20 {
        settings.reduce(SettingsAction::NavigateField(1));
    }
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("base");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();
    let theme = ThemeTokens::default();
    let backend = ratatui::backend::TestBackend::new(100, 16);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal should initialize");

    terminal
        .draw(|frame| {
            render(
                frame,
                Rect::new(0, 0, 100, 16),
                &settings,
                &config,
                &modal,
                &auth,
                &subagents,
                &concierge,
                &tier,
                &plugin_settings,
                0,
                &theme,
            );
        })
        .expect("render should succeed");

    let footer = (0..100)
        .filter_map(|x| {
            terminal
                .backend()
                .buffer()
                .cell((x, 14))
                .map(|cell| cell.symbol())
        })
        .collect::<String>();
    assert!(footer.contains("navigate"));
    assert!(!footer.contains("edit/select"), "{footer}");
    assert!(!footer.contains("toggle"), "{footer}");
}

#[test]
fn settings_tab_bar_uses_swarog_and_rarog_labels() {
    let area = Rect::new(0, 0, 120, 1);
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Concierge,
    ));
    let tabs = visible_tabs(area, active_tab_index(SettingsTab::Concierge));
    let line = render_tabs_line(&tabs, &settings, &ThemeTokens::default());
    let text = line.to_string();

    assert!(text.contains("Svar"));
    assert!(text.contains("Rar"));
    assert!(!text.contains("Con"));
    assert!(!text.contains("Prov"));
    let auth_idx = text.find("Auth").expect("auth tab should be visible");
    let swar_idx = text.find("Svar").expect("svarog tab should be visible");
    let rar_idx = text.find("Rar").expect("rarog tab should be visible");
    assert!(auth_idx < swar_idx && swar_idx < rar_idx);
}

#[test]
fn settings_tab_bar_includes_about_label() {
    let area = Rect::new(0, 0, 140, 1);
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::About,
    ));

    let tabs = visible_tabs(area, active_tab_index(SettingsTab::About));
    let line = render_tabs_line(&tabs, &settings, &ThemeTokens::default());
    let text = line.to_string();

    assert!(text.contains("About"));
}

#[test]
fn chat_tab_keeps_honcho_as_single_row_in_main_list() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Chat,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(2));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Honcho Memory"));
    assert!(!text.contains("Honcho API Key"));
    assert!(!text.contains("Honcho Base URL"));
    assert!(!text.contains("Honcho Workspace"));
}

#[test]
fn chat_tab_renders_inline_honcho_editor_when_active() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Chat,
    ));
    let mut config = ConfigState::new();
    config.honcho_editor = Some(crate::state::config::HonchoEditorState {
        enabled: true,
        api_key: "hc_test".to_string(),
        base_url: "https://honcho.example".to_string(),
        workspace_id: "zorai-lab".to_string(),
        field: crate::state::config::HonchoEditorField::ApiKey,
    });
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Honcho Memory Settings"));
    assert!(text.contains("API Key"));
    assert!(text.contains("Base URL"));
    assert!(text.contains("Workspace"));
    assert!(text.contains("[Save]"));
    assert!(text.contains("[Cancel]"));
}

#[test]
fn chat_tab_places_honcho_editor_below_last_checkbox() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Chat,
    ));
    let mut config = ConfigState::new();
    config.honcho_editor = Some(crate::state::config::HonchoEditorState {
        enabled: false,
        api_key: String::new(),
        base_url: String::new(),
        workspace_id: "zorai".to_string(),
        field: crate::state::config::HonchoEditorField::Enabled,
    });
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    let require_activation = text
        .find("Require Activation")
        .expect("require activation row should render");
    let honcho_editor = text
        .find("Honcho Memory Settings")
        .expect("honcho editor heading should render");
    let tool_limit = text
        .find("Tool Limit:")
        .expect("tool limit row should render");

    assert!(require_activation < honcho_editor);
    assert!(honcho_editor < tool_limit);
}

#[test]
fn about_tab_renders_product_metadata() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::About,
    ));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Version:"));
    assert!(text.contains("Mariusz Kurman"));
    assert!(text.contains("mkurman/zorai"));
    assert!(text.contains("zorai.app"));
}

#[test]
fn gateway_tab_contains_selectable_link_device_row() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Gateway,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(17));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("> Link Device"));
}

#[test]
fn advanced_tab_renders_sleep_delay_rows() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Advanced,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(7));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Message Loop (ms):"));
    assert!(text.contains("Tool Call Gap (ms):"));
    assert!(text.contains("LLM Stream Timeout (s):"));
}

#[test]
fn auth_tab_shows_chatgpt_logout_when_daemon_auth_is_available() {
    let settings = SettingsState::new();
    let mut config = ConfigState::new();
    config.chatgpt_auth_available = true;
    let modal = ModalState::new();
    let mut auth = crate::state::auth::AuthState::new();
    auth.loaded = true;
    auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: false,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("[API Key] [Logout]"),
        "expected ChatGPT action to switch to logout, got: {text}"
    );
}
