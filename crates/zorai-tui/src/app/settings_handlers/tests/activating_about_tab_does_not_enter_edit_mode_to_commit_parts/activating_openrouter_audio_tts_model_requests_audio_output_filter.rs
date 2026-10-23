use super::super::make_model;
use super::opening_weles_editor_hides_inherited_main_system_prompt_to_feat_skill::focus_settings_field;
use super::*;
use crate::app::TuiModel;
use crate::state::settings::SettingsTab;
use crossterm::event::{KeyCode, KeyModifiers};
#[test]
fn activating_openrouter_audio_tts_model_requests_audio_output_filter() {
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
            "tts": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-4o-mini-tts"
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
            assert_eq!(provider_id, PROVIDER_ID_OPENROUTER);
            assert_eq!(base_url, "https://openrouter.ai/api/v1");
            assert_eq!(api_key, "router-key");
            assert_eq!(output_modalities.as_deref(), Some("speech"));
        }
        other => {
            panic!("expected filtered FetchModels for OpenRouter audio TTS picker, got {other:?}")
        }
    }
}

#[test]
fn activating_openrouter_image_generation_model_requests_image_output_filter() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "providers": {
            PROVIDER_ID_OPENROUTER: {
                "base_url": "https://openrouter.ai/api/v1",
                "api_key": "router-key",
                "auth_source": "api_key"
            }
        },
        "image": {
            "generation": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-image-1"
            }
        }
    }));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    focus_settings_field(
        &mut model,
        SettingsTab::Media,
        "feat_image_generation_model",
    );

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
            assert_eq!(output_modalities.as_deref(), Some("image"));
        }
        other => panic!("expected filtered FetchModels for OpenRouter image picker, got {other:?}"),
    }
}

#[test]
fn image_generation_catalog_includes_gpt_image_2_for_openai_and_openrouter() {
    let openai_models = TuiModel::image_generation_catalog_models(PROVIDER_ID_OPENAI);
    assert!(
        openai_models.iter().any(|model| model.id == "gpt-image-2"),
        "expected OpenAI image catalog to include gpt-image-2"
    );
    assert!(
        openai_models.iter().any(|model| model.id == "gpt-image-1"),
        "expected OpenAI image catalog to retain gpt-image-1"
    );

    let openrouter_models = TuiModel::image_generation_catalog_models(PROVIDER_ID_OPENROUTER);
    assert!(
        openrouter_models
            .iter()
            .any(|model| model.id == "openai/gpt-image-2"),
        "expected OpenRouter image catalog to include openai/gpt-image-2"
    );
    assert!(
        openrouter_models
            .iter()
            .any(|model| model.id == "openai/gpt-image-1"),
        "expected OpenRouter image catalog to retain openai/gpt-image-1"
    );

    let minimax_models = TuiModel::image_generation_catalog_models(PROVIDER_ID_MINIMAX);
    assert!(
        minimax_models.iter().any(|model| model.id == "image-01"),
        "expected MiniMax image catalog to include image-01"
    );

    let minimax_coding_models =
        TuiModel::image_generation_catalog_models(PROVIDER_ID_MINIMAX_CODING_PLAN);
    assert!(
        minimax_coding_models
            .iter()
            .any(|model| model.id == "image-01"),
        "expected MiniMax Coding Plan image catalog to include image-01"
    );
}

#[test]
fn embedding_catalog_includes_openrouter_embedding_models() {
    let openrouter_models = TuiModel::embedding_catalog_models(PROVIDER_ID_OPENROUTER);

    assert!(
        openrouter_models
            .iter()
            .any(|model| model.id == "openai/text-embedding-3-small"),
        "expected OpenRouter embedding catalog to include text-embedding-3-small"
    );
    assert!(
        openrouter_models
            .iter()
            .any(|model| model.id == "openai/text-embedding-3-large"),
        "expected OpenRouter embedding catalog to include text-embedding-3-large"
    );
}

