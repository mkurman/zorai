use super::super::*;
use super::preferred_voice_capture_backend_from_env_to_stop_voice_capture_process::{
    preferred_voice_capture_backend, read_voice_capture_error_summary, spawn_arecord_voice_capture,
    spawn_ffmpeg_voice_capture, stop_voice_capture_process, voice_capture_backend_label,
};
use base64::Engine;
use std::process::{Command, Stdio};

impl TuiModel {
    pub(in crate::app) fn input_wrap_width(&self) -> usize {
        self.width.saturating_sub(4).max(1) as usize
    }

    pub(in crate::app) fn input_height(&self) -> u16 {
        let inner_w = self.input_wrap_width();
        if inner_w <= 2 {
            return 3;
        }

        let wrapped = self.input.wrapped_display_buffer(inner_w);
        let attach_count = self.attachments.len();
        let min_visual_lines = if wrapped.is_empty() {
            1
        } else {
            wrapped.split('\n').count().max(1)
        };
        (min_visual_lines + attach_count + 2).clamp(3, 12) as u16
    }

    pub fn handle_paste(&mut self, text: String) {
        if let Some(modal_kind) = self.modal.top() {
            match modal_kind {
                modal::ModalKind::Settings if self.auth.login_target.is_some() => {
                    for ch in text.chars() {
                        match ch {
                            '\r' | '\n' => {}
                            other => self
                                .auth
                                .reduce(crate::state::auth::AuthAction::LoginKeyChar(other)),
                        }
                    }
                    return;
                }
                modal::ModalKind::Settings
                    if self.settings.active_tab() == SettingsTab::Plugins
                        && self.plugin_settings.install_mode =>
                {
                    for ch in text.chars() {
                        match ch {
                            '\r' | '\n' => {}
                            other => {
                                self.handle_plugins_settings_key(KeyCode::Char(other));
                            }
                        }
                    }
                    return;
                }
                modal::ModalKind::Settings if self.settings.is_editing() => {
                    let allow_newlines = self.settings.is_textarea();
                    for ch in text.chars() {
                        match ch {
                            '\r' => {}
                            '\n' if allow_newlines => {
                                self.settings.reduce(SettingsAction::InsertChar('\n'));
                            }
                            '\n' => {}
                            other => self.settings.reduce(SettingsAction::InsertChar(other)),
                        }
                    }
                    return;
                }
                modal::ModalKind::ContextWindowEditor => {
                    for ch in text.chars() {
                        if ch.is_ascii_digit() || matches!(ch, '_' | ',' | ' ') {
                            self.settings.reduce(SettingsAction::InsertChar(ch));
                        }
                    }
                    return;
                }
                modal::ModalKind::CustomModelEditor => {
                    for ch in text.chars() {
                        if !matches!(ch, '\r' | '\n') {
                            self.settings.reduce(SettingsAction::InsertChar(ch));
                        }
                    }
                    return;
                }
                modal::ModalKind::CommandPalette => {
                    let query = text
                        .chars()
                        .filter(|ch| !matches!(ch, '\r' | '\n'))
                        .collect::<String>();
                    self.modal.reduce(modal::ModalAction::SetQuery(query));
                    return;
                }
                modal::ModalKind::WorkspaceCreate => {
                    self.paste_into_workspace_create_workspace_modal(&text);
                    return;
                }
                modal::ModalKind::WorkspaceCreateTask => {
                    self.paste_into_workspace_create_modal(&text);
                    return;
                }
                modal::ModalKind::WorkspaceEditTask => {
                    self.paste_into_workspace_edit_modal(&text);
                    return;
                }
                modal::ModalKind::ThreadPicker
                | modal::ModalKind::GoalPicker
                | modal::ModalKind::WorkspacePicker
                | modal::ModalKind::ProviderPicker
                | modal::ModalKind::ModelPicker
                | modal::ModalKind::OpenRouterProviderPicker => {
                    self.input.reduce(input::InputAction::Clear);
                    for ch in text.chars() {
                        match ch {
                            '\r' | '\n' => {}
                            other => self.input.reduce(input::InputAction::InsertChar(other)),
                        }
                    }
                    self.modal.reduce(modal::ModalAction::SetQuery(
                        self.input.buffer().to_string(),
                    ));
                    if modal_kind == modal::ModalKind::ThreadPicker {
                        self.sync_thread_picker_item_count();
                    } else if modal_kind == modal::ModalKind::GoalPicker {
                        self.sync_goal_picker_item_count();
                    } else if modal_kind == modal::ModalKind::WorkspacePicker {
                        self.sync_workspace_picker_item_count();
                    } else if modal_kind == modal::ModalKind::ProviderPicker {
                        self.sync_provider_picker_item_count();
                    } else if modal_kind == modal::ModalKind::ModelPicker {
                        self.sync_model_picker_item_count();
                    } else if modal_kind == modal::ModalKind::OpenRouterProviderPicker {
                        self.sync_openrouter_provider_picker_item_count();
                    }
                    return;
                }
                _ => return,
            }
        }

        if self.focus != FocusArea::Input {
            self.focus = FocusArea::Input;
            self.input.set_mode(input::InputMode::Insert);
        }

        let trimmed = text.trim();
        if !trimmed.contains('\n')
            && (trimmed.starts_with('/')
                || trimmed.starts_with('~')
                || trimmed.starts_with("C:\\")
                || trimmed.starts_with("D:\\"))
        {
            let expanded = if trimmed.starts_with('~') {
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_default();
                trimmed.replacen('~', &home, 1)
            } else {
                trimmed.to_string()
            };

            if std::path::Path::new(&expanded).is_file() {
                self.attach_file(trimmed);
                return;
            }
        }

        if text.contains('\n') {
            self.input.insert_paste_block(text);
        } else {
            for c in text.chars() {
                self.input.reduce(input::InputAction::InsertChar(c));
            }
        }
        self.sync_goal_mission_control_prompt_from_input();
    }

