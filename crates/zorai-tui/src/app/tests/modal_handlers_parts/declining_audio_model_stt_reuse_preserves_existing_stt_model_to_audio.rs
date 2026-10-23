use super::whatsapp_modal_esc_sends_stop_and_closes_to_clicking_rendered_settings::*;
use crate::app::*;
use zorai_shared::providers::*;

#[test]
fn elevenlabs_audio_model_picker_uses_static_stt_and_tts_models() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_ELEVENLABS,
                "model": "scribe_v2"
            },
            "tts": {
                "provider": PROVIDER_ID_ELEVENLABS,
                "model": "eleven_multilingual_v2"
            }
        }
    }));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    let stt_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    let tts_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert_eq!(stt_models, vec!["scribe_v2"]);
    assert_eq!(tts_models, vec!["eleven_multilingual_v2"]);
}

#[test]
fn declining_audio_model_stt_reuse_preserves_existing_stt_model() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.model.clear();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-4o-audio-preview".to_string(),
                name: Some("GPT-4o Audio Preview".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text", "audio"]
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::Model);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    navigate_model_picker_to(&mut model, "gpt-4o-audio-preview");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

    let quit = model.handle_key_modal(
        KeyCode::Tab,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    assert!(!quit);

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("provider"))
            .and_then(|value| value.as_str()),
        Some(PROVIDER_ID_OPENAI)
    );
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("model"))
            .and_then(|value| value.as_str()),
        Some("whisper-1")
    );

    let commands = collect_daemon_commands(&mut daemon_rx);
    assert!(!commands.iter().any(|command| {
        matches!(
            command,
            DaemonCommand::SetConfigItem { key_path, .. } if key_path == "/audio/stt/model"
        )
    }));
}

#[test]
fn audio_stt_custom_model_entry_keeps_audio_field_selected() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Media));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "whisper-1".to_string(),
                name: Some("Whisper 1".to_string()),
                context_window: None,
                pricing: None,
                metadata: None,
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    let custom_row = model.available_model_picker_models().len();
    model
        .modal
        .reduce(modal::ModalAction::Navigate(custom_row as i32));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.settings.active_tab(), SettingsTab::Media);
    assert_eq!(model.settings.field_cursor(), 2);
    assert_eq!(model.settings.editing_field(), Some("feat_audio_stt_model"));
}

#[test]
fn audio_tts_custom_model_entry_keeps_audio_field_selected() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "tts": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-4o-mini-tts"
            }
        }
    }));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Media));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-4o-mini-tts".to_string(),
                name: Some("GPT-4o Mini TTS".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    let custom_row = model.available_model_picker_models().len();
    model
        .modal
        .reduce(modal::ModalAction::Navigate(custom_row as i32));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.settings.active_tab(), SettingsTab::Media);
    assert_eq!(model.settings.field_cursor(), 6);
    assert_eq!(model.settings.editing_field(), Some("feat_audio_tts_model"));
}

#[test]
fn audio_model_picker_filters_fetched_models_to_audio_capable_entries() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-audio"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/gpt-audio".to_string(),
                name: Some("GPT Audio".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text", "audio"]
                    }
                })),
            },
            crate::state::config::FetchedModel {
                id: "openai/gpt-text".to_string(),
                name: Some("GPT Text".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    prompt: Some("0.000002".to_string()),
                    completion: Some("0.000008".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text"],
                        "output_modalities": ["text"]
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);

    let models = model.available_model_picker_models();

    assert!(models.iter().any(|model| model.id == "openai/gpt-audio"));
    assert!(!models.iter().any(|model| model.id == "openai/gpt-text"));
}

#[test]
fn audio_model_picker_keeps_input_only_models_out_of_tts() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-stt-only"
            },
            "tts": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-tts-only"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/gpt-stt-only".to_string(),
                name: Some("GPT STT Only".to_string()),
                context_window: Some(128_000),
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
            crate::state::config::FetchedModel {
                id: "openai/gpt-tts-only".to_string(),
                name: Some("GPT TTS Only".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text"],
                        "output_modalities": ["text", "audio"]
                    }
                })),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    let stt_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    let tts_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert!(stt_models.iter().any(|id| id == "openai/gpt-stt-only"));
    assert!(!stt_models.iter().any(|id| id == "openai/gpt-tts-only"));
    assert!(tts_models.iter().any(|id| id == "openai/gpt-tts-only"));
    assert!(!tts_models.iter().any(|id| id == "openai/gpt-stt-only"));
}

#[test]
fn audio_model_picker_uses_directional_audio_metadata_when_modality_is_sparse() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "xai/grok-listen"
            },
            "tts": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "xai/grok-speak"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "xai/grok-listen".to_string(),
                name: Some("xAI Grok Listen".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "input_modalities": ["audio"]
                })),
            },
            crate::state::config::FetchedModel {
                id: "xai/grok-speak".to_string(),
                name: Some("xAI Grok Speak".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "output_modalities": ["audio"]
                })),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    let stt_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    let tts_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert!(stt_models.iter().any(|id| id == "xai/grok-listen"));
    assert!(!stt_models.iter().any(|id| id == "xai/grok-speak"));
    assert!(tts_models.iter().any(|id| id == "xai/grok-speak"));
    assert!(!tts_models.iter().any(|id| id == "xai/grok-listen"));
}

#[test]
fn audio_model_picker_does_not_treat_generic_modalities_audio_as_directional_support() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-stt-only"
            },
            "tts": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-tts-only"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/generic-audio".to_string(),
                name: Some("Generic Audio".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "modalities": ["text", "audio"]
                })),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    let stt_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    let tts_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert!(!stt_models.iter().any(|id| id == "openai/generic-audio"));
    assert!(!tts_models.iter().any(|id| id == "openai/generic-audio"));
}

#[test]
fn audio_model_picker_does_not_treat_nondirectional_modality_string_as_directional_support() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-stt-only"
            },
            "tts": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-tts-only"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/plain-modality-audio".to_string(),
                name: Some("Plain Modality Audio".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "modality": "text+audio"
                    }
                })),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    let stt_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    let tts_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert!(!stt_models
        .iter()
        .any(|id| id == "openai/plain-modality-audio"));
    assert!(!tts_models
        .iter()
        .any(|id| id == "openai/plain-modality-audio"));
}
