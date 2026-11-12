use super::*;

const VIDEO_POLL_INTERVAL: std::time::Duration = if cfg!(test) {
    std::time::Duration::from_millis(10)
} else {
    std::time::Duration::from_secs(5)
};
const DEFAULT_VIDEO_MIME_TYPE: &str = "video/mp4";
const DASHSCOPE_LEGACY_WAN_PREFIXES: &[&str] = &["wan2.1", "wan2.2", "wan2.5", "wan2.6"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoGenerationRoute {
    MiniMax,
    DashScopeWan,
    GeminiOmni,
    OpenRouter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VideoJobPoll<T> {
    Pending,
    Ready(T),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GeminiOmniVideo {
    Inline { mime_type: String, data: String },
    Uri(String),
}

struct GeneratedVideo {
    job_id: Option<String>,
    bytes: Vec<u8>,
    mime_type: String,
    remote_url: Option<String>,
}

fn video_generation_route(provider_id: &str) -> Option<VideoGenerationRoute> {
    if is_minimax_provider(provider_id) {
        return Some(VideoGenerationRoute::MiniMax);
    }
    match provider_id {
        zorai_shared::providers::PROVIDER_ID_QWEN => Some(VideoGenerationRoute::DashScopeWan),
        zorai_shared::providers::PROVIDER_ID_GEMINI => Some(VideoGenerationRoute::GeminiOmni),
        zorai_shared::providers::PROVIDER_ID_OPENROUTER => Some(VideoGenerationRoute::OpenRouter),
        _ => None,
    }
}

pub(super) fn provider_specific_default_video_model(provider_id: &str) -> Option<&'static str> {
    match video_generation_route(provider_id)? {
        VideoGenerationRoute::MiniMax => Some("MiniMax-Hailuo-2.3"),
        VideoGenerationRoute::DashScopeWan => Some("wan2.7-t2v"),
        VideoGenerationRoute::GeminiOmni => Some("gemini-omni-1.2-flash"),
        VideoGenerationRoute::OpenRouter => None,
    }
}

fn video_arg_str<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn video_arg_duration(args: &serde_json::Value) -> Option<u64> {
    args.get("duration_seconds").and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
    })
}

fn json_id_string(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn same_origin(left: &str, right: &str) -> bool {
    match (reqwest::Url::parse(left), reqwest::Url::parse(right)) {
        (Ok(left), Ok(right)) => {
            left.scheme() == right.scheme()
                && left.host_str() == right.host_str()
                && left.port_or_known_default() == right.port_or_known_default()
        }
        _ => false,
    }
}

fn build_openrouter_video_body(
    args: &serde_json::Value,
    model: &str,
    prompt: &str,
) -> serde_json::Value {
    let mut body = serde_json::json!({ "model": model, "prompt": prompt });
    if let Some(duration) = video_arg_duration(args) {
        body["duration"] = duration.into();
    }
    if let Some(size) = video_arg_str(args, "size") {
        body["size"] = size.into();
    } else {
        if let Some(resolution) = video_arg_str(args, "resolution") {
            body["resolution"] = resolution.into();
        }
        if let Some(aspect_ratio) = video_arg_str(args, "aspect_ratio") {
            body["aspect_ratio"] = aspect_ratio.into();
        }
    }
    body
}

fn parse_openrouter_video_job(payload: &serde_json::Value) -> Result<VideoJobPoll<String>> {
    match payload.get("status").and_then(|value| value.as_str()) {
        Some("completed") => payload
            .get("unsigned_urls")
            .and_then(|value| value.as_array())
            .and_then(|urls| urls.first())
            .and_then(|value| value.as_str())
            .map(|url| VideoJobPoll::Ready(url.to_string()))
            .ok_or_else(|| anyhow::anyhow!("OpenRouter video job completed without a video URL")),
        Some("failed" | "cancelled" | "expired") => {
            let reason = payload
                .get("error")
                .map(|error| {
                    error
                        .as_str()
                        .map(ToOwned::to_owned)
                        .or_else(|| {
                            error
                                .get("message")
                                .and_then(|value| value.as_str())
                                .map(ToOwned::to_owned)
                        })
                        .unwrap_or_else(|| error.to_string())
                })
                .unwrap_or_else(|| "no reason given".to_string());
            anyhow::bail!("OpenRouter video generation failed: {reason}")
        }
        _ => Ok(VideoJobPoll::Pending),
    }
}

fn build_minimax_video_body(
    args: &serde_json::Value,
    model: &str,
    prompt: &str,
) -> serde_json::Value {
    let mut body = serde_json::json!({ "model": model, "prompt": prompt });
    if let Some(duration) = video_arg_duration(args) {
        body["duration"] = duration.into();
    }
    if let Some(resolution) = video_arg_str(args, "resolution") {
        body["resolution"] = resolution.to_ascii_uppercase().into();
    }
    body
}

fn parse_minimax_video_task(payload: &serde_json::Value) -> Result<VideoJobPoll<String>> {
    if let Some(error) = minimax_base_resp_error(payload) {
        anyhow::bail!(error);
    }
    match payload.get("status").and_then(|value| value.as_str()) {
        Some("Success") => json_id_string(payload.get("file_id"))
            .map(VideoJobPoll::Ready)
            .ok_or_else(|| anyhow::anyhow!("MiniMax video task succeeded without a file_id")),
        Some("Fail") => anyhow::bail!("MiniMax video generation task failed"),
        _ => Ok(VideoJobPoll::Pending),
    }
}

fn dashscope_native_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let root = trimmed
        .strip_suffix("/compatible-mode/v1")
        .or_else(|| trimmed.strip_suffix("/api/v1"))
        .unwrap_or(trimmed);
    format!("{root}/api/v1")
}

