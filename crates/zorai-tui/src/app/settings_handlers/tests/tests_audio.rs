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
