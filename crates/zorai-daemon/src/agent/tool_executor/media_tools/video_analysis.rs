use super::*;

const DEFAULT_VIDEO_ANALYSIS_PROMPT: &str =
    "Describe what happens in this video in detail, in chronological order.";
const DEFAULT_VIDEO_FRAMES: u64 = 8;
const MAX_VIDEO_FRAMES: u64 = 32;
const VIDEO_FRAME_WIDTH: u32 = 768;

#[derive(Debug, Clone, PartialEq)]
enum VideoAnalysisSource {
    Local(PathBuf),
    Remote(String),
}

impl VideoAnalysisSource {
    fn ffmpeg_input(&self) -> String {
        match self {
            Self::Local(path) => path.to_string_lossy().into_owned(),
            Self::Remote(url) => url.clone(),
        }
    }
}

fn video_analysis_source(args: &serde_json::Value) -> Result<VideoAnalysisSource> {
    let arg = |key: &str| {
        args.get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    match (arg("path"), arg("url")) {
        (Some(_), Some(_)) => anyhow::bail!("pass exactly one of 'path' or 'url'"),
        (Some(path), None) => {
            let path = path.strip_prefix("file://").unwrap_or(path);
            validate_read_path(path)?;
            let resolved = resolve_tool_path(path, None);
            if !resolved.is_file() {
                anyhow::bail!("video file not found: {}", resolved.display());
            }
            Ok(VideoAnalysisSource::Local(resolved))
        }
        (None, Some(url)) if url.starts_with("http://") || url.starts_with("https://") => {
            Ok(VideoAnalysisSource::Remote(url.to_string()))
        }
        (None, Some(_)) => anyhow::bail!("'url' must be an http(s) URL"),
        (None, None) => anyhow::bail!("missing 'path' or 'url' argument"),
    }
}

fn video_mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("mkv") => "video/x-matroska",
        Some("avi") => "video/x-msvideo",
        _ => "video/mp4",
    }
}

pub(crate) fn video_frame_timestamps(duration_seconds: f64, frames: u64) -> Vec<f64> {
    let frames = frames.clamp(1, MAX_VIDEO_FRAMES);
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return vec![0.0];
    }
    (0..frames)
        .map(|index| duration_seconds * (index as f64 + 0.5) / frames as f64)
        .collect()
}

fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

async fn run_media_command(program: &str, args: &[String]) -> Result<std::process::Output> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!(
                    "{program} is not installed; install ffmpeg to analyze video with a model that cannot watch video directly"
                )
            } else {
                anyhow::anyhow!("failed to run {program}: {error}")
            }
        })?;
    Ok(output)
}