fn build_dashscope_wan_video_body(
    args: &serde_json::Value,
    model: &str,
    prompt: &str,
) -> serde_json::Value {
    let mut input = serde_json::json!({ "prompt": prompt });
    if let Some(negative_prompt) = video_arg_str(args, "negative_prompt") {
        input["negative_prompt"] = negative_prompt.into();
    }
    let mut parameters = serde_json::json!({});
    if let Some(duration) = video_arg_duration(args) {
        parameters["duration"] = duration.into();
    }
    let legacy = DASHSCOPE_LEGACY_WAN_PREFIXES
        .iter()
        .any(|prefix| model.starts_with(prefix));
    if legacy {
        if let Some(size) = video_arg_str(args, "size") {
            parameters["size"] = size.replace(['x', 'X'], "*").into();
        }
    } else {
        if let Some(resolution) = video_arg_str(args, "resolution") {
            parameters["resolution"] = resolution.to_ascii_uppercase().into();
        }
        if let Some(aspect_ratio) = video_arg_str(args, "aspect_ratio") {
            parameters["ratio"] = aspect_ratio.into();
        }
    }
    serde_json::json!({ "model": model, "input": input, "parameters": parameters })
}

fn dashscope_error(payload: &serde_json::Value) -> Option<String> {
    let code = payload.get("code").and_then(|value| value.as_str())?;
    let message = payload
        .get("message")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown error");
    Some(format!("DashScope error {code}: {message}"))
}

fn parse_dashscope_wan_task(payload: &serde_json::Value) -> Result<VideoJobPoll<String>> {
    let output = payload.get("output").unwrap_or(&serde_json::Value::Null);
    match output.get("task_status").and_then(|value| value.as_str()) {
        Some("SUCCEEDED") => output
            .get("video_url")
            .and_then(|value| value.as_str())
            .map(|url| VideoJobPoll::Ready(url.to_string()))
            .ok_or_else(|| anyhow::anyhow!("DashScope video task succeeded without a video_url")),
        Some(status @ ("FAILED" | "CANCELED" | "UNKNOWN")) => {
            let detail = dashscope_error(output)
                .unwrap_or_else(|| format!("DashScope video task ended with status {status}"));
            anyhow::bail!(detail)
        }
        _ => Ok(VideoJobPoll::Pending),
    }
}

fn build_gemini_omni_video_body(
    args: &serde_json::Value,
    model: &str,
    prompt: &str,
) -> serde_json::Value {
    let mut response_format = serde_json::json!({ "type": "video" });
    if let Some(aspect_ratio) = video_arg_str(args, "aspect_ratio") {
        response_format["aspect_ratio"] = aspect_ratio.into();
    }
    if let Some(resolution) = video_arg_str(args, "resolution") {
        response_format["resolution"] = resolution.to_ascii_lowercase().into();
    }
    serde_json::json!({
        "model": model,
        "input": prompt,
        "response_format": response_format,
        "generation_config": { "video_config": { "task": "text_to_video" } },
    })
}

fn parse_gemini_omni_interaction(
    payload: &serde_json::Value,
) -> Result<VideoJobPoll<GeminiOmniVideo>> {
    if let Some(error) = payload.get("error") {
        let message = error
            .get("message")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| error.to_string());
        anyhow::bail!("Gemini video generation failed: {message}");
    }
    match payload.get("status").and_then(|value| value.as_str()) {
        Some("completed") => {}
        Some("failed" | "cancelled") => anyhow::bail!("Gemini video generation failed"),
        _ => return Ok(VideoJobPoll::Pending),
    }
    let video = payload
        .get("steps")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter(|step| step.get("type").and_then(|value| value.as_str()) == Some("model_output"))
        .filter_map(|step| step.get("content").and_then(|value| value.as_array()))
        .flatten()
        .find(|item| item.get("type").and_then(|value| value.as_str()) == Some("video"))
        .ok_or_else(|| anyhow::anyhow!("Gemini interaction completed without a video output"))?;
    if let Some(data) = video.get("data").and_then(|value| value.as_str()) {
        return Ok(VideoJobPoll::Ready(GeminiOmniVideo::Inline {
            mime_type: video
                .get("mime_type")
                .and_then(|value| value.as_str())
                .unwrap_or(DEFAULT_VIDEO_MIME_TYPE)
                .to_string(),
            data: data.to_string(),
        }));
    }
    video
        .get("uri")
        .and_then(|value| value.as_str())
        .map(|uri| VideoJobPoll::Ready(GeminiOmniVideo::Uri(uri.to_string())))
        .ok_or_else(|| anyhow::anyhow!("Gemini video output had neither data nor uri"))
}

