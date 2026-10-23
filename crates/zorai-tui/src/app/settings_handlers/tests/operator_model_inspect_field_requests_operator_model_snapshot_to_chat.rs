use super::make_model;
use super::whatsapp_link_device_probes_status_before_starting_link_flow::focus_settings_field;
use super::*;
#[test]
fn operator_model_inspect_field_requests_operator_model_snapshot() {
    let (mut model, mut daemon_rx) = make_model();
    focus_settings_field(
        &mut model,
        SettingsTab::Proactivity,
        "operator_model_inspect",
    );
    assert_eq!(
        model.settings.current_field_name(),
        "operator_model_inspect"
    );

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected inspect command"),
        DaemonCommand::GetOperatorModel
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn operator_model_reset_field_requests_model_reset() {
    let (mut model, mut daemon_rx) = make_model();
    focus_settings_field(&mut model, SettingsTab::Proactivity, "operator_model_reset");
    assert_eq!(model.settings.current_field_name(), "operator_model_reset");

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected reset command"),
        DaemonCommand::ResetOperatorModel
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn chat_settings_history_page_size_allows_twenty_messages() {
    let (mut model, _daemon_rx) = make_model();
    focus_settings_field(&mut model, SettingsTab::Chat, "tui_chat_history_page_size");
    model.config.tui_chat_history_page_size = 100;

    model.activate_settings_field();
    assert_eq!(
        model.settings.editing_field(),
        Some("tui_chat_history_page_size")
    );

    for _ in 0..3 {
        model.settings.reduce(SettingsAction::Backspace);
    }
    model.settings.reduce(SettingsAction::InsertChar('2'));
    model.settings.reduce(SettingsAction::InsertChar('0'));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(model.config.tui_chat_history_page_size, 20);
    assert_eq!(model.settings.editing_field(), None);
}

#[test]
fn chat_settings_tool_limit_enters_edit_mode_on_enter() {
    let (mut model, _daemon_rx) = make_model();
    focus_settings_field(
        &mut model,
        SettingsTab::Tools,
        "tool_synthesis_max_generated_tools",
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(
        model.settings.editing_field(),
        Some("tool_synthesis_max_generated_tools")
    );
}

#[test]
fn websearch_provider_cycle_includes_duckduckgo() {
    let (mut model, _daemon_rx) = make_model();
    focus_settings_field(&mut model, SettingsTab::WebSearch, "search_provider");
    model.config.search_provider = "firecrawl".to_string();

    model.activate_settings_field();

    assert_eq!(model.config.search_provider, "duckduckgo");
}

#[test]
fn websearch_duckduckgo_safe_search_cycle_is_editable() {
    let (mut model, _daemon_rx) = make_model();
    model.config.search_provider = "duckduckgo".to_string();
    focus_settings_field(&mut model, SettingsTab::WebSearch, "duckduckgo_safe_search");
    model.config.duckduckgo_safe_search = "moderate".to_string();

    model.activate_settings_field();

    assert_eq!(model.config.duckduckgo_safe_search, "strict");
}
