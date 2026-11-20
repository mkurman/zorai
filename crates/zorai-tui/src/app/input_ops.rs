#[path = "input_ops_parts/preferred_voice_capture_backend_from_env_to_stop_voice_capture_process.rs"]
mod preferred_voice_capture_backend_from_env_to_stop_voice_capture_process;

#[path = "input_ops_parts/input_wrap_width_to_infer_attachment_mime.rs"]
mod input_wrap_width_to_infer_attachment_mime;

#[cfg(test)]
pub(crate) use preferred_voice_capture_backend_from_env_to_stop_voice_capture_process::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_voice_capture_backend_uses_pulse_when_pulse_server_is_present() {
        assert_eq!(
            preferred_voice_capture_backend_from_env(true),
            VoiceCaptureBackend::Pulse
        );
    }

    #[test]
    fn preferred_voice_capture_backend_defaults_to_alsa_without_pulse_server() {
        assert_eq!(
            preferred_voice_capture_backend_from_env(false),
            VoiceCaptureBackend::Alsa
        );
    }

    #[test]
    fn ffmpeg_voice_capture_args_use_pulse_input_and_no_duration_limit_so_recording_runs_until_stopped() {
        let args = ffmpeg_voice_capture_args("/tmp/test.wav", VoiceCaptureBackend::Pulse);
        assert_eq!(
            args,
            vec![
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "pulse",
                "-i",
                "default",
                "-ac",
                "1",
                "/tmp/test.wav",
            ]
        );
    }

    #[test]
    fn ffmpeg_voice_capture_args_use_alsa_input_when_alsa_backend_selected() {
        let args = ffmpeg_voice_capture_args("/tmp/test.wav", VoiceCaptureBackend::Alsa);
        assert_eq!(args[5], "alsa");
    }
}
