use super::whatsapp_modal_esc_sends_stop_and_closes_to_clicking_rendered_settings::*;
use crate::app::*;
use zorai_shared::providers::*;
#[test]
fn audio_model_picker_render_uses_same_filtered_models_as_selection() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 120;
    model.height = 40;
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-audio-mini"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "anthropic/claude-opus-4.6".to_string(),
                name: Some("Anthropic: Claude Opus 4.6".to_string()),
                context_window: Some(1_000_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    prompt: Some("0.000015".to_string()),
                    completion: Some("0.000075".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text"],
                        "output_modalities": ["text"]
                    }
                })),
            },
            crate::state::config::FetchedModel {
                id: "xiaomi/mimo-v2-omni".to_string(),
                name: Some("Xiaomi: MiMo-V2-Omni".to_string()),
                context_window: Some(262_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text"]
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));

    let screen = render_screen(&mut model).join("\n");

    assert!(
        !screen.contains("Anthropic: Claude Opus 4.6"),
        "audio picker should not render text-only fetched models"
    );
    assert!(
        screen.contains("Xiaomi: MiMo-V2-Omni"),
        "audio picker should render audio-capable fetched models"
    );
}

#[test]
fn protected_weles_editor_can_open_provider_model_role_and_effort_pickers() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];

    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("weles_builtin".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4-mini".to_string(),
    );
    editor.name = "WELES".to_string();
    editor.builtin = true;
    editor.immutable_identity = true;
    editor.disable_allowed = false;
    editor.delete_allowed = false;
    editor.reasoning_effort = Some("medium".to_string());
    editor.field = crate::state::subagents::SubAgentEditorField::Provider;
    model.subagents.editor = Some(editor.clone());
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));

    model.close_top_modal();
    if let Some(editor) = model.subagents.editor.as_mut() {
        editor.field = crate::state::subagents::SubAgentEditorField::Model;
    }
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));

    model.close_top_modal();
    if let Some(editor) = model.subagents.editor.as_mut() {
        editor.field = crate::state::subagents::SubAgentEditorField::Role;
    }
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::RolePicker));

    model.close_top_modal();
    if let Some(editor) = model.subagents.editor.as_mut() {
        editor.field = crate::state::subagents::SubAgentEditorField::ReasoningEffort;
    }
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::EffortPicker));
}

#[test]
fn subagent_role_picker_applies_selected_role_preset() {
    let (mut model, _daemon_rx) = make_model();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("worker".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4-mini".to_string(),
    );
    editor.field = crate::state::subagents::SubAgentEditorField::Role;
    model.subagents.editor = Some(editor);
    model.settings_picker_target = Some(SettingsPickerTarget::SubAgentRole);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::RolePicker));

    let planning_index = crate::state::subagents::SUBAGENT_ROLE_PRESETS
        .iter()
        .position(|preset| preset.id == "planning")
        .expect("planning preset should exist");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(planning_index as i32));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::RolePicker,
    );

    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert_eq!(
        model
            .subagents
            .editor
            .as_ref()
            .map(|editor| editor.role.as_str()),
        Some("planning")
    );
    assert_eq!(
        model
            .subagents
            .editor
            .as_ref()
            .map(|editor| editor.system_prompt.as_str()),
        crate::state::subagents::find_role_preset("planning").map(|preset| preset.system_prompt)
    );
    assert_eq!(model.status_line, "Sub-agent role: Planning");
    assert!(!model.settings.is_editing());
}

#[test]
fn subagent_role_picker_custom_option_starts_inline_edit() {
    let (mut model, _daemon_rx) = make_model();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("worker".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4-mini".to_string(),
    );
    editor.field = crate::state::subagents::SubAgentEditorField::Role;
    editor.role = "my_custom_role".to_string();
    model.subagents.editor = Some(editor);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.settings_picker_target = Some(SettingsPickerTarget::SubAgentRole);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::RolePicker));
    model.modal.reduce(modal::ModalAction::Navigate(
        crate::state::subagents::role_picker_custom_index() as i32,
    ));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::RolePicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Settings));
    assert_eq!(model.settings.editing_field(), Some("subagent_role"));
    assert_eq!(model.settings.edit_buffer(), "my_custom_role");
    assert_eq!(model.status_line, "Enter sub-agent role ID");
}

