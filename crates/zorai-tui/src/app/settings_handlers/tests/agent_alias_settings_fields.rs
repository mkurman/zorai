use super::make_model;
use super::whatsapp_link_device_probes_status_before_starting_link_flow::focus_settings_field;
use super::*;

fn type_and_commit(model: &mut TuiModel, text: &str) {
    for ch in text.chars() {
        model.handle_key_modal(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
    }
    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
}

fn drain(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Vec<DaemonCommand> {
    let mut commands = Vec::new();
    while let Ok(command) = daemon_rx.try_recv() {
        commands.push(command);
    }
    commands
}

fn alias_items(commands: &[DaemonCommand]) -> Vec<(String, String)> {
    commands
        .iter()
        .filter_map(|command| match command {
            DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            } if key_path.starts_with("/agent_aliases/") => {
                Some((key_path.clone(), value_json.clone()))
            }
            _ => None,
        })
        .collect()
}

fn weles_builtin_entry() -> crate::state::SubAgentEntry {
    crate::state::SubAgentEntry {
        claude_permission_mode: None,
        id: "weles_builtin".to_string(),
        name: "Weles".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("governance".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Daemon-owned WELES registry entry".to_string()),
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: Some(serde_json::json!({
            "id": "weles_builtin",
            "name": "Weles",
            "provider": PROVIDER_ID_OPENAI,
            "model": "gpt-5.4-mini",
            "builtin": true,
            "immutable_identity": true,
            "disable_allowed": false,
            "delete_allowed": false,
            "protected_reason": "Daemon-owned WELES registry entry"
        })),
    }
}

#[test]
fn renaming_weles_in_subagent_editor_sets_alias_without_renaming_protected_definition() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.agent_config_raw = Some(serde_json::json!({}));
    model.subagents.entries = vec![weles_builtin_entry()];
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
    model.open_subagent_editor_existing();
    drain(&mut daemon_rx);

    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert_eq!(model.settings.editing_field(), Some("weles_alias"));
    type_and_commit(&mut model, "Judge");

    let commands = drain(&mut daemon_rx);
    assert_eq!(
        alias_items(&commands),
        vec![("/agent_aliases/weles".to_string(), "\"Judge\"".to_string())]
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, DaemonCommand::SetSubAgent(_))),
        "daemon rejects WELES renames, so the alias must not be sent as a sub-agent update"
    );
    assert!(commands
        .iter()
        .any(|command| matches!(command, DaemonCommand::GetConfig)));
    assert_eq!(
        model.subagents.editor.as_ref().map(|e| e.name.as_str()),
        Some("Weles")
    );
    assert_eq!(crate::agent_aliases::display_agent_name("Weles"), "Judge");
}

#[test]
fn rarog_alias_field_in_concierge_tab_writes_rarog_alias_item() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.agent_config_raw = Some(serde_json::json!({}));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    focus_settings_field(&mut model, SettingsTab::Concierge, "rarog_alias");
    assert_eq!(model.current_settings_field_name(), "rarog_alias");

    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert_eq!(model.settings.editing_field(), Some("rarog_alias"));
    type_and_commit(&mut model, "Scout");

    let commands = drain(&mut daemon_rx);
    assert_eq!(
        alias_items(&commands),
        vec![("/agent_aliases/rarog".to_string(), "\"Scout\"".to_string())]
    );
    assert_eq!(
        model.config.agent_config_raw.as_ref().unwrap()["agent_aliases"]["rarog"],
        "Scout"
    );
    assert_eq!(crate::agent_aliases::display_agent_name("Rarog"), "Scout");
}