async fn video_json_response(request: reqwest::RequestBuilder) -> Result<serde_json::Value> {
    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("video generation failed: HTTP {status} {text}");
    }
    Ok(response.json().await?)
}

async fn download_video(request: reqwest::RequestBuilder) -> Result<(Vec<u8>, String)> {
    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("video download failed: HTTP {status} {text}");
    }
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| value.starts_with("video/"))
        .unwrap_or(DEFAULT_VIDEO_MIME_TYPE)
        .to_string();
    let bytes = response.bytes().await?.to_vec();
    if bytes.is_empty() {
        anyhow::bail!("video download returned no bytes");
    }
    Ok((bytes, mime_type))
}

async fn wait_for_next_video_poll(deadline: tokio::time::Instant, job_id: &str) -> Result<()> {
    if tokio::time::Instant::now() + VIDEO_POLL_INTERVAL > deadline {
        anyhow::bail!(
            "video generation job {job_id} did not finish before the tool timeout; raise timeout_seconds or try a shorter clip"
        );
    }
    tokio::time::sleep(VIDEO_POLL_INTERVAL).await;
    Ok(())
}

async fn generate_openrouter_video(
    client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    args: &serde_json::Value,
    prompt: &str,
    deadline: tokio::time::Instant,
) -> Result<GeneratedVideo> {
    let endpoint = openai_like_endpoint(&provider_config.base_url, "videos");
    let body = build_openrouter_video_body(args, &provider_config.model, prompt);
    let created = video_json_response(
        apply_media_auth_headers(client.post(&endpoint), provider_id, provider_config)?.json(&body),
    )
    .await?;
    let job_id = json_id_string(created.get("id"))
        .ok_or_else(|| anyhow::anyhow!("OpenRouter video job response had no id"))?;
    let mut job = created;
    let video_url = loop {
        if let VideoJobPoll::Ready(url) = parse_openrouter_video_job(&job)? {
            break url;
        }
        wait_for_next_video_poll(deadline, &job_id).await?;
        job = video_json_response(apply_media_auth_headers(
            client.get(format!("{endpoint}/{job_id}")),
            provider_id,
            provider_config,
        )?)
        .await?;
    };
    let download = if same_origin(&video_url, &provider_config.base_url) {
        apply_media_auth_headers(client.get(&video_url), provider_id, provider_config)?
    } else {
        client.get(&video_url)
    };
    let (bytes, mime_type) = download_video(download).await?;
    Ok(GeneratedVideo {
        job_id: Some(job_id),
        bytes,
        mime_type,
        remote_url: None,
    })
}

async fn generate_minimax_video(
    client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    args: &serde_json::Value,
    prompt: &str,
    deadline: tokio::time::Instant,
) -> Result<GeneratedVideo> {
    let base = minimax_media_base_url(&provider_config.base_url);
    let body = build_minimax_video_body(args, &provider_config.model, prompt);
    let created = video_json_response(
        apply_media_auth_headers(
            client.post(format!("{base}/video_generation")),
            provider_id,
            provider_config,
        )?
        .json(&body),
    )
    .await?;
    if let Some(error) = minimax_base_resp_error(&created) {
        anyhow::bail!(error);
    }
    let task_id = json_id_string(created.get("task_id"))
        .ok_or_else(|| anyhow::anyhow!("MiniMax video generation response had no task_id"))?;
    let file_id = loop {
        let task = video_json_response(
            apply_media_auth_headers(
                client.get(format!("{base}/query/video_generation")),
                provider_id,
                provider_config,
            )?
            .query(&[("task_id", task_id.as_str())]),
        )
        .await?;
        if let VideoJobPoll::Ready(file_id) = parse_minimax_video_task(&task)? {
            break file_id;
        }
        wait_for_next_video_poll(deadline, &task_id).await?;
    };
    let file = video_json_response(
        apply_media_auth_headers(
            client.get(format!("{base}/files/retrieve")),
            provider_id,
            provider_config,
        )?
        .query(&[("file_id", file_id.as_str())]),
    )
    .await?;
    if let Some(error) = minimax_base_resp_error(&file) {
        anyhow::bail!(error);
    }
    let download_url = file
        .pointer("/file/download_url")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("MiniMax file {file_id} has no download_url"))?
        .to_string();
    let (bytes, mime_type) = download_video(client.get(&download_url)).await?;
    Ok(GeneratedVideo {
        job_id: Some(task_id),
        bytes,
        mime_type,
        remote_url: None,
    })
}