#[test]
fn minimax_audio_catalog_is_tts_only_and_uses_speech_28_defaults() {
    let minimax_tts = TuiModel::audio_catalog_models("tts", PROVIDER_ID_MINIMAX);
    assert!(
        minimax_tts.iter().any(|model| model.id == "speech-2.8-hd"),
        "expected MiniMax TTS catalog to include speech-2.8-hd"
    );
    assert!(
        TuiModel::audio_catalog_models("stt", PROVIDER_ID_MINIMAX).is_empty(),
        "expected MiniMax STT catalog to stay empty"
    );

    let minimax_coding_tts = TuiModel::audio_catalog_models("tts", PROVIDER_ID_MINIMAX_CODING_PLAN);
    assert!(
        minimax_coding_tts
            .iter()
            .any(|model| model.id == "speech-2.8-turbo"),
        "expected MiniMax Coding Plan TTS catalog to include speech-2.8-turbo"
    );
}

#[test]
fn activating_subagent_model_fetches_remote_models_for_fetchable_provider() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "providers": {
            PROVIDER_ID_CHUTES: {
                "base_url": "https://llm.chutes.ai/v1",
                "api_key": "chutes-key",
                "auth_source": "api_key"
            }
        }
    }));
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("worker".to_string()),
        1,
        PROVIDER_ID_CHUTES.to_string(),
        "deepseek-ai/DeepSeek-R1".to_string(),
    );
    editor.field = crate::state::subagents::SubAgentEditorField::Model;
    model.subagents.editor = Some(editor);
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
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::FetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        }) => {
            assert_eq!(provider_id, PROVIDER_ID_CHUTES);
            assert_eq!(base_url, "https://llm.chutes.ai/v1");
            assert_eq!(api_key, "chutes-key");
            assert_eq!(output_modalities, None);
        }
        other => panic!("expected FetchModels for sub-agent model picker, got {other:?}"),
    }
}

#[test]
fn audio_stt_catalog_includes_openai_diarization_model() {
    let model_ids = TuiModel::audio_catalog_models("stt", PROVIDER_ID_OPENAI)
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();

    assert_eq!(
        model_ids,
        vec![
            "gpt-4o-transcribe",
            "gpt-4o-mini-transcribe",
            "gpt-4o-transcribe-diarize",
            "whisper-1",
        ]
    );
}

#[test]
fn audio_stt_catalog_includes_groq_transcription_models() {
    let model_ids = TuiModel::audio_catalog_models("stt", PROVIDER_ID_GROQ)
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();

    assert_eq!(
        model_ids,
        vec!["whisper-large-v3-turbo", "whisper-large-v3"]
    );
}

#[test]
fn activating_audio_stt_model_prefills_groq_static_models_and_fetches_remote_catalog() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "providers": {
            PROVIDER_ID_GROQ: {
                "base_url": "https://api.groq.com/openai/v1",
                "api_key": "groq-key",
                "auth_source": "api_key"
            }
        },
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_GROQ,
                "model": "whisper-large-v3-turbo"
            }
        }
    }));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    focus_settings_field(&mut model, SettingsTab::Media, "feat_audio_stt_model");

    model.activate_settings_field();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    assert_eq!(
        model
            .config
            .fetched_models
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec!["whisper-large-v3-turbo", "whisper-large-v3"]
    );
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::FetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        }) => {
            assert_eq!(provider_id, PROVIDER_ID_GROQ);
            assert_eq!(base_url, "https://api.groq.com/openai/v1");
            assert_eq!(api_key, "groq-key");
            assert_eq!(output_modalities, None);
        }
        other => panic!("expected FetchModels for Groq audio STT picker, got {other:?}"),
    }
}

#[test]
fn audio_tts_catalog_does_not_fabricate_groq_entries() {
    let model_ids = TuiModel::audio_catalog_models("tts", PROVIDER_ID_GROQ)
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();

    assert_eq!(
        model_ids,
        vec![
            "canopylabs/orpheus-v1-english",
            "canopylabs/orpheus-arabic-saudi",
        ]
    );
}