    pub(in crate::app) fn attach_file(&mut self, path: &str) {
        let expanded = if path.starts_with('~') {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_default();
            path.replacen('~', &home, 1)
        } else {
            path.to_string()
        };

        let path = std::path::Path::new(&expanded);
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| expanded.clone());
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        let inferred_kind = match extension.as_str() {
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "svg" => {
                Some(("image", infer_attachment_mime(path)))
            }
            "mp3" | "wav" | "ogg" | "m4a" | "mp4" | "flac" | "webm" => {
                Some(("audio", infer_attachment_mime(path)))
            }
            _ => None,
        };

        let attachment = match inferred_kind {
            Some((kind, mime_type)) => match std::fs::read(&expanded) {
                Ok(bytes) => {
                    let data_url = format!(
                        "data:{};base64,{}",
                        mime_type,
                        base64::engine::general_purpose::STANDARD.encode(&bytes)
                    );
                    let block = serde_json::json!({
                        "type": kind,
                        "data_url": data_url,
                        "mime_type": mime_type,
                    });
                    Some(Attachment {
                        filename: filename.clone(),
                        size_bytes: bytes.len(),
                        payload: AttachmentPayload::ContentBlock(block),
                    })
                }
                Err(error) => {
                    self.status_line = "Attach failed".to_string();
                    self.last_error = Some(format!("Attach failed: {}", error));
                    self.error_active = true;
                    self.error_tick = self.tick_counter;
                    None
                }
            },
            None => match std::fs::read_to_string(&expanded) {
                Ok(content) => Some(Attachment {
                    filename: filename.clone(),
                    size_bytes: content.len(),
                    payload: AttachmentPayload::Text(content),
                }),
                Err(error) => {
                    self.status_line = "Attach failed".to_string();
                    self.last_error = Some(format!(
                        "Attach failed: text attachments must be UTF-8 or use a supported image/audio format ({})",
                        error
                    ));
                    self.error_active = true;
                    self.error_tick = self.tick_counter;
                    None
                }
            },
        };

