use super::make_model;
use super::whatsapp_link_device_probes_status_before_starting_link_flow::focus_settings_field;
use super::*;
#[test]
fn audio_toggle_fields_write_extra_paths() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({ "audio": { "stt": {} } }));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    focus_settings_field(&mut model, SettingsTab::Media, "feat_audio_stt_enabled");
    let quit = model.handle_key_modal(
        KeyCode::Char(' '),
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SetConfigItem {
            key_path,
            value_json,
        }) => {
            assert_eq!(key_path, "/audio/stt/enabled");
            assert_eq!(value_json, "true");
        }
        other => panic!("expected SetConfigItem for audio toggle, got {other:?}"),
    }
}

#[test]
fn audio_text_fields_start_and_commit_edit() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "tts": {
                "voice": "alloy"
            }
        }
    }));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    focus_settings_field(&mut model, SettingsTab::Media, "feat_audio_tts_voice");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.settings.editing_field(), Some("feat_audio_tts_voice"));
    assert_eq!(model.settings.edit_buffer(), "alloy");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.settings.editing_field(), None);

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SetConfigItem {
            key_path,
            value_json,
        }) => {
            assert_eq!(key_path, "/audio/tts/voice");
            assert_eq!(value_json, "\"alloy\"");
        }
        other => panic!("expected SetConfigItem for audio voice edit, got {other:?}"),
    }
}

#[test]
fn understanding_fields_write_their_own_path_so_analysis_tools_stop_inheriting_the_main_model() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({}));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    focus_settings_field(
        &mut model,
        SettingsTab::Media,
        "feat_video_understanding_provider",
    );
    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert_eq!(
        model.settings.editing_field(),
        Some("feat_video_understanding_provider")
    );
    model
        .settings
        .start_editing("feat_video_understanding_provider", " gemini ");
    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    let mut writes = Vec::new();
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::SetConfigItem {
            key_path,
            value_json,
        } = command
        {
            writes.push((key_path, value_json));
        }
    }
    assert_eq!(
        writes,
        vec![(
            "/video/understanding/provider".to_string(),
            "\"gemini\"".to_string()
        )]
    );
    assert_eq!(
        model.config.agent_config_raw.as_ref().unwrap()["video"]["understanding"]["provider"],
        "gemini"
    );
    assert!(model.config.agent_config_raw.as_ref().unwrap()["image"]
        .get("understanding")
        .is_none());
}
