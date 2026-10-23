use super::super::make_model;
use super::opening_weles_editor_hides_inherited_main_system_prompt_to_feat_skill::focus_settings_field;
use super::*;
use crate::state::settings::SettingsTab;
use crate::widgets;
use zorai_shared::providers::{
    PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENAI, PROVIDER_ID_OPENROUTER, PROVIDER_ID_XAI,
};

#[test]
fn whatsapp_link_device_probes_status_before_starting_link_flow() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.config.whatsapp_allowed_contacts = "+48663977535".to_string();
    model.settings.reduce(SettingsAction::NavigateField(17));
    assert_eq!(model.settings.current_field_name(), "whatsapp_link_device");

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected subscribe command"),
        DaemonCommand::WhatsAppLinkSubscribe
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected status probe"),
        DaemonCommand::WhatsAppLinkStatus
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected start command"),
        DaemonCommand::WhatsAppLinkStart
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));
}

#[test]
fn database_sync_now_action_sends_sync_command() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Database));
    assert_eq!(model.settings.current_field_name(), "db_sync_now");

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected database sync command"),
        DaemonCommand::DatabaseSyncNow
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn whatsapp_link_device_does_not_reset_existing_link() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.config.whatsapp_allowed_contacts = "+48663977535".to_string();
    model.settings.reduce(SettingsAction::NavigateField(17));
    model
        .modal
        .set_whatsapp_link_connected(Some("+48663977535".to_string()));

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected subscribe command"),
        DaemonCommand::WhatsAppLinkSubscribe
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected status command"),
        DaemonCommand::WhatsAppLinkStatus
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));
    assert_eq!(model.status_line, "Showing WhatsApp link status");
}

#[test]
fn whatsapp_relink_device_resets_existing_link_before_restart() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.config.whatsapp_allowed_contacts = "+48663977535".to_string();
    model.settings.reduce(SettingsAction::NavigateField(18));
    model
        .modal
        .set_whatsapp_link_connected(Some("+48663977535".to_string()));

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected subscribe command"),
        DaemonCommand::WhatsAppLinkSubscribe
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected status command"),
        DaemonCommand::WhatsAppLinkStatus
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected reset command"),
        DaemonCommand::WhatsAppLinkReset
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected start command"),
        DaemonCommand::WhatsAppLinkStart
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn activating_model_for_custom_provider_starts_inline_custom_model_edit() {
    let (mut model, _daemon_rx) = make_model();
    model.apply_provider_selection(PROVIDER_ID_CUSTOM);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
    model.settings.reduce(SettingsAction::NavigateField(3));
    assert_eq!(model.settings.current_field_name(), "model");

    model.activate_settings_field();

    assert_eq!(model.settings.editing_field(), Some("custom_model_entry"));
    assert_eq!(model.settings.field_cursor(), 3);
    assert_ne!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn activating_context_length_for_custom_model_starts_inline_edit() {
    let (mut model, _daemon_rx) = make_model();
    model.config.provider = "openrouter".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.model = "openrouter/custom-preview".to_string();
    model.config.custom_model_name = "Custom Preview".to_string();
    model.config.context_window_tokens = 333_000;
    model.config.custom_context_window_tokens = Some(333_000);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
    model.settings.reduce(SettingsAction::NavigateField(7));
    assert_eq!(model.settings.current_field_name(), "context_window_tokens");

    model.activate_settings_field();

    assert_eq!(
        model.settings.editing_field(),
        Some("context_window_tokens")
    );
    assert_eq!(model.settings.edit_buffer(), "333000");
}

#[test]
fn activating_message_loop_delay_starts_inline_edit() {
    let (mut model, _daemon_rx) = make_model();
    focus_settings_field(&mut model, SettingsTab::Execution, "message_loop_delay_ms");

    assert_eq!(
        model.settings.current_field_name_with_config(&model.config),
        "message_loop_delay_ms"
    );

    model.activate_settings_field();

    assert_eq!(
        model.settings.editing_field(),
        Some("message_loop_delay_ms")
    );
    assert_eq!(model.settings.edit_buffer(), "500");
}

#[test]
fn activating_compaction_weles_provider_opens_provider_picker() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.config.compaction_strategy = "weles".to_string();
    focus_settings_field(
        &mut model,
        SettingsTab::Compaction,
        "compaction_weles_provider",
    );

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));
}