async fn generate_dashscope_wan_video(
    client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    args: &serde_json::Value,
    prompt: &str,
    deadline: tokio::time::Instant,
) -> Result<GeneratedVideo> {
    let base = dashscope_native_base_url(&provider_config.base_url);
    let body = build_dashscope_wan_video_body(args, &provider_config.model, prompt);
    let created = video_json_response(
        apply_media_auth_headers(
            client.post(format!(
                "{base}/services/aigc/video-generation/video-synthesis"
            )),
            provider_id,
            provider_config,
        )?
        .header("X-DashScope-Async", "enable")
        .json(&body),
    )
    .await?;
    if let Some(error) = dashscope_error(&created) {
        anyhow::bail!(error);
    }
    let task_id = json_id_string(created.pointer("/output/task_id"))
        .ok_or_else(|| anyhow::anyhow!("DashScope video response had no task_id"))?;
    let video_url = loop {
        let task = video_json_response(apply_media_auth_headers(
            client.get(format!("{base}/tasks/{task_id}")),
            provider_id,
            provider_config,
        )?)
        .await?;
        if let VideoJobPoll::Ready(url) = parse_dashscope_wan_task(&task)? {
            break url;
        }
        wait_for_next_video_poll(deadline, &task_id).await?;
    };
    let (bytes, mime_type) = download_video(client.get(&video_url)).await?;
    Ok(GeneratedVideo {
        job_id: Some(task_id),
        bytes,
        mime_type,
        remote_url: None,
    })
}

async fn generate_gemini_omni_video(
    client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    args: &serde_json::Value,
    prompt: &str,
    deadline: tokio::time::Instant,
) -> Result<GeneratedVideo> {
    let base = provider_config.base_url.trim().trim_end_matches('/');
    let body = build_gemini_omni_video_body(args, &provider_config.model, prompt);
    let mut interaction = video_json_response(
        apply_media_auth_headers(
            client.post(format!("{base}/interactions")),
            provider_id,
            provider_config,
        )?
        .json(&body),
    )
    .await?;
    let interaction_id = json_id_string(interaction.get("id"));
    let video = loop {
        if let VideoJobPoll::Ready(video) = parse_gemini_omni_interaction(&interaction)? {
            break video;
        }
        let Some(id) = interaction_id.as_deref() else {
            anyhow::bail!("Gemini interaction is still running but returned no id to poll");
        };
        wait_for_next_video_poll(deadline, id).await?;
        interaction = video_json_response(apply_media_auth_headers(
            client.get(format!("{base}/interactions/{id}")),
            provider_id,
            provider_config,
        )?)
        .await?;
    };
    let (bytes, mime_type, remote_url) = match video {
        GeminiOmniVideo::Inline { mime_type, data } => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|error| anyhow::anyhow!("invalid video base64 payload: {error}"))?;
            (bytes, mime_type, None)
        }
        GeminiOmniVideo::Uri(uri) => {
            let (bytes, mime_type) = download_video(apply_media_auth_headers(
                client.get(&uri),
                provider_id,
                provider_config,
            )?)
            .await?;
            (bytes, mime_type, Some(uri))
        }
    };
    Ok(GeneratedVideo {
        job_id: interaction_id,
        bytes,
        mime_type,
        remote_url,
    })
}

async fn persist_generated_video_for_thread(
    agent: &AgentEngine,
    thread_id: &str,
    prompt: &str,
    result_json: &str,
    include_prompt_message: bool,
) -> Result<String> {
    let payload: serde_json::Value = serde_json::from_str(result_json)?;
    let provider = video_arg_str(&payload, "provider")
        .unwrap_or("unknown")
        .to_string();
    let model = video_arg_str(&payload, "model")
        .unwrap_or("unknown")
        .to_string();
    let mime_type = video_arg_str(&payload, "mime_type")
        .unwrap_or(DEFAULT_VIDEO_MIME_TYPE)
        .to_string();
    let source = PathBuf::from(
        video_arg_str(&payload, "path")
            .ok_or_else(|| anyhow::anyhow!("generated video result has no path"))?,
    );
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| file_extension_for_generated_mime(&mime_type, "mp4"));
    let destination = thread_media_output_path(&agent.data_dir, thread_id, "video", &extension);
    move_generated_media_file(&source, &destination).await?;
    let file_url = file_url_for_path(&destination);

    {
        let mut threads = agent.threads.write().await;
        let Some(thread) = threads.get_mut(thread_id) else {
            anyhow::bail!("thread not found while persisting generated video");
        };
        if include_prompt_message {
            thread.messages.push(AgentMessage::user(
                format!("🎬 {}", prompt.trim()),
                now_millis(),
            ));
        }
        let mut message = AgentMessage::user("Generated video.", now_millis());
        message.role = MessageRole::Assistant;
        message.content_blocks = vec![AgentContentBlock::Video {
            url: Some(file_url.clone()),
            data_url: None,
            mime_type: Some(mime_type.clone()),
        }];
        message.provider = Some(provider.clone());
        message.model = Some(model.clone());
        message.author_agent_id = Some(current_agent_scope_id());
        message.author_agent_name = Some("assistant".to_string());
        thread.messages.push(message);
        thread.updated_at = now_millis();
    }
    agent.persist_thread_by_id(thread_id).await;
    agent
        .record_file_work_context(
            thread_id,
            None,
            tool_names::GENERATE_VIDEO,
            &destination.to_string_lossy(),
        )
        .await;

    serde_json::to_string_pretty(&serde_json::json!({
        "ok": true,
        "thread_id": thread_id,
        "provider": provider,
        "model": model,
        "mime_type": mime_type,
        "path": destination.to_string_lossy(),
        "file_url": file_url,
        "url": payload.get("url").and_then(|value| value.as_str()),
        "job_id": payload.get("job_id").and_then(|value| value.as_str()),
    }))
    .map_err(Into::into)
}