async fn probe_video_duration(input: &str) -> Result<f64> {
    let output = run_media_command(
        "ffprobe",
        &[
            "-v".into(),
            "error".into(),
            "-show_entries".into(),
            "format=duration".into(),
            "-of".into(),
            "default=noprint_wrappers=1:nokey=1".into(),
            input.into(),
        ],
    )
    .await?;
    if !output.status.success() {
        anyhow::bail!(
            "ffprobe could not read the video: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0))
}

async fn extract_video_frame(input: &str, at_seconds: f64, output: &Path) -> Result<Vec<u8>> {
    let result = run_media_command(
        "ffmpeg",
        &[
            "-v".into(),
            "error".into(),
            "-ss".into(),
            format!("{at_seconds:.3}"),
            "-i".into(),
            input.into(),
            "-frames:v".into(),
            "1".into(),
            "-vf".into(),
            format!("scale='min({VIDEO_FRAME_WIDTH},iw)':-2"),
            "-q:v".into(),
            "4".into(),
            "-y".into(),
            output.to_string_lossy().into_owned(),
        ],
    )
    .await?;
    if !result.status.success() {
        anyhow::bail!(
            "ffmpeg could not extract a frame at {}: {}",
            format_timestamp(at_seconds),
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(tokio::fs::read(output).await?)
}

async fn extract_video_audio(input: &str, output: &Path) -> Result<()> {
    let result = run_media_command(
        "ffmpeg",
        &[
            "-v".into(),
            "error".into(),
            "-i".into(),
            input.into(),
            "-vn".into(),
            "-ac".into(),
            "1".into(),
            "-ar".into(),
            "16000".into(),
            "-y".into(),
            output.to_string_lossy().into_owned(),
        ],
    )
    .await?;
    if !result.status.success() {
        anyhow::bail!(
            "no usable audio track ({})",
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(())
}

async fn transcribe_video_audio(
    agent: &AgentEngine,
    http_client: &reqwest::Client,
    input: &str,
    work_dir: &Path,
) -> std::result::Result<String, String> {
    let audio_path = work_dir.join("audio.wav");
    extract_video_audio(input, &audio_path)
        .await
        .map_err(|error| error.to_string())?;
    let transcript = execute_speech_to_text(
        &serde_json::json!({
            "path": audio_path.to_string_lossy(),
            "mime_type": "audio/wav",
            "response_format": "text",
        }),
        agent,
        http_client,
    )
    .await
    .map_err(|error| format!("speech-to-text failed: {error}"))?;
    let transcript = transcript.trim().to_string();
    if transcript.is_empty() {
        Err("the audio track had no speech".to_string())
    } else {
        Ok(transcript)
    }
}

fn single_user_message(blocks: Vec<serde_json::Value>) -> Vec<ApiMessage> {
    vec![ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Blocks(blocks),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    }]
}

async fn run_video_analysis_completion(
    http_client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    transport: ApiTransport,
    messages: Vec<ApiMessage>,
) -> Result<String> {
    let stream = crate::agent::llm_client::send_completion_request(
        http_client,
        provider_id,
        provider_config,
        "",
        &messages,
        &[],
        transport,
        None,
        None,
        RetryStrategy::Bounded {
            max_retries: 1,
            retry_delay_ms: 1_000,
        },
    );
    let (content, _, _) = collect_completion_output(stream).await?;
    if content.trim().is_empty() {
        anyhow::bail!("video analysis returned no content");
    }
    Ok(content)
}

fn native_video_fits(source: &VideoAnalysisSource) -> bool {
    match source {
        VideoAnalysisSource::Local(path) => std::fs::metadata(path)
            .map(|meta| meta.len() <= crate::agent::llm_client::MAX_INLINE_VIDEO_BYTES)
            .unwrap_or(false),
        VideoAnalysisSource::Remote(_) => true,
    }
}

pub(crate) async fn execute_analyze_video(
    args: &serde_json::Value,
    agent: &AgentEngine,
    _http_client: &reqwest::Client,
) -> Result<String> {
    let media_http_client = fresh_media_http_client(tool_names::ANALYZE_VIDEO, args);
    let source = video_analysis_source(args)?;
    let prompt = args
        .get("prompt")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_VIDEO_ANALYSIS_PROMPT);
    let (provider_id, provider_config) =
        resolve_media_provider_config(agent, args, Some(MediaEndpoint::VideoUnderstanding)).await?;
    let transport = effective_multimodal_transport(&provider_id, &provider_config);
    let api_type = crate::agent::types::get_provider_api_type(
        &provider_id,
        &provider_config.model,
        &provider_config.base_url,
    );

    if crate::agent::llm_client::native_video_supported(
        &provider_id,
        &provider_config.model,
        api_type,
        transport,
    ) && native_video_fits(&source)
    {
        let video_block = match &source {
            VideoAnalysisSource::Local(path) => serde_json::json!({
                "type": "input_video",
                "path": path.to_string_lossy(),
                "mime_type": video_mime_for_path(path),
            }),
            VideoAnalysisSource::Remote(url) => serde_json::json!({
                "type": "input_video",
                "video_url": url,
                "mime_type": "video/mp4",
            }),
        };
        let messages = single_user_message(vec![
            serde_json::json!({ "type": "input_text", "text": prompt }),
            video_block,
        ]);
        let analysis = run_video_analysis_completion(
            &media_http_client,
            &provider_id,
            &provider_config,
            transport,
            messages,
        )
        .await?;
        return Ok(format!(
            "[analyzed natively by {provider_id}/{}]\n\n{analysis}",
            provider_config.model
        ));
    }

    if !crate::agent::types::model_supports(
        &provider_id,
        &provider_config.model,
        crate::agent::types::Modality::Image,
    ) {
        anyhow::bail!(
            "{provider_id}/{} can neither watch video nor read images. Pass `provider` and `model` for a video- or image-capable model, or mark this model's modalities in its model settings.",
            provider_config.model
        );
    }

    let input = source.ffmpeg_input();
    let frames = args
        .get("frames")
        .and_then(|value| value.as_u64())
        .unwrap_or(DEFAULT_VIDEO_FRAMES);
    let duration = probe_video_duration(&input).await?;
    let work_dir = std::env::temp_dir().join(format!("zorai-video-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&work_dir).await?;
    let result = analyze_video_frames(
        agent,
        &media_http_client,
        &provider_id,
        &provider_config,
        prompt,
        &input,
        duration,
        frames,
        args.get("include_transcript")
            .and_then(|value| value.as_bool())
            .unwrap_or(true),
        &work_dir,
    )
    .await;
    let _ = tokio::fs::remove_dir_all(&work_dir).await;
    result
}

#[allow(clippy::too_many_arguments)]
async fn analyze_video_frames(
    agent: &AgentEngine,
    http_client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    prompt: &str,
    input: &str,
    duration: f64,
    frames: u64,
    include_transcript: bool,
    work_dir: &Path,
) -> Result<String> {
    let timestamps = video_frame_timestamps(duration, frames);
    let transcript = if include_transcript {
        Some(transcribe_video_audio(agent, http_client, input, work_dir).await)
    } else {
        None
    };

    let mut intro = format!(
        "{prompt}\n\nThe video is {} long. Below are {} frames sampled evenly across it, each labelled with its timestamp.",
        format_timestamp(duration),
        timestamps.len()
    );
    match &transcript {
        Some(Ok(text)) => {
            intro.push_str("\n\nAudio transcript:\n");
            intro.push_str(text);
        }
        Some(Err(reason)) => {
            intro.push_str(&format!("\n\n(No audio transcript: {reason}.)"));
        }
        None => {}
    }

    let mut transport = effective_multimodal_transport(provider_id, provider_config);
    let mut blocks = Vec::new();
    for (index, at) in timestamps.iter().enumerate() {
        let frame_path = work_dir.join(format!("frame-{index:02}.jpg"));
        let bytes = extract_video_frame(input, *at, &frame_path).await?;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let source = MediaInputSource::Inline {
            mime_type: "image/jpeg".to_string(),
            data_url: format!("data:image/jpeg;base64,{base64_data}"),
            base64_data,
        };
        let label = format!("Frame at {}", format_timestamp(*at));
        let (frame_transport, frame_blocks) =
            build_image_analysis_blocks(provider_id, provider_config, &label, &source)?;
        transport = frame_transport;
        if blocks.is_empty() {
            let mut intro_block = frame_blocks[0].clone();
            intro_block["text"] = serde_json::Value::String(intro.clone());
            blocks.push(intro_block);
        }
        blocks.extend(frame_blocks);
    }

    let analysis = run_video_analysis_completion(
        http_client,
        provider_id,
        provider_config,
        transport,
        single_user_message(blocks),
    )
    .await?;
    let transcript_note = match transcript {
        Some(Ok(_)) => "with audio transcript",
        Some(Err(_)) => "no audio transcript",
        None => "transcript skipped",
    };
    Ok(format!(
        "[analyzed from {} frames by {provider_id}/{}, {transcript_note}]\n\n{analysis}",
        timestamps.len(),
        provider_config.model
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_spread_across_the_whole_clip_not_bunched_at_the_start() {
        assert_eq!(
            video_frame_timestamps(80.0, 4),
            vec![10.0, 30.0, 50.0, 70.0]
        );
        assert_eq!(video_frame_timestamps(0.0, 8), vec![0.0]);
        assert_eq!(
            video_frame_timestamps(100.0, 500).len(),
            MAX_VIDEO_FRAMES as usize
        );
    }

    #[test]
    fn exactly_one_video_source_is_required() {
        assert!(video_analysis_source(&serde_json::json!({})).is_err());
        assert!(video_analysis_source(&serde_json::json!({"url": "ftp://x/y.mp4"})).is_err());
        assert!(video_analysis_source(
            &serde_json::json!({"path": "/a.mp4", "url": "https://x/y.mp4"})
        )
        .is_err());
        assert_eq!(
            video_analysis_source(&serde_json::json!({"url": "https://x/y.mp4"})).unwrap(),
            VideoAnalysisSource::Remote("https://x/y.mp4".to_string())
        );
    }

    #[tokio::test]
    async fn ffmpeg_reads_duration_frames_and_audio_from_a_real_clip() {
        if std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_err()
        {
            eprintln!("skipping: ffmpeg not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=4:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=4",
                "-shortest",
                "-y",
            ])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(made.success());
        let input = clip.to_string_lossy().into_owned();

        let duration = probe_video_duration(&input).await.unwrap();
        assert!((3.5..=4.5).contains(&duration), "{duration}");

        let frame = extract_video_frame(&input, 2.0, &dir.path().join("f.jpg"))
            .await
            .unwrap();
        assert_eq!(&frame[..2], &[0xFF, 0xD8]);

        extract_video_audio(&input, &dir.path().join("a.wav"))
            .await
            .unwrap();
        assert!(dir.path().join("a.wav").metadata().unwrap().len() > 1000);
    }
}