#[test]
fn activating_compaction_weles_model_opens_model_picker() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.config.compaction_strategy = "weles".to_string();
    model.config.compaction_weles_provider = PROVIDER_ID_OPENAI.to_string();
    focus_settings_field(
        &mut model,
        SettingsTab::Compaction,
        "compaction_weles_model",
    );

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn activating_compaction_custom_provider_opens_provider_picker() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.config.compaction_strategy = "custom_model".to_string();
    focus_settings_field(
        &mut model,
        SettingsTab::Compaction,
        "compaction_custom_provider",
    );

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));
}

#[test]
fn activating_compaction_custom_model_opens_model_picker() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.config.compaction_strategy = "custom_model".to_string();
    model.config.compaction_custom_provider = PROVIDER_ID_OPENAI.to_string();
    focus_settings_field(
        &mut model,
        SettingsTab::Compaction,
        "compaction_custom_model",
    );

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn activating_audio_stt_provider_opens_provider_picker() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_OPENAI.to_string(),
            provider_name: "OpenAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "gpt-5.4".to_string(),
        },
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_XAI.to_string(),
            provider_name: "xAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "grok-4".to_string(),
        },
    ];
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    focus_settings_field(&mut model, SettingsTab::Media, "feat_audio_stt_provider");

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));
    assert!(widgets::provider_picker::available_audio_provider_defs(
        &model.auth,
        zorai_shared::providers::AudioToolKind::SpeechToText,
    )
    .iter()
    .any(|provider| provider.id == PROVIDER_ID_XAI));
}

#[test]
fn activating_audio_stt_model_opens_model_picker() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    focus_settings_field(&mut model, SettingsTab::Media, "feat_audio_stt_model");

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn activating_audio_tts_provider_opens_provider_picker() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_OPENAI.to_string(),
            provider_name: "OpenAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "gpt-5.4".to_string(),
        },
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_XAI.to_string(),
            provider_name: "xAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "grok-4".to_string(),
        },
    ];
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    focus_settings_field(&mut model, SettingsTab::Media, "feat_audio_tts_provider");

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));
    assert!(widgets::provider_picker::available_audio_provider_defs(
        &model.auth,
        zorai_shared::providers::AudioToolKind::TextToSpeech,
    )
    .iter()
    .any(|provider| provider.id == PROVIDER_ID_XAI));
}

#[test]
fn activating_audio_tts_model_opens_model_picker() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "tts": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-4o-mini-tts"
            }
        }
    }));
    focus_settings_field(&mut model, SettingsTab::Media, "feat_audio_tts_model");

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn activating_audio_stt_model_fetches_remote_models_for_audio_provider() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "providers": {
            PROVIDER_ID_OPENROUTER: {
                "base_url": "https://openrouter.ai/api/v1",
                "api_key": "router-key",
                "auth_source": "api_key"
            }
        },
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-audio"
            }
        }
    }));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    focus_settings_field(&mut model, SettingsTab::Media, "feat_audio_stt_model");

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::FetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        }) => {
            assert_eq!(provider_id, PROVIDER_ID_OPENROUTER);
            assert_eq!(base_url, "https://openrouter.ai/api/v1");
            assert_eq!(api_key, "router-key");
            assert_eq!(output_modalities.as_deref(), Some("transcription"));
        }
        other => panic!("expected FetchModels for audio STT picker, got {other:?}"),
    }
}

#[test]
fn activating_audio_tts_model_fetches_remote_models_for_audio_provider() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.base_url = "https://api.openai.com/v1".to_string();
    model.config.api_key = "openai-key".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "tts": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-4o-mini-tts"
            }
        }
    }));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    focus_settings_field(&mut model, SettingsTab::Media, "feat_audio_tts_model");

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::FetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        }) => {
            assert_eq!(provider_id, PROVIDER_ID_OPENAI);
            assert_eq!(base_url, "https://api.openai.com/v1");
            assert_eq!(api_key, "openai-key");
            assert_eq!(output_modalities, None);
        }
        other => panic!("expected FetchModels for audio TTS picker, got {other:?}"),
    }
}