#[test]
fn audio_tts_catalog_matches_azure_openai_alias() {
    let model_ids = TuiModel::audio_catalog_models("tts", PROVIDER_ID_AZURE_OPENAI)
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();

    assert_eq!(model_ids, vec!["gpt-4o-mini-tts", "tts-1", "tts-1-hd"]);
}

#[test]
fn xai_audio_catalog_uses_provider_native_defaults_for_both_endpoints() {
    let stt_model_ids = TuiModel::audio_catalog_models("stt", PROVIDER_ID_XAI)
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();
    let tts_model_ids = TuiModel::audio_catalog_models("tts", PROVIDER_ID_XAI)
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();

    assert_eq!(stt_model_ids, vec!["grok-4.3"]);
    assert_eq!(tts_model_ids, vec!["grok-4.3"]);
    assert_eq!(
        TuiModel::default_audio_model_for("stt", PROVIDER_ID_XAI),
        "grok-4.3"
    );
    assert_eq!(
        TuiModel::default_audio_model_for("tts", PROVIDER_ID_XAI),
        "grok-4.3"
    );
}

#[test]
fn xiaomi_audio_catalog_is_tts_only_and_uses_v25_defaults() {
    let stt_model_ids = TuiModel::audio_catalog_models(
        "stt",
        zorai_shared::providers::PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
    )
    .into_iter()
    .map(|model| model.id)
    .collect::<Vec<_>>();
    let tts_model_ids = TuiModel::audio_catalog_models(
        "tts",
        zorai_shared::providers::PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
    )
    .into_iter()
    .map(|model| model.id)
    .collect::<Vec<_>>();

    assert!(stt_model_ids.is_empty());
    assert_eq!(
        tts_model_ids,
        vec![
            "mimo-v2.5-tts",
            "mimo-v2.5-tts-voiceclone",
            "mimo-v2.5-tts-voicedesign",
        ]
    );
    assert_eq!(
        TuiModel::default_audio_model_for(
            "tts",
            zorai_shared::providers::PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
        ),
        "mimo-v2.5-tts"
    );
}

#[test]
fn audio_default_model_is_empty_when_provider_has_no_static_audio_catalog() {
    assert_eq!(
        TuiModel::default_audio_model_for("tts", PROVIDER_ID_OPENROUTER),
        ""
    );
}

#[test]
fn activating_compaction_custom_auth_source_for_openai_forces_responses_transport() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.config.compaction_strategy = "custom_model".to_string();
    model.config.compaction_custom_provider = PROVIDER_ID_OPENAI.to_string();
    model.config.compaction_custom_auth_source = "api_key".to_string();
    model.config.compaction_custom_api_transport = "chat_completions".to_string();
    focus_settings_field(
        &mut model,
        SettingsTab::Compaction,
        "compaction_custom_auth_source",
    );

    model.activate_settings_field();

    assert_eq!(
        model.config.compaction_custom_auth_source,
        "chatgpt_subscription"
    );
    assert_eq!(model.config.compaction_custom_api_transport, "responses");
}

#[test]
fn settings_enter_toggles_embedding_enabled() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({}));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    focus_settings_field(&mut model, SettingsTab::Memory, "feat_embedding_enabled");
    assert!(!model.config.semantic_embedding_enabled());

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert!(model.config.semantic_embedding_enabled());
    assert!(matches!(
        daemon_rx.try_recv().expect("expected embedding toggle command"),
        DaemonCommand::SetConfigItem {
            key_path,
            value_json,
        }         if key_path == "/semantic/embedding/enabled" && value_json == "true"
    ));
}

#[test]
fn settings_enter_cycles_auto_thread_title() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({}));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    focus_settings_field(&mut model, SettingsTab::Workflows, "feat_auto_thread_title");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("auto_thread_title"))
            .and_then(|value| value.as_str()),
        Some("rarog")
    );
    assert!(matches!(
        daemon_rx.try_recv().expect("expected auto thread title cycle command"),
        DaemonCommand::SetConfigItem {
            key_path,
            value_json,
        } if key_path == "/auto_thread_title" && value_json == "\"rarog\""
    ));
}