#[test]
fn enter_on_honcho_memory_opens_inline_editor() {
    let (mut model, _daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Memory));
    model
        .settings
        .navigate_field(0, model.settings_field_count());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert!(model.config.honcho_editor.is_some());
}

#[test]
fn honcho_editor_save_updates_config() {
    let (mut model, _daemon_rx) = make_model();
    model.config.enable_honcho_memory = false;
    model.config.honcho_workspace_id = "zorai".to_string();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Memory));
    model
        .settings
        .navigate_field(0, model.settings_field_count());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    let _ = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    let editor = model
        .config
        .honcho_editor
        .as_mut()
        .expect("honcho editor should be open");
    editor.enabled = true;
    editor.api_key = "hc_test".to_string();
    editor.base_url = "https://honcho.example".to_string();
    editor.workspace_id = "zorai-lab".to_string();
    editor.field = crate::state::config::HonchoEditorField::Save;

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert!(model.config.honcho_editor.is_none());
    assert!(model.config.enable_honcho_memory);
    assert_eq!(model.config.honcho_api_key, "hc_test");
    assert_eq!(model.config.honcho_base_url, "https://honcho.example");
    assert_eq!(model.config.honcho_workspace_id, "zorai-lab");
}

#[test]
fn honcho_editor_cancel_discards_staged_values() {
    let (mut model, _daemon_rx) = make_model();
    model.config.enable_honcho_memory = false;
    model.config.honcho_api_key = "persisted".to_string();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Memory));
    model
        .settings
        .navigate_field(0, model.settings_field_count());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    let _ = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    let editor = model
        .config
        .honcho_editor
        .as_mut()
        .expect("honcho editor should be open");
    editor.enabled = true;
    editor.api_key = "staged".to_string();

    let quit = model.handle_key_modal(KeyCode::Esc, KeyModifiers::NONE, modal::ModalKind::Settings);

    assert!(!quit);
    assert!(model.config.honcho_editor.is_none());
    assert!(!model.config.enable_honcho_memory);
    assert_eq!(model.config.honcho_api_key, "persisted");
}

#[test]
fn honcho_editor_space_toggles_staged_enabled_only() {
    let (mut model, _daemon_rx) = make_model();
    model.config.enable_honcho_memory = false;
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Memory));
    model
        .settings
        .navigate_field(0, model.settings_field_count());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    let _ = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    let quit = model.handle_key_modal(
        KeyCode::Char(' '),
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert!(!model.config.enable_honcho_memory);
    assert!(model
        .config
        .honcho_editor
        .as_ref()
        .is_some_and(|editor| editor.enabled));
}

#[test]
fn subagent_model_picker_uses_subagent_current_model_instead_of_primary_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.model = "gpt-5.4".to_string();
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));

    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("weles_builtin".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "claude-sonnet-4-5".to_string(),
    );
    editor.name = "WELES".to_string();
    editor.builtin = true;
    editor.immutable_identity = true;
    model.subagents.editor = Some(editor);
    model.settings_picker_target = Some(SettingsPickerTarget::SubAgentModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(
        model
            .subagents
            .editor
            .as_ref()
            .map(|editor| editor.model.as_str()),
        Some("claude-sonnet-4-5")
    );
    assert_eq!(model.config.model, "gpt-5.4");
}

#[test]
fn subagent_custom_model_entry_does_not_mutate_primary_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.model = "gpt-5.4".to_string();
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));

    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("weles_builtin".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4-mini".to_string(),
    );
    editor.name = "WELES".to_string();
    editor.builtin = true;
    editor.immutable_identity = true;
    editor.field = crate::state::subagents::SubAgentEditorField::Model;
    model.subagents.editor = Some(editor);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.settings_picker_target = Some(SettingsPickerTarget::SubAgentModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.settings.editing_field(), Some("subagent_model"));
    model.settings.reduce(SettingsAction::InsertChar('x'));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(
        model
            .subagents
            .editor
            .as_ref()
            .map(|editor| editor.model.as_str()),
        Some("gpt-5.4-minix")
    );
    assert_eq!(model.config.model, "gpt-5.4");
}