        if let Some(attachment) = attachment {
            let size = attachment.size_bytes;
            self.attachments.push(attachment);
            self.status_line = format!("Attached: {} ({} bytes)", filename, size);
        }
    }

    pub(in crate::app) fn start_voice_capture(&mut self) {
        if self.voice_recording {
            self.status_line = "Voice capture already active".to_string();
            return;
        }

        let capture_id = uuid::Uuid::new_v4();
        let capture_path = std::env::temp_dir().join(format!("zorai-voice-{capture_id}.wav"));
        let stderr_path = std::env::temp_dir().join(format!("zorai-voice-{capture_id}.log"));
        let capture_path_string = capture_path.to_string_lossy().to_string();
        let stderr_path_string = stderr_path.to_string_lossy().to_string();
        let ffmpeg_backend = preferred_voice_capture_backend();

        let child =
            match spawn_ffmpeg_voice_capture(&capture_path_string, &stderr_path, ffmpeg_backend) {
                Ok(child) => Ok((
                    child,
                    voice_capture_backend_label(ffmpeg_backend).to_string(),
                )),
                Err(_) => spawn_arecord_voice_capture(&capture_path_string, &stderr_path)
                    .map(|child| (child, "arecord".to_string())),
            };

        match child {
            Ok((child, backend_label)) => {
                self.voice_recorder = Some(child);
                self.voice_recording = true;
                self.voice_capture_path = Some(capture_path_string);
                self.voice_capture_stderr_path = Some(stderr_path_string);
                self.voice_capture_backend_label = Some(backend_label);
                self.begin_voice_live_session();
                self.status_line = "Voice capture started".to_string();
                self.show_input_notice(
                    "Recording voice capture... toggle again to stop",
                    InputNoticeKind::Success,
                    120,
                    false,
                );
            }
            Err(error) => {
                self.status_line = "Voice capture failed (ffmpeg/arecord unavailable)".to_string();
                self.show_input_notice(
                    "Voice capture failed: install ffmpeg or arecord",
                    InputNoticeKind::Warning,
                    90,
                    true,
                );
                self.last_error = Some(format!(
                    "Voice capture failed: ffmpeg/arecord unavailable ({error})"
                ));
                self.error_active = true;
                self.error_tick = self.tick_counter;
            }
        }
    }

    pub(in crate::app) fn stop_voice_capture(&mut self) -> Option<String> {
        let recorder_status = self
            .voice_recorder
            .as_mut()
            .and_then(stop_voice_capture_process);
        self.voice_recorder.take();
        let stderr_path = self.voice_capture_stderr_path.take();
        let backend_label = self.voice_capture_backend_label.take();

        self.voice_recording = false;
        let capture_path = self.voice_capture_path.take();
        if let Some(path) = capture_path.as_ref() {
            if std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) == 0 {
                if recorder_status.is_some_and(|status| !status.success()) {
                    self.status_line = "Voice capture failed".to_string();
                    self.show_input_notice(
                        "Voice capture failed — check error viewer",
                        InputNoticeKind::Warning,
                        80,
                        true,
                    );
                    let backend = backend_label
                        .map(|label| format!(" ({label})"))
                        .unwrap_or_default();
                    let detail = read_voice_capture_error_summary(stderr_path.as_deref())
                        .unwrap_or_else(|| "recorder exited before writing audio".to_string());
                    self.last_error = Some(format!("Voice capture failed{backend}: {detail}"));
                    self.error_active = true;
                    self.error_tick = self.tick_counter;
                    if let Some(stderr_path) = stderr_path.as_deref() {
                        let _ = std::fs::remove_file(stderr_path);
                    }
                    return None;
                }
                self.status_line = "Voice capture empty".to_string();
                self.show_input_notice(
                    "Voice capture was empty — try speaking louder/longer",
                    InputNoticeKind::Warning,
                    80,
                    true,
                );
                if let Some(stderr_path) = stderr_path.as_deref() {
                    let _ = std::fs::remove_file(stderr_path);
                }
                return None;
            }
            self.status_line = "Voice capture stopped".to_string();
        }
        if let Some(stderr_path) = stderr_path.as_deref() {
            let _ = std::fs::remove_file(stderr_path);
        }
        capture_path
    }

    pub(in crate::app) fn clear_voice_player_ipc(&mut self) {
        if let Some(socket) = self.voice_player_ipc.take() {
            let _ = std::fs::remove_file(&socket);
        }
        self.voice_paused = false;
    }

    pub(in crate::app) fn stop_voice_playback(&mut self) {
        if let Some(mut child) = self.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.clear_voice_player_ipc();
            self.status_line = "Audio playback stopped".to_string();
            self.show_input_notice("Stopped playback", InputNoticeKind::Success, 50, true);
        } else {
            self.clear_voice_player_ipc();
            self.status_line = "No active audio playback".to_string();
        }
    }

    /// Toggle pause/resume on the active mpv player via its IPC socket.
    /// Returns true when the key was handled (something was playing), false when
    /// there is nothing to toggle so the caller can fall back to synthesis.
    pub(in crate::app) fn toggle_voice_pause(&mut self) -> bool {
        if self.voice_player.is_none() {
            return false;
        }
        let Some(socket) = self.voice_player_ipc.clone() else {
            self.status_line = "Pause needs mpv (paplay can't pause)".to_string();
            self.show_input_notice(
                "Pause/resume requires mpv; Ctrl+S stops playback",
                InputNoticeKind::Warning,
                70,
                true,
            );
            return true;
        };
        match send_mpv_ipc_command(&socket, "{\"command\":[\"cycle\",\"pause\"]}") {
            Ok(()) => {
                self.voice_paused = !self.voice_paused;
                self.status_line = if self.voice_paused {
                    "Speech paused".to_string()
                } else {
                    "Speech resumed".to_string()
                };
            }
            Err(error) => {
                self.status_line = "Pause/resume failed".to_string();
                self.last_error = Some(format!("mpv pause/resume failed: {error}"));
                self.error_active = true;
                self.error_tick = self.tick_counter;
            }
        }
        true
    }

    pub(in crate::app) fn play_audio_path(&mut self, path: &str) {
        if let Some(mut child) = self.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.clear_voice_player_ipc();

        self.voice_player_seq = self.voice_player_seq.wrapping_add(1);
        let socket_path = std::env::temp_dir().join(format!(
            "zorai-mpv-{}-{}.sock",
            std::process::id(),
            self.voice_player_seq
        ));
        let _ = std::fs::remove_file(&socket_path);
        let ipc_arg = format!("--input-ipc-server={}", socket_path.display());

        let mpv = Command::new("mpv")
            .args(["--no-video", "--no-terminal", &ipc_arg, path])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let (child, has_ipc) = match mpv {
            Ok(child) => (Ok(child), true),
            Err(_) => (
                Command::new("paplay")
                    .arg(path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn(),
                false,
            ),
        };

        match child {
            Ok(child) => {
                self.voice_player = Some(child);
                self.voice_paused = false;
                self.voice_player_ipc = if has_ipc { Some(socket_path) } else { None };
                self.status_line = if has_ipc {
                    "Playing speech (Ctrl+P pause/resume, Ctrl+S stop)".to_string()
                } else {
                    "Playing synthesized speech...".to_string()
                };
            }
            Err(error) => {
                self.voice_player_ipc = None;
                self.status_line = "Audio playback failed (mpv/paplay unavailable)".to_string();
                self.show_input_notice(
                    "Audio playback failed: install mpv or paplay",
                    InputNoticeKind::Warning,
                    90,
                    true,
                );
                self.last_error = Some(format!(
                    "Audio playback failed: mpv/paplay unavailable ({error})"
                ));
                self.error_active = true;
                self.error_tick = self.tick_counter;
            }
        }
    }
}

#[cfg(unix)]
fn send_mpv_ipc_command(socket: &std::path::Path, command: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    let mut stream = UnixStream::connect(socket)?;
    stream.write_all(command.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

#[cfg(not(unix))]
fn send_mpv_ipc_command(_socket: &std::path::Path, _command: &str) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "mpv IPC pause/resume is only supported on Unix",
    ))
}

fn infer_attachment_mime(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" | "mp4" => "audio/mp4",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    }
}