pub(crate) async fn execute_generate_video(
    args: &serde_json::Value,
    agent: &AgentEngine,
    _http_client: &reqwest::Client,
    thread_id: Option<&str>,
) -> Result<String> {
    let client = fresh_media_http_client(tool_names::GENERATE_VIDEO, args);
    let deadline =
        tokio::time::Instant::now() + media_tool_timeout(tool_names::GENERATE_VIDEO, args);
    let prompt = video_arg_str(args, "prompt")
        .ok_or_else(|| anyhow::anyhow!("missing 'prompt' argument"))?;
    let (provider_id, provider_config) =
        resolve_media_provider_config(agent, args, Some(MediaEndpoint::VideoGeneration)).await?;
    let route = video_generation_route(&provider_id).ok_or_else(|| {
        anyhow::anyhow!(
            "provider '{provider_id}' does not support video generation; use minimax, qwen, gemini, or openrouter"
        )
    })?;
    if provider_config.model.trim().is_empty() {
        anyhow::bail!(
            "no video generation model configured for provider '{provider_id}'; set video.generation.model"
        );
    }

    let generated = match route {
        VideoGenerationRoute::OpenRouter => {
            generate_openrouter_video(
                &client,
                &provider_id,
                &provider_config,
                args,
                prompt,
                deadline,
            )
            .await?
        }
        VideoGenerationRoute::MiniMax => {
            generate_minimax_video(
                &client,
                &provider_id,
                &provider_config,
                args,
                prompt,
                deadline,
            )
            .await?
        }
        VideoGenerationRoute::DashScopeWan => {
            generate_dashscope_wan_video(
                &client,
                &provider_id,
                &provider_config,
                args,
                prompt,
                deadline,
            )
            .await?
        }
        VideoGenerationRoute::GeminiOmni => {
            generate_gemini_omni_video(
                &client,
                &provider_id,
                &provider_config,
                args,
                prompt,
                deadline,
            )
            .await?
        }
    };

    let extension = file_extension_for_generated_mime(&generated.mime_type, "mp4");
    let output_path = temp_output_path("video", &extension);
    tokio::fs::write(&output_path, &generated.bytes).await?;
    let result = serde_json::to_string_pretty(&serde_json::json!({
        "provider": provider_id,
        "model": provider_config.model,
        "path": output_path,
        "mime_type": generated.mime_type,
        "bytes": generated.bytes.len(),
        "job_id": generated.job_id,
        "url": generated.remote_url,
    }))?;
    if let Some(thread_id) = thread_id {
        return persist_generated_video_for_thread(agent, thread_id, prompt, &result, false).await;
    }
    Ok(result)
}

