use std::fs::File;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoiceCaptureBackend {
    Pulse,
    Alsa,
}

pub(crate) fn preferred_voice_capture_backend_from_env(
    has_pulse_server: bool,
) -> VoiceCaptureBackend {
    if has_pulse_server {
        VoiceCaptureBackend::Pulse
    } else {
        VoiceCaptureBackend::Alsa
    }
}

pub(super) fn preferred_voice_capture_backend() -> VoiceCaptureBackend {
    preferred_voice_capture_backend_from_env(std::env::var_os("PULSE_SERVER").is_some())
}

pub(crate) fn ffmpeg_voice_capture_args(
    capture_path: &str,
    backend: VoiceCaptureBackend,
) -> Vec<String> {
    let input_format = match backend {
        VoiceCaptureBackend::Pulse => "pulse",
        VoiceCaptureBackend::Alsa => "alsa",
    };
    vec![
        "-y".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-f".to_string(),
        input_format.to_string(),
        "-i".to_string(),
        "default".to_string(),
        "-ac".to_string(),
        "1".to_string(),
        capture_path.to_string(),
    ]
}

pub(super) fn voice_capture_backend_label(backend: VoiceCaptureBackend) -> &'static str {
    match backend {
        VoiceCaptureBackend::Pulse => "ffmpeg/pulse",
        VoiceCaptureBackend::Alsa => "ffmpeg/alsa",
    }
}

pub(super) fn read_voice_capture_error_summary(stderr_path: Option<&str>) -> Option<String> {
    let path = stderr_path?;
    let content = std::fs::read_to_string(path).ok()?;
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(240).collect::<String>())
}

pub(super) fn spawn_ffmpeg_voice_capture(
    capture_path: &str,
    stderr_path: &Path,
    backend: VoiceCaptureBackend,
) -> std::io::Result<std::process::Child> {
    let stderr = File::create(stderr_path)?;
    Command::new("ffmpeg")
        .args(ffmpeg_voice_capture_args(capture_path, backend))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
}

pub(super) fn spawn_arecord_voice_capture(
    capture_path: &str,
    stderr_path: &Path,
) -> std::io::Result<std::process::Child> {
    let stderr = File::create(stderr_path)?;
    Command::new("arecord")
        .args(["-f", "cd", "-t", "wav", capture_path])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
}

pub(super) fn stop_voice_capture_process(child: &mut Child) -> Option<ExitStatus> {
    #[cfg(unix)]
    {
        let interrupt_result = unsafe { libc::kill(child.id() as i32, libc::SIGINT) };
        if interrupt_result == 0 {
            let deadline = Instant::now() + Duration::from_millis(1500);
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => return Some(status),
                    Ok(None) if Instant::now() < deadline => sleep(Duration::from_millis(50)),
                    Ok(None) | Err(_) => break,
                }
            }
        }
    }

    let _ = child.kill();
    child.wait().ok()
}