pub(crate) async fn execute_generate_video_for_ipc(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let prompt = video_arg_str(args, "prompt")
        .ok_or_else(|| anyhow::anyhow!("missing 'prompt' argument"))?
        .to_string();
    let requested_thread_id = video_arg_str(args, "thread_id");
    let (thread_id, is_new_thread) = agent
        .get_or_create_thread(requested_thread_id, &prompt)
        .await;
    agent.ensure_thread_messages_loaded(&thread_id).await;

    let upstream_result = execute_generate_video(args, agent, &agent.http_client, None).await?;
    let persisted =
        persist_generated_video_for_thread(agent, &thread_id, &prompt, &upstream_result, true)
            .await?;
    agent
        .record_operator_message(&thread_id, &format!("🎬 {}", prompt), is_new_thread)
        .await?;
    Ok(persisted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    type SeenRequests = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

    struct MockResponse {
        path_prefix: &'static str,
        content_type: &'static str,
        body: Vec<u8>,
    }

    fn json_response(path_prefix: &'static str, body: serde_json::Value) -> MockResponse {
        MockResponse {
            path_prefix,
            content_type: "application/json",
            body: body.to_string().into_bytes(),
        }
    }

    fn video_response(path_prefix: &'static str, body: &[u8]) -> MockResponse {
        MockResponse {
            path_prefix,
            content_type: "video/mp4",
            body: body.to_vec(),
        }
    }

    async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let read = socket.read(&mut chunk).await.unwrap_or(0);
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&buffer).to_string();
            let Some(head_end) = text.find("\r\n\r\n") else {
                continue;
            };
            let content_length = text[..head_end]
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if buffer.len() >= head_end + 4 + content_length {
                break;
            }
        }
        String::from_utf8_lossy(&buffer).to_string()
    }

    async fn spawn_video_provider_mock(responses: Vec<MockResponse>) -> (String, SeenRequests) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock listener should bind");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: SeenRequests = Default::default();
        let recorded = seen.clone();
        let served_base = base.clone();
        tokio::spawn(async move {
            let mut responses = std::collections::VecDeque::from(responses);
            while let Ok((mut socket, _)) = listener.accept().await {
                let request = read_http_request(&mut socket).await;
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or_default()
                    .to_string();
                recorded.lock().unwrap().push(request);
                let next = match responses.front() {
                    Some(next) if path.starts_with(next.path_prefix) => responses.pop_front(),
                    _ => None,
                };
                let (status, content_type, body) = match next {
                    Some(next) => (
                        "200 OK",
                        next.content_type,
                        String::from_utf8_lossy(&next.body)
                            .replace("{BASE}", &served_base)
                            .into_bytes(),
                    ),
                    None => (
                        "404 Not Found",
                        "text/plain",
                        format!("unexpected request {path}").into_bytes(),
                    ),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(head.as_bytes()).await.ok();
                socket.write_all(&body).await.ok();
                socket.shutdown().await.ok();
            }
        });
        (base, seen)
    }

    fn mock_provider_config(base_url: String) -> crate::agent::types::ProviderConfig {
        crate::agent::types::ProviderConfig {
            base_url,
            model: "chat-model".to_string(),
            api_key: "sk-video".to_string(),
            assistant_id: String::new(),
            auth_source: crate::agent::types::AuthSource::ApiKey,
            api_transport: crate::agent::types::ApiTransport::ChatCompletions,
            reasoning_effort: "off".to_string(),
            context_window_tokens: 128_000,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        }
    }

    async fn engine_with_video_provider(
        root: &std::path::Path,
        provider_id: &str,
        base_url: String,
        model: Option<&str>,
    ) -> std::sync::Arc<AgentEngine> {
        let manager = crate::session_manager::SessionManager::new_test(root).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4".to_string();
        config.api_key = "sk-chat".to_string();
        let mut generation = serde_json::json!({ "provider": provider_id });
        if let Some(model) = model {
            generation["model"] = model.into();
        }
        config.extra.insert(
            "video".to_string(),
            serde_json::json!({ "generation": generation }),
        );
        config
            .providers
            .insert(provider_id.to_string(), mock_provider_config(base_url));
        AgentEngine::new_test(manager, config, root).await
    }

    async fn assert_thread_shows_generated_video(
        engine: &AgentEngine,
        thread_id: &str,
        result: &str,
        expected_bytes: &[u8],
    ) {
        let result: serde_json::Value = serde_json::from_str(result).expect("result is JSON");
        let path = result["path"].as_str().expect("result has a saved path");
        assert!(
            path.contains("/threads/")
                && path.contains("/artifacts/media/")
                && path.ends_with(".mp4"),
            "video should be saved as an mp4 in the thread media dir, got {path}"
        );
        assert_eq!(tokio::fs::read(path).await.unwrap(), expected_bytes);
        let thread = engine
            .get_thread_filtered(thread_id, true, None, 0)
            .await
            .expect("thread should exist");
        let last = thread.thread.messages.last().expect("thread has messages");
        assert_eq!(last.role, MessageRole::Assistant);
        assert!(matches!(
            last.content_blocks.first(),
            Some(AgentContentBlock::Video { url: Some(url), mime_type: Some(mime), .. })
                if url == &format!("file://{path}") && mime == "video/mp4"
        ));
        let context = engine
            .thread_work_contexts
            .read()
            .await
            .get(thread_id)
            .cloned()
            .expect("work context should be recorded");
        assert_eq!(context.entries[0].source, tool_names::GENERATE_VIDEO);
    }

    #[tokio::test]
    async fn openrouter_video_job_is_polled_until_complete_then_saved_into_the_thread() {
        let root = tempfile::tempdir().unwrap();
        let (base, seen) = spawn_video_provider_mock(vec![
            json_response(
                "/v1/videos",
                serde_json::json!({ "id": "job-1", "status": "pending" }),
            ),
            json_response(
                "/v1/videos/job-1",
                serde_json::json!({ "id": "job-1", "status": "in_progress" }),
            ),
            json_response(
                "/v1/videos/job-1",
                serde_json::json!({
                    "id": "job-1",
                    "status": "completed",
                    "unsigned_urls": ["{BASE}/v1/videos/job-1/content?index=0"]
                }),
            ),
            video_response("/v1/videos/job-1/content", b"openrouter-mp4"),
        ])
        .await;
        let engine = engine_with_video_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_OPENROUTER,
            format!("{base}/v1"),
            Some("acme/video-1"),
        )
        .await;
        let (thread_id, _) = engine.get_or_create_thread(None, "foggy forest").await;

        let result = execute_generate_video(
            &serde_json::json!({ "prompt": "foggy forest", "duration_seconds": 4, "aspect_ratio": "16:9" }),
            &engine,
            &engine.http_client,
            Some(&thread_id),
        )
        .await
        .expect("video generation should succeed");

        assert_thread_shows_generated_video(&engine, &thread_id, &result, b"openrouter-mp4").await;
        let requests = seen.lock().unwrap().clone();
        assert_eq!(requests.len(), 4, "create, two polls, download");
        assert!(requests[0].starts_with("POST /v1/videos "));
        assert!(requests[0].contains("\"model\":\"acme/video-1\""));
        assert!(requests[0].contains("\"duration\":4"));
        assert!(
            requests[3]
                .to_ascii_lowercase()
                .contains("authorization: bearer sk-video"),
            "the content URL is on the provider origin, so the download must be authenticated"
        );
    }

    #[tokio::test]
    async fn minimax_video_task_is_resolved_through_its_file_id_before_download() {
        let root = tempfile::tempdir().unwrap();
        let ok = serde_json::json!({ "status_code": 0, "status_msg": "success" });
        let (base, seen) = spawn_video_provider_mock(vec![
            json_response(
                "/v1/video_generation",
                serde_json::json!({ "task_id": "task-9", "base_resp": ok }),
            ),
            json_response(
                "/v1/query/video_generation?task_id=task-9",
                serde_json::json!({ "task_id": "task-9", "status": "Success", "file_id": 4242, "base_resp": ok }),
            ),
            json_response(
                "/v1/files/retrieve?file_id=4242",
                serde_json::json!({
                    "file": { "file_id": 4242, "download_url": "{BASE}/download/video.mp4" },
                    "base_resp": ok
                }),
            ),
            video_response("/download/video.mp4", b"minimax-mp4"),
        ])
        .await;
        let engine = engine_with_video_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_MINIMAX,
            base.clone(),
            None,
        )
        .await;
        let (thread_id, _) = engine.get_or_create_thread(None, "city timelapse").await;

        let result = execute_generate_video(
            &serde_json::json!({ "prompt": "city timelapse", "resolution": "1080p" }),
            &engine,
            &engine.http_client,
            Some(&thread_id),
        )
        .await
        .expect("video generation should succeed");

        assert_thread_shows_generated_video(&engine, &thread_id, &result, b"minimax-mp4").await;
        let requests = seen.lock().unwrap().clone();
        assert!(
            requests[0].contains("\"model\":\"MiniMax-Hailuo-2.3\""),
            "without a configured model the MiniMax default video model is used"
        );
        assert!(requests[0].contains("\"resolution\":\"1080P\""));
    }

    #[tokio::test]
    async fn minimax_base_resp_failure_is_reported_even_when_http_succeeds() {
        let root = tempfile::tempdir().unwrap();
        let (base, _seen) = spawn_video_provider_mock(vec![json_response(
            "/v1/video_generation",
            serde_json::json!({
                "task_id": "",
                "base_resp": { "status_code": 1026, "status_msg": "input contains sensitive content" }
            }),
        )])
        .await;
        let engine = engine_with_video_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_MINIMAX,
            base,
            None,
        )
        .await;

        let error = execute_generate_video(
            &serde_json::json!({ "prompt": "something" }),
            &engine,
            &engine.http_client,
            None,
        )
        .await
        .expect_err("a non-zero base_resp must fail the tool");
        assert!(error.to_string().contains("sensitive content"), "{error}");
    }

    #[tokio::test]
    async fn dashscope_wan_task_is_submitted_async_and_polled_until_succeeded() {
        let root = tempfile::tempdir().unwrap();
        let (base, seen) = spawn_video_provider_mock(vec![
            json_response(
                "/api/v1/services/aigc/video-generation/video-synthesis",
                serde_json::json!({ "output": { "task_id": "wan-1", "task_status": "PENDING" } }),
            ),
            json_response(
                "/api/v1/tasks/wan-1",
                serde_json::json!({
                    "output": { "task_status": "SUCCEEDED", "video_url": "{BASE}/oss/wan.mp4" }
                }),
            ),
            video_response("/oss/wan.mp4", b"wan-mp4"),
        ])
        .await;
        let engine = engine_with_video_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_QWEN,
            format!("{base}/compatible-mode/v1"),
            None,
        )
        .await;
        let (thread_id, _) = engine.get_or_create_thread(None, "koi pond").await;

        let result = execute_generate_video(
            &serde_json::json!({ "prompt": "koi pond", "aspect_ratio": "9:16" }),
            &engine,
            &engine.http_client,
            Some(&thread_id),
        )
        .await
        .expect("video generation should succeed");

        assert_thread_shows_generated_video(&engine, &thread_id, &result, b"wan-mp4").await;
        let create = seen.lock().unwrap()[0].to_ascii_lowercase();
        assert!(
            create.contains("x-dashscope-async: enable"),
            "DashScope rejects synchronous video calls"
        );
        assert!(create.contains("\"model\":\"wan2.7-t2v\""));
        assert!(create.contains("\"ratio\":\"9:16\""));
    }

    #[tokio::test]
    async fn gemini_omni_inline_video_is_decoded_and_saved() {
        let root = tempfile::tempdir().unwrap();
        let data = base64::engine::general_purpose::STANDARD.encode(b"omni-mp4");
        let (base, seen) = spawn_video_provider_mock(vec![json_response(
            "/v1beta/interactions",
            serde_json::json!({
                "id": "int-1",
                "status": "completed",
                "steps": [
                    { "type": "thought" },
                    { "type": "model_output", "content": [
                        { "type": "text", "text": "here" },
                        { "type": "video", "mime_type": "video/mp4", "data": data }
                    ] }
                ]
            }),
        )])
        .await;
        let engine = engine_with_video_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_GEMINI,
            format!("{base}/v1beta"),
            None,
        )
        .await;
        let (thread_id, _) = engine.get_or_create_thread(None, "neon city").await;

        let result = execute_generate_video(
            &serde_json::json!({ "prompt": "neon city", "resolution": "720P" }),
            &engine,
            &engine.http_client,
            Some(&thread_id),
        )
        .await
        .expect("video generation should succeed");

        assert_thread_shows_generated_video(&engine, &thread_id, &result, b"omni-mp4").await;
        let create = seen.lock().unwrap()[0].clone();
        assert!(create
            .to_ascii_lowercase()
            .contains("x-goog-api-key: sk-video"));
        assert!(create.contains("\"model\":\"gemini-omni-1.2-flash\""));
        assert!(create.contains("\"resolution\":\"720p\""));
    }

    #[tokio::test]
    async fn providers_without_a_video_api_are_rejected_before_any_request() {
        let root = tempfile::tempdir().unwrap();
        let (base, seen) = spawn_video_provider_mock(Vec::new()).await;
        let engine = engine_with_video_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_OPENAI,
            base,
            Some("sora-2"),
        )
        .await;

        let error = execute_generate_video(
            &serde_json::json!({ "prompt": "anything" }),
            &engine,
            &engine.http_client,
            None,
        )
        .await
        .expect_err("openai has no video API");
        assert!(
            error
                .to_string()
                .contains("does not support video generation"),
            "{error}"
        );
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn openrouter_without_a_configured_video_model_fails_instead_of_using_the_chat_model() {
        let root = tempfile::tempdir().unwrap();
        let (base, seen) = spawn_video_provider_mock(Vec::new()).await;
        let engine = engine_with_video_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_OPENROUTER,
            base,
            None,
        )
        .await;

        let error = execute_generate_video(
            &serde_json::json!({ "prompt": "anything" }),
            &engine,
            &engine.http_client,
            None,
        )
        .await
        .expect_err("openrouter has no default video model");
        assert!(
            error.to_string().contains("no video generation model"),
            "{error}"
        );
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn legacy_wan_models_take_pixel_sizes_while_wan27_takes_resolution_and_ratio() {
        let args = serde_json::json!({
            "size": "1280x720",
            "resolution": "720p",
            "aspect_ratio": "16:9",
            "duration_seconds": 5
        });
        let legacy = build_dashscope_wan_video_body(&args, "wan2.5-t2v-preview", "p");
        assert_eq!(legacy["parameters"]["size"], "1280*720");
        assert!(legacy["parameters"].get("ratio").is_none());
        let current = build_dashscope_wan_video_body(&args, "wan2.7-t2v", "p");
        assert_eq!(current["parameters"]["resolution"], "720P");
        assert_eq!(current["parameters"]["ratio"], "16:9");
        assert!(current["parameters"].get("size").is_none());
        assert_eq!(current["parameters"]["duration"], 5);
    }

    #[test]
    fn failed_jobs_surface_the_provider_reason() {
        let openrouter = parse_openrouter_video_job(
            &serde_json::json!({ "status": "failed", "error": "content policy" }),
        )
        .unwrap_err();
        assert!(openrouter.to_string().contains("content policy"));
        let wan = parse_dashscope_wan_task(&serde_json::json!({
            "output": { "task_status": "FAILED", "code": "DataInspectionFailed", "message": "unsafe" }
        }))
        .unwrap_err();
        assert_eq!(
            wan.to_string(),
            "DashScope error DataInspectionFailed: unsafe"
        );
    }

    #[test]
    fn dashscope_native_base_url_is_derived_from_the_compatible_mode_url() {
        assert_eq!(
            dashscope_native_base_url("https://dashscope-intl.aliyuncs.com/compatible-mode/v1/"),
            "https://dashscope-intl.aliyuncs.com/api/v1"
        );
        assert_eq!(
            dashscope_native_base_url("https://ws1.ap-southeast-1.maas.aliyuncs.com/api/v1"),
            "https://ws1.ap-southeast-1.maas.aliyuncs.com/api/v1"
        );
    }

    #[tokio::test]
    async fn generate_video_tool_is_offered_only_once_a_video_model_is_configured() {
        let mut config = AgentConfig::default();
        let offered = |config: &AgentConfig| {
            let mut tools = Vec::new();
            crate::agent::tool_executor::add_available_tools_part_c(
                &mut tools,
                config,
                std::path::Path::new("/tmp"),
                false,
            );
            tools
                .iter()
                .any(|tool| tool.function.name == tool_names::GENERATE_VIDEO)
        };
        assert!(!offered(&config));
        config.extra.insert(
            "video".to_string(),
            serde_json::json!({ "generation": { "provider": "minimax", "model": "MiniMax-Hailuo-2.3" } }),
        );
        assert!(offered(&config));
    }
}
