use futures::StreamExt;

use super::*;

const DEFAULT_IMAGE_ANALYSIS_PROMPT: &str = "Analyze this image in detail.";
const DEFAULT_IMAGE_GENERATION_MODEL: &str = "gpt-image-1";
const DEFAULT_STT_MODEL: &str = "whisper-1";
const DEFAULT_TTS_MODEL: &str = "gpt-4o-mini-tts";
const DEFAULT_TTS_VOICE: &str = "alloy";
const DEFAULT_XAI_TTS_VOICE: &str = "eve";
const DEFAULT_MINIMAX_TTS_VOICE: &str = "English_expressive_narrator";
const DEFAULT_IMAGE_OUTPUT_FORMAT: &str = "png";
const DEFAULT_TTS_OUTPUT_FORMAT: &str = "mp3";

const OPENROUTER_ATTRIBUTION_URL: &str = "https://zorai.app";
const OPENROUTER_ATTRIBUTION_TITLE: &str = "zorai";
const OPENROUTER_ATTRIBUTION_CATEGORIES: &str = "cli-agent,personal-agent";

fn media_tool_timeout(tool_name: &str, args: &serde_json::Value) -> std::time::Duration {
    std::time::Duration::from_secs(daemon_tool_timeout_seconds(tool_name, args))
}

fn fresh_media_http_client(tool_name: &str, args: &serde_json::Value) -> reqwest::Client {
    crate::agent::build_fresh_agent_http_client(media_tool_timeout(tool_name, args))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    Image,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioToolRoute {
    OpenAiCompatibleDirect,
    MiniMaxTts,
    ProviderMultimodalCompletion,
    OpenRouterJsonStt,
    OpenRouterTts,
    XiaomiChatCompletionsTts,
    XaiStt,
    XaiTts,
    ElevenLabs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageGenerationRoute {
    MiniMaxDirect,
    OpenAiCompatibleDirect,
    OpenRouterChatCompletions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OrderedMultipartField {
    Text {
        name: String,
        value: String,
    },
    File {
        name: String,
        filename: String,
        mime_type: String,
    },
}

#[derive(Debug, Clone)]
enum MediaInputSource {
    RemoteUrl {
        url: String,
    },
    Inline {
        mime_type: String,
        base64_data: String,
        data_url: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaEndpoint {
    ImageGeneration,
    SpeechToText,
    TextToSpeech,
}

impl MediaEndpoint {
    fn default_model(self) -> &'static str {
        match self {
            Self::ImageGeneration => DEFAULT_IMAGE_GENERATION_MODEL,
            Self::SpeechToText => DEFAULT_STT_MODEL,
            Self::TextToSpeech => DEFAULT_TTS_MODEL,
        }
    }

    fn nested_root(self) -> &'static str {
        match self {
            Self::ImageGeneration => "image",
            Self::SpeechToText | Self::TextToSpeech => "audio",
        }
    }

    fn nested_group(self) -> &'static [&'static str] {
        match self {
            Self::ImageGeneration => &["generation"],
            Self::SpeechToText => &["stt"],
            Self::TextToSpeech => &["tts"],
        }
    }

    fn legacy_prefix(self) -> &'static str {
        match self {
            Self::ImageGeneration => "image_generation",
            Self::SpeechToText => "audio_stt",
            Self::TextToSpeech => "audio_tts",
        }
    }
}

fn media_input_source_arg_count(args: &serde_json::Value) -> usize {
    ["path", "url", "base64", "data_url"]
        .iter()
        .filter(|key| {
            args.get(**key)
                .and_then(|value| value.as_str())
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
        })
        .count()
}

fn openai_like_endpoint(base_url: &str, endpoint: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let endpoint = endpoint.trim_start_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/{endpoint}")
    } else {
        format!("{base}/v1/{endpoint}")
    }
}

fn infer_media_mime(path: &Path, kind: MediaKind) -> &'static str {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    match (kind, extension.as_str()) {
        (MediaKind::Image, "png") => "image/png",
        (MediaKind::Image, "jpg" | "jpeg") => "image/jpeg",
        (MediaKind::Image, "webp") => "image/webp",
        (MediaKind::Image, "gif") => "image/gif",
        (MediaKind::Image, "bmp") => "image/bmp",
        (MediaKind::Image, "svg") => "image/svg+xml",
        (MediaKind::Image, "tif" | "tiff") => "image/tiff",
        (MediaKind::Image, _) => "application/octet-stream",
        (MediaKind::Audio, "mp3") => "audio/mpeg",
        (MediaKind::Audio, "wav") => "audio/wav",
        (MediaKind::Audio, "ogg") => "audio/ogg",
        (MediaKind::Audio, "m4a") => "audio/mp4",
        (MediaKind::Audio, "mp4") => "audio/mp4",
        (MediaKind::Audio, "flac") => "audio/flac",
        (MediaKind::Audio, "webm") => "audio/webm",
        (MediaKind::Audio, _) => "application/octet-stream",
    }
}

fn file_extension_for_generated_mime(mime_type: &str, fallback: &str) -> String {
    match mime_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "audio/mpeg" => "mp3",
        "audio/L16" => "pcm",
        "audio/wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/mp4" => "m4a",
        "audio/webm" => "webm",
        _ => fallback,
    }
    .to_string()
}

fn image_generation_route(provider_id: &str) -> ImageGenerationRoute {
    if provider_id == zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        ImageGenerationRoute::OpenRouterChatCompletions
    } else if is_minimax_provider(provider_id) {
        ImageGenerationRoute::MiniMaxDirect
    } else {
        ImageGenerationRoute::OpenAiCompatibleDirect
    }
}

fn is_minimax_provider(provider_id: &str) -> bool {
    matches!(
        provider_id,
        zorai_shared::providers::PROVIDER_ID_MINIMAX
            | zorai_shared::providers::PROVIDER_ID_MINIMAX_CODING_PLAN
    )
}

fn openrouter_image_generation_modalities(model: &str) -> &'static [&'static str] {
    let normalized = model.trim().to_ascii_lowercase();
    let image_only = [
        "flux",
        "sourceful",
        "recraft",
        "stable-diffusion",
        "sdxl",
        "imagen",
    ]
    .iter()
    .any(|needle| normalized.contains(needle));

    if image_only {
        &["image"]
    } else {
        &["image", "text"]
    }
}

fn openrouter_image_aspect_ratio(size: &str) -> Option<&'static str> {
    match size.trim() {
        "1024x1024" | "512x512" | "2048x2048" | "4096x4096" => Some("1:1"),
        "832x1248" => Some("2:3"),
        "1248x832" => Some("3:2"),
        "864x1184" => Some("3:4"),
        "1184x864" => Some("4:3"),
        "896x1152" => Some("4:5"),
        "1152x896" => Some("5:4"),
        "768x1344" => Some("9:16"),
        "1344x768" => Some("16:9"),
        "1536x672" => Some("21:9"),
        _ => None,
    }
}

fn openrouter_image_size_bucket(size: &str) -> Option<&'static str> {
    let (width, height) = size.trim().split_once('x')?;
    let width: u32 = width.parse().ok()?;
    let height: u32 = height.parse().ok()?;
    let max_dimension = width.max(height);
    if max_dimension <= 512 {
        Some("0.5K")
    } else if max_dimension <= 1536 {
        Some("1K")
    } else if max_dimension <= 3072 {
        Some("2K")
    } else {
        Some("4K")
    }
}

fn build_openrouter_image_generation_body(
    args: &serde_json::Value,
    provider_config: &crate::agent::types::ProviderConfig,
    prompt: &str,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": provider_config.model,
        "messages": [{
            "role": "user",
            "content": prompt,
        }],
        "modalities": openrouter_image_generation_modalities(&provider_config.model),
        "stream": false,
    });

    let mut image_config = serde_json::Map::new();
    if let Some(size) = args
        .get("size")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(aspect_ratio) = openrouter_image_aspect_ratio(size) {
            image_config.insert(
                "aspect_ratio".to_string(),
                serde_json::Value::String(aspect_ratio.to_string()),
            );
        }
        if let Some(image_size) = openrouter_image_size_bucket(size) {
            image_config.insert(
                "image_size".to_string(),
                serde_json::Value::String(image_size.to_string()),
            );
        }
    }

    if !image_config.is_empty() {
        body["image_config"] = serde_json::Value::Object(image_config);
    }

    body
}

fn extract_openrouter_generated_image(payload: &serde_json::Value) -> Result<String> {
    let first = payload
        .get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.get("message"))
        .and_then(|value| value.get("images"))
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .ok_or_else(|| anyhow::anyhow!("image generation returned no image payload"))?;

    let image_value = first
        .get("image_url")
        .or_else(|| first.get("imageUrl"))
        .ok_or_else(|| anyhow::anyhow!("image generation returned no image URL"))?;

    image_value
        .get("url")
        .and_then(|value| value.as_str())
        .or_else(|| image_value.as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("image generation returned no image URL"))
}

fn generated_image_response_from_data_url(
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    data_url: &str,
) -> Result<String> {
    let (mime_type, base64_data) = parse_data_url(data_url)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|error| anyhow::anyhow!("invalid image base64 payload: {error}"))?;
    let extension = file_extension_for_generated_mime(&mime_type, DEFAULT_IMAGE_OUTPUT_FORMAT);
    let output_path = temp_output_path("image", &extension);
    std::fs::write(&output_path, &bytes)?;
    serde_json::to_string_pretty(&serde_json::json!({
        "provider": provider_id,
        "model": provider_config.model,
        "path": output_path,
        "mime_type": mime_type,
        "bytes": bytes.len(),
    }))
    .map_err(Into::into)
}

fn data_url_from_base64(mime_type: &str, base64_data: &str) -> String {
    format!("data:{mime_type};base64,{base64_data}")
}

fn audio_format_from_mime_type(mime_type: &str) -> Option<&'static str> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/ogg" => Some("ogg"),
        "audio/flac" => Some("flac"),
        "audio/mp4" | "audio/m4a" => Some("mp4"),
        "audio/webm" => Some("webm"),
        _ => None,
    }
}

fn parse_data_url(raw: &str) -> Result<(String, String)> {
    let trimmed = raw.trim();
    let header_end = trimmed
        .find(",")
        .ok_or_else(|| anyhow::anyhow!("invalid data URL: missing comma separator"))?;
    let (header, data) = trimmed.split_at(header_end);
    let data = data.trim_start_matches(',');
    if !header.starts_with("data:") {
        anyhow::bail!("invalid data URL: missing data: prefix");
    }
    let mime_type = header
        .trim_start_matches("data:")
        .split(';')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream")
        .to_string();
    if !header.contains(";base64") {
        anyhow::bail!("invalid data URL: only base64 payloads are supported");
    }
    if data.trim().is_empty() {
        anyhow::bail!("invalid data URL: empty payload");
    }
    Ok((mime_type, data.trim().to_string()))
}

async fn resolve_media_input_source(
    args: &serde_json::Value,
    kind: MediaKind,
) -> Result<MediaInputSource> {
    let provided = media_input_source_arg_count(args);
    if provided == 0 {
        anyhow::bail!("provide exactly one of 'path', 'url', 'base64', or 'data_url'");
    }
    if provided > 1 {
        anyhow::bail!("arguments 'path', 'url', 'base64', and 'data_url' are mutually exclusive");
    }

    if let Some(url) = args
        .get("url")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let parsed = url::Url::parse(url)
            .map_err(|error| anyhow::anyhow!("invalid 'url' argument: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            anyhow::bail!("'url' must use http or https");
        }
        return Ok(MediaInputSource::RemoteUrl {
            url: url.to_string(),
        });
    }

    if let Some(data_url) = args
        .get("data_url")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let (mime_type, base64_data) = parse_data_url(data_url)?;
        return Ok(MediaInputSource::Inline {
            mime_type,
            base64_data,
            data_url: data_url.to_string(),
        });
    }

    if let Some(base64_data) = args
        .get("base64")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let mime_type = args
            .get("mime_type")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("'mime_type' is required when using 'base64'"))?
            .to_string();
        return Ok(MediaInputSource::Inline {
            data_url: data_url_from_base64(&mime_type, base64_data),
            mime_type,
            base64_data: base64_data.to_string(),
        });
    }

    let path = args
        .get("path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing media input path"))?;
    validate_read_path(path)?;
    let resolved = resolve_tool_path(path, None);
    let bytes = tokio::fs::read(&resolved).await?;
    let mime_type = args
        .get("mime_type")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| infer_media_mime(&resolved, kind).to_string());
    let base64_data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(MediaInputSource::Inline {
        data_url: data_url_from_base64(&mime_type, &base64_data),
        mime_type,
        base64_data,
    })
}

fn provider_specific_default_media_model(
    provider_id: &str,
    endpoint: MediaEndpoint,
) -> Option<&'static str> {
    if provider_id == zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        return match endpoint {
            MediaEndpoint::TextToSpeech => Some("openai/gpt-4o-mini-tts"),
            MediaEndpoint::SpeechToText => Some("openai/whisper-1"),
            MediaEndpoint::ImageGeneration => None,
        };
    }
    None
}

fn select_media_provider_config(
    config: &AgentConfig,
    provider_id: &str,
    mut provider_config: crate::agent::types::ProviderConfig,
    explicit_model: Option<&str>,
    default_model: Option<&str>,
    endpoint: Option<MediaEndpoint>,
) -> (String, crate::agent::types::ProviderConfig) {
    let resolved_provider_id = if provider_id.trim().is_empty() {
        config.provider.clone()
    } else {
        provider_id.to_string()
    };
    let provider_specific_default = endpoint.and_then(|endpoint| {
        provider_specific_default_media_model(&resolved_provider_id, endpoint)
    });
    let chosen_model = explicit_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| provider_specific_default.map(ToOwned::to_owned))
        .or_else(|| default_model.map(ToOwned::to_owned))
        .unwrap_or_else(|| provider_config.model.clone());
    provider_config.model = chosen_model;
    (resolved_provider_id, provider_config)
}

fn media_setting_string(
    config: &AgentConfig,
    nested_root: &str,
    nested_path: &[&str],
    legacy_flat_key: &str,
) -> Option<String> {
    config
        .extra
        .get(nested_root)
        .and_then(|value| {
            nested_path
                .iter()
                .try_fold(value, |acc, key| acc.get(*key))
                .and_then(|value| value.as_str())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            config
                .extra
                .get(legacy_flat_key)
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

fn media_persisted_provider_model(
    config: &AgentConfig,
    endpoint: MediaEndpoint,
) -> (Option<String>, Option<String>) {
    let nested_root = endpoint.nested_root();
    let nested_group = endpoint.nested_group();
    let legacy_prefix = endpoint.legacy_prefix();
    let mut provider_path = nested_group.to_vec();
    provider_path.push("provider");
    let mut model_path = nested_group.to_vec();
    model_path.push("model");
    let provider = media_setting_string(
        config,
        nested_root,
        &provider_path,
        &format!("{legacy_prefix}_provider"),
    );
    let model = media_setting_string(
        config,
        nested_root,
        &model_path,
        &format!("{legacy_prefix}_model"),
    );
    (provider, model)
}

async fn resolve_media_provider_config(
    agent: &AgentEngine,
    args: &serde_json::Value,
    endpoint: Option<MediaEndpoint>,
) -> Result<(String, crate::agent::types::ProviderConfig)> {
    let config = agent.get_config().await;
    let explicit_provider = args
        .get("provider")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let explicit_model = args
        .get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let (persisted_provider, persisted_model) = endpoint
        .map(|endpoint| media_persisted_provider_model(&config, endpoint))
        .unwrap_or((None, None));
    let selected_provider = explicit_provider.or(persisted_provider.as_deref());
    let selected_model = explicit_model.or(persisted_model.as_deref());
    let default_model = endpoint.map(MediaEndpoint::default_model);

    if let Some(provider_id) = selected_provider {
        let provider_config = agent.resolve_sub_agent_provider_config(&config, provider_id)?;
        return Ok(select_media_provider_config(
            &config,
            provider_id,
            provider_config,
            selected_model,
            default_model,
            endpoint,
        ));
    }

    let provider_config = agent.resolve_provider_config(&config)?;
    Ok(select_media_provider_config(
        &config,
        &config.provider,
        provider_config,
        selected_model,
        default_model,
        endpoint,
    ))
}

fn effective_multimodal_transport(
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
) -> ApiTransport {
    if let Some(fixed_transport) =
        crate::agent::types::fixed_api_transport_for_model(provider_id, &provider_config.model)
    {
        return fixed_transport;
    }
    let mut transport = if crate::agent::types::provider_supports_transport(
        provider_id,
        provider_config.api_transport,
    ) {
        provider_config.api_transport
    } else {
        crate::agent::types::default_api_transport_for_provider(provider_id)
    };
    if transport == ApiTransport::NativeAssistant {
        transport = if crate::agent::types::provider_supports_transport(
            provider_id,
            ApiTransport::Responses,
        ) {
            ApiTransport::Responses
        } else {
            ApiTransport::ChatCompletions
        };
    }
    transport
}

fn build_image_analysis_blocks(
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    prompt: &str,
    source: &MediaInputSource,
) -> Result<(ApiTransport, Vec<serde_json::Value>)> {
    let api_type = crate::agent::types::get_provider_api_type(
        provider_id,
        &provider_config.model,
        &provider_config.base_url,
    );
    let transport = effective_multimodal_transport(provider_id, provider_config);

    if transport == ApiTransport::AnthropicMessages || api_type == ApiType::Anthropic {
        let MediaInputSource::Inline {
            mime_type,
            base64_data,
            ..
        } = source
        else {
            anyhow::bail!(
                "remote image URLs are not supported for Anthropic-compatible image analysis; use 'path', 'base64', or 'data_url'"
            );
        };
        return Ok((
            transport,
            vec![
                serde_json::json!({
                    "type": "text",
                    "text": prompt,
                }),
                serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": mime_type,
                        "data": base64_data,
                    }
                }),
            ],
        ));
    }

    match transport {
        ApiTransport::ChatCompletions => {
            let image_payload = match source {
                MediaInputSource::RemoteUrl { url } => serde_json::json!({ "url": url }),
                MediaInputSource::Inline { data_url, .. } => serde_json::json!({ "url": data_url }),
            };
            Ok((
                transport,
                vec![
                    serde_json::json!({
                        "type": "text",
                        "text": prompt,
                    }),
                    serde_json::json!({
                        "type": "image_url",
                        "image_url": image_payload,
                    }),
                ],
            ))
        }
        _ => {
            let image_url = match source {
                MediaInputSource::RemoteUrl { url } => url.clone(),
                MediaInputSource::Inline { data_url, .. } => data_url.clone(),
            };
            Ok((
                transport,
                vec![
                    serde_json::json!({
                        "type": "input_text",
                        "text": prompt,
                    }),
                    serde_json::json!({
                        "type": "input_image",
                        "image_url": image_url,
                    }),
                ],
            ))
        }
    }
}

fn provider_auth_method(provider_id: &str) -> crate::agent::types::AuthMethod {
    if is_minimax_provider(provider_id) {
        return crate::agent::types::AuthMethod::Bearer;
    }
    crate::agent::types::get_provider_definition(provider_id)
        .map(|definition| definition.auth_method)
        .unwrap_or(crate::agent::types::AuthMethod::Bearer)
}

fn apply_media_auth_headers(
    request: reqwest::RequestBuilder,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
) -> Result<reqwest::RequestBuilder> {
    if provider_config.auth_source != crate::agent::types::AuthSource::ApiKey {
        anyhow::bail!(
            "provider '{}' currently requires api_key authentication for direct image/audio media endpoints",
            provider_id
        );
    }
    if provider_config.api_key.trim().is_empty() {
        anyhow::bail!("provider '{}' is missing an API key", provider_id);
    }
    let request = provider_auth_method(provider_id).apply(request, &provider_config.api_key);
    if provider_id == zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        return Ok(request
            .header("HTTP-Referer", OPENROUTER_ATTRIBUTION_URL)
            .header("X-OpenRouter-Title", OPENROUTER_ATTRIBUTION_TITLE)
            .header("X-OpenRouter-Categories", OPENROUTER_ATTRIBUTION_CATEGORIES));
    }
    Ok(request)
}

fn ensure_openai_like_media_endpoint(
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
) -> Result<()> {
    let api_type = crate::agent::types::get_provider_api_type(
        provider_id,
        &provider_config.model,
        &provider_config.base_url,
    );
    if api_type != ApiType::OpenAI {
        anyhow::bail!(
            "provider '{}' does not expose an OpenAI-compatible media endpoint in the current configuration",
            provider_id
        );
    }
    Ok(())
}

fn audio_tool_route(
    provider_id: &str,
    audio_tool_kind: zorai_shared::providers::AudioToolKind,
) -> AudioToolRoute {
    if provider_id == zorai_shared::providers::PROVIDER_ID_XAI {
        return match audio_tool_kind {
            zorai_shared::providers::AudioToolKind::SpeechToText => AudioToolRoute::XaiStt,
            zorai_shared::providers::AudioToolKind::TextToSpeech => AudioToolRoute::XaiTts,
        };
    }

    if provider_id == zorai_shared::providers::PROVIDER_ID_ELEVENLABS {
        return AudioToolRoute::ElevenLabs;
    }

    if provider_id == zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        return match audio_tool_kind {
            zorai_shared::providers::AudioToolKind::SpeechToText => {
                AudioToolRoute::OpenRouterJsonStt
            }
            zorai_shared::providers::AudioToolKind::TextToSpeech => AudioToolRoute::OpenRouterTts,
        };
    }

    if is_minimax_provider(provider_id) {
        return match audio_tool_kind {
            zorai_shared::providers::AudioToolKind::SpeechToText => {
                AudioToolRoute::OpenAiCompatibleDirect
            }
            zorai_shared::providers::AudioToolKind::TextToSpeech => AudioToolRoute::MiniMaxTts,
        };
    }

    if matches!(
        provider_id,
        zorai_shared::providers::PROVIDER_ID_XIAOMI_MIMO
            | zorai_shared::providers::PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN
    ) {
        return match audio_tool_kind {
            zorai_shared::providers::AudioToolKind::SpeechToText => {
                AudioToolRoute::OpenAiCompatibleDirect
            }
            zorai_shared::providers::AudioToolKind::TextToSpeech => {
                AudioToolRoute::XiaomiChatCompletionsTts
            }
        };
    }

    AudioToolRoute::OpenAiCompatibleDirect
}

fn resolve_audio_tool_route(
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    audio_tool_kind: zorai_shared::providers::AudioToolKind,
) -> Result<AudioToolRoute> {
    if !zorai_shared::providers::provider_supports_audio_tool(provider_id, audio_tool_kind) {
        anyhow::bail!(
            "provider '{}' is not supported for {}",
            provider_id,
            match audio_tool_kind {
                zorai_shared::providers::AudioToolKind::SpeechToText => tool_names::SPEECH_TO_TEXT,
                zorai_shared::providers::AudioToolKind::TextToSpeech => tool_names::TEXT_TO_SPEECH,
            }
        );
    }

    let route = audio_tool_route(provider_id, audio_tool_kind);
    if route == AudioToolRoute::OpenAiCompatibleDirect {
        ensure_openai_like_media_endpoint(provider_id, provider_config)?;
    }
    Ok(route)
}

fn stt_endpoint_for_route(base_url: &str, route: AudioToolRoute) -> String {
    match route {
        AudioToolRoute::XaiStt => openai_like_endpoint(base_url, "stt"),
        _ => openai_like_endpoint(base_url, "audio/transcriptions"),
    }
}

fn tts_endpoint_for_route(base_url: &str, route: AudioToolRoute) -> String {
    match route {
        AudioToolRoute::XaiTts => openai_like_endpoint(base_url, "tts"),
        _ => openai_like_endpoint(base_url, "audio/speech"),
    }
}

fn build_minimax_tts_body(
    model: &str,
    input: &str,
    voice: Option<&str>,
    output_format: &str,
) -> serde_json::Value {
    let voice_id = voice
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_MINIMAX_TTS_VOICE);
    let codec = minimax_tts_codec(output_format);
    serde_json::json!({
        "model": model.trim(),
        "text": input,
        "stream": false,
        "output_format": "hex",
        "voice_setting": {
            "voice_id": voice_id,
            "speed": 1,
            "vol": 1,
            "pitch": 0,
        },
        "audio_setting": {
            "sample_rate": 32_000,
            "bitrate": 128_000,
            "format": codec,
            "channel": 1,
        },
    })
}

fn minimax_base_resp_error(payload: &serde_json::Value) -> Option<String> {
    let base_resp = payload.get("base_resp")?;
    let status_code = base_resp
        .get("status_code")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);
    if status_code == 0 {
        return None;
    }
    let status_msg = base_resp
        .get("status_msg")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown error");
    Some(format!("MiniMax error {status_code}: {status_msg}"))
}

fn extract_minimax_tts_audio_bytes(payload: &serde_json::Value) -> Result<Vec<u8>> {
    if let Some(error) = minimax_base_resp_error(payload) {
        anyhow::bail!("{error}");
    }
    let audio_data = payload
        .pointer("/data/audio")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("MiniMax TTS response did not include data.audio"))?;
    decode_hex_string(audio_data)
}

fn build_minimax_image_generation_body(
    args: &serde_json::Value,
    model: &str,
    prompt: &str,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model.trim(),
        "prompt": prompt,
        "response_format": "base64",
    });

    if let Some(size) = args
        .get("size")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(aspect_ratio) = minimax_image_aspect_ratio(size) {
            body["aspect_ratio"] = serde_json::Value::String(aspect_ratio.to_string());
        }
        if let Some((width, height)) = size.split_once('x') {
            if let (Ok(width), Ok(height)) = (width.parse::<u32>(), height.parse::<u32>()) {
                body["width"] = serde_json::Value::Number(width.into());
                body["height"] = serde_json::Value::Number(height.into());
            }
        }
    }

    body
}

fn extract_minimax_generated_image_base64(payload: &serde_json::Value) -> Result<String> {
    payload
        .pointer("/data/image_base64/0")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("MiniMax image generation returned no base64 image"))
}

async fn execute_minimax_text_to_speech(
    args: &serde_json::Value,
    media_http_client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    input: &str,
) -> Result<String> {
    let requested_format = args
        .get("response_format")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TTS_OUTPUT_FORMAT);
    let voice = args
        .get("voice")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let codec = minimax_tts_codec(requested_format);
    let body = build_minimax_tts_body(&provider_config.model, input, voice, codec);
    let url = format!(
        "{}/t2a_v2",
        minimax_media_base_url(&provider_config.base_url)
    );
    let request =
        apply_media_auth_headers(media_http_client.post(&url), provider_id, provider_config)?;
    let response = request.json(&body).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("text_to_speech failed: HTTP {status} {text}");
    }

    let payload: serde_json::Value = response.json().await?;
    let bytes = extract_minimax_tts_audio_bytes(&payload)?;
    let mime_type = output_mime_type_for_codec(codec);
    let extension = file_extension_for_generated_mime(mime_type, codec);
    let output_path = temp_output_path("speech", &extension);
    tokio::fs::write(&output_path, &bytes).await?;

    serde_json::to_string_pretty(&serde_json::json!({
        "provider": provider_id,
        "model": provider_config.model,
        "voice": voice.unwrap_or(DEFAULT_MINIMAX_TTS_VOICE),
        "path": output_path,
        "mime_type": mime_type,
        "bytes": bytes.len(),
    }))
    .map_err(Into::into)
}

fn xiaomi_tts_model_requires_voice(model: &str) -> bool {
    matches!(
        model.trim(),
        "mimo-v2.5-tts-voiceclone" | "mimo-v2.5-tts-voicedesign"
    )
}

fn xiaomi_tts_output_format(output_format: &str) -> &str {
    match output_format.trim().to_ascii_lowercase().as_str() {
        "pcm" => "pcm16",
        other if !other.is_empty() => output_format.trim(),
        _ => DEFAULT_TTS_OUTPUT_FORMAT,
    }
}

fn build_xiaomi_tts_body(
    provider_config: &crate::agent::types::ProviderConfig,
    input: &str,
    voice: Option<&str>,
    output_format: &str,
) -> Result<serde_json::Value> {
    let model = provider_config.model.trim();
    let trimmed_voice = voice.map(str::trim).filter(|value| !value.is_empty());
    let messages = match model {
        "mimo-v2.5-tts-voicedesign" => vec![
            serde_json::json!({
                "role": "user",
                "content": trimmed_voice.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Xiaomi MiMo voice design requires a non-empty 'voice' style prompt"
                    )
                })?,
            }),
            serde_json::json!({
                "role": "assistant",
                "content": input,
            }),
        ],
        "mimo-v2.5-tts-voiceclone" => vec![serde_json::json!({
            "role": "assistant",
            "content": input,
        })],
        _ => vec![serde_json::json!({
            "role": "assistant",
            "content": input,
        })],
    };

    let mut audio = serde_json::json!({
        "format": xiaomi_tts_output_format(output_format),
    });
    if let Some(voice) = trimmed_voice {
        audio["voice"] = serde_json::Value::String(voice.to_string());
    } else if xiaomi_tts_model_requires_voice(model) {
        anyhow::bail!(
            "Xiaomi MiMo model '{}' requires a non-empty 'voice' argument",
            model
        );
    }

    Ok(serde_json::json!({
        "model": model,
        "messages": messages,
        "audio": audio,
    }))
}

fn minimax_media_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        return trimmed.to_string();
    }
    if let Some(stripped) = trimmed.strip_suffix("/anthropic") {
        return format!("{stripped}/v1");
    }
    format!("{trimmed}/v1")
}

fn minimax_tts_codec(output_format: &str) -> &'static str {
    match output_format.trim().to_ascii_lowercase().as_str() {
        "wav" => "wav",
        "flac" => "flac",
        _ => "mp3",
    }
}

fn minimax_image_aspect_ratio(size: &str) -> Option<&'static str> {
    match size.trim() {
        "1024x1024" | "512x512" | "2048x2048" => Some("1:1"),
        "1344x768" => Some("16:9"),
        "1152x864" | "1184x864" => Some("4:3"),
        "1248x832" => Some("3:2"),
        "832x1248" => Some("2:3"),
        "864x1152" | "864x1184" => Some("3:4"),
        "768x1344" => Some("9:16"),
        "1536x672" => Some("21:9"),
        _ => None,
    }
}

fn decode_hex_string(payload: &str) -> Result<Vec<u8>> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        anyhow::bail!("hex payload was empty");
    }
    if trimmed.len() % 2 != 0 {
        anyhow::bail!("hex payload had an odd number of characters");
    }

    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    let mut chars = trimmed.as_bytes().chunks_exact(2);
    for pair in &mut chars {
        let hex = std::str::from_utf8(pair)
            .map_err(|error| anyhow::anyhow!("hex payload was not valid utf-8: {error}"))?;
        let byte = u8::from_str_radix(hex, 16)
            .map_err(|error| anyhow::anyhow!("invalid hex audio payload: {error}"))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

fn extract_xiaomi_tts_audio_bytes(payload: &serde_json::Value) -> Result<Vec<u8>> {
    let audio_data = payload
        .pointer("/choices/0/message/audio/data")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Xiaomi MiMo TTS response did not include audio data"))?;
    base64::engine::general_purpose::STANDARD
        .decode(audio_data)
        .map_err(|error| anyhow::anyhow!("Xiaomi MiMo TTS returned invalid base64 audio: {error}"))
}

async fn execute_xiaomi_text_to_speech(
    args: &serde_json::Value,
    media_http_client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    input: &str,
) -> Result<String> {
    let voice = args.get("voice").and_then(|value| value.as_str());
    let output_format = args
        .get("response_format")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TTS_OUTPUT_FORMAT);
    let body = build_xiaomi_tts_body(provider_config, input, voice, output_format)?;
    let url = openai_like_endpoint(&provider_config.base_url, "chat/completions");
    let request =
        apply_media_auth_headers(media_http_client.post(&url), provider_id, provider_config)?;
    let response = request.json(&body).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("text_to_speech failed: HTTP {status} {text}");
    }

    let payload: serde_json::Value = response.json().await?;
    let bytes = extract_xiaomi_tts_audio_bytes(&payload)?;
    let mime_type = output_mime_type_for_codec(xiaomi_tts_output_format(output_format));
    let extension = file_extension_for_generated_mime(mime_type, output_format);
    let output_path = temp_output_path("speech", &extension);
    tokio::fs::write(&output_path, &bytes).await?;

    serde_json::to_string_pretty(&serde_json::json!({
        "provider": provider_id,
        "model": provider_config.model,
        "voice": voice.unwrap_or_default(),
        "path": output_path,
        "mime_type": mime_type,
        "bytes": bytes.len(),
    }))
    .map_err(Into::into)
}

fn build_openrouter_stt_body(
    args: &serde_json::Value,
    model: &str,
    base64_data: &str,
    audio_format: &str,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model.trim(),
        "input_audio": {
            "data": base64_data,
            "format": audio_format,
        },
    });
    if let Some(language) = args
        .get("language")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["language"] = serde_json::Value::String(language.to_string());
    }
    if let Some(temperature) = args.get("temperature").and_then(|value| value.as_f64()) {
        body["temperature"] = serde_json::json!(temperature);
    }
    body
}

async fn execute_openrouter_speech_to_text(
    args: &serde_json::Value,
    media_http_client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    mime_type: &str,
    bytes: &[u8],
) -> Result<String> {
    let audio_format = audio_format_from_mime_type(mime_type)
        .or_else(|| {
            args.get("format")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| match value {
                    "wav" | "mp3" | "flac" | "m4a" | "ogg" | "webm" | "aac" => value,
                    _ => "wav",
                })
        })
        .unwrap_or("wav");
    let base64_data = base64::engine::general_purpose::STANDARD.encode(bytes);
    let body = build_openrouter_stt_body(args, &provider_config.model, &base64_data, audio_format);
    let url = openai_like_endpoint(&provider_config.base_url, "audio/transcriptions");
    let request =
        apply_media_auth_headers(media_http_client.post(&url), provider_id, provider_config)?;
    let response = request.json(&body).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("speech_to_text failed: HTTP {status} {text}");
    }

    let response_format = args
        .get("response_format")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("json");
    let body = response.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| anyhow::anyhow!("speech_to_text returned invalid JSON: {error}"))?;
    format_speech_to_text_response(AudioToolRoute::OpenRouterJsonStt, response_format, &parsed)
}

const OPENROUTER_TTS_MAX_CHARS: usize = 3_000;
const OPENROUTER_TTS_DEFAULT_FORMAT: &str = "wav";
const OPENROUTER_TTS_PCM_SAMPLE_RATE: u32 = 24_000;

fn chunk_tts_input(text: &str, max_chars: usize) -> Vec<String> {
    let text = text.trim();
    if max_chars == 0 || text.chars().count() <= max_chars {
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![text.to_string()]
        };
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    let flush = |current: &mut String, current_len: &mut usize, chunks: &mut Vec<String>| {
        let trimmed = current.trim();
        if !trimmed.is_empty() {
            chunks.push(trimmed.to_string());
        }
        current.clear();
        *current_len = 0;
    };

    for unit in text.split_inclusive(char::is_whitespace) {
        let unit_len = unit.chars().count();
        if unit_len > max_chars {
            flush(&mut current, &mut current_len, &mut chunks);
            let mut piece = String::new();
            let mut piece_len = 0usize;
            for ch in unit.chars() {
                if piece_len == max_chars {
                    let trimmed = piece.trim();
                    if !trimmed.is_empty() {
                        chunks.push(trimmed.to_string());
                    }
                    piece.clear();
                    piece_len = 0;
                }
                piece.push(ch);
                piece_len += 1;
            }
            let trimmed = piece.trim();
            if !trimmed.is_empty() {
                chunks.push(trimmed.to_string());
            }
            continue;
        }

        if current_len + unit_len > max_chars {
            flush(&mut current, &mut current_len, &mut chunks);
        }
        current.push_str(unit);
        current_len += unit_len;
    }
    flush(&mut current, &mut current_len, &mut chunks);
    chunks
}

fn wrap_pcm_as_wav(pcm: &[u8], sample_rate: u32, channels: u16, bits_per_sample: u16) -> Vec<u8> {
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample / 8);
    let block_align = channels * (bits_per_sample / 8);
    let data_len = pcm.len() as u32;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

fn parse_wav(bytes: &[u8]) -> Result<(u16, u32, u16, Vec<u8>)> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        anyhow::bail!("OpenRouter TTS returned audio that is not a RIFF/WAVE container");
    }
    let mut pos = 12usize;
    let mut fmt: Option<(u16, u32, u16)> = None;
    let mut data: Option<Vec<u8>> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body_start = pos + 8;
        let body_end = body_start.saturating_add(size).min(bytes.len());
        if id == b"fmt " && body_end - body_start >= 16 {
            let channels = u16::from_le_bytes([bytes[body_start + 2], bytes[body_start + 3]]);
            let sample_rate = u32::from_le_bytes([
                bytes[body_start + 4],
                bytes[body_start + 5],
                bytes[body_start + 6],
                bytes[body_start + 7],
            ]);
            let bits = u16::from_le_bytes([bytes[body_start + 14], bytes[body_start + 15]]);
            fmt = Some((channels, sample_rate, bits));
        } else if id == b"data" {
            data = Some(bytes[body_start..body_end].to_vec());
        }
        pos = body_start + size + (size & 1);
    }
    let (channels, sample_rate, bits) =
        fmt.ok_or_else(|| anyhow::anyhow!("OpenRouter TTS WAV is missing a fmt chunk"))?;
    let data = data.ok_or_else(|| anyhow::anyhow!("OpenRouter TTS WAV is missing a data chunk"))?;
    Ok((channels, sample_rate, bits, data))
}

fn merge_wav_chunks(chunks: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut iter = chunks.iter();
    let first = iter
        .next()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter TTS produced no audio chunks"))?;
    let (channels, sample_rate, bits, mut payload) = parse_wav(first)?;
    for chunk in iter {
        let (_, _, _, data) = parse_wav(chunk)?;
        payload.extend_from_slice(&data);
    }
    Ok(wrap_pcm_as_wav(&payload, sample_rate, channels, bits))
}

fn combine_openrouter_tts_chunks(
    format: &str,
    chunks: &[Vec<u8>],
) -> Result<(Vec<u8>, &'static str, String)> {
    match format {
        "wav" => Ok((merge_wav_chunks(chunks)?, "audio/wav", "wav".to_string())),
        "pcm" | "pcm16" => {
            let mut raw = Vec::new();
            for chunk in chunks {
                raw.extend_from_slice(chunk);
            }
            let wav = wrap_pcm_as_wav(&raw, OPENROUTER_TTS_PCM_SAMPLE_RATE, 1, 16);
            Ok((wav, "audio/wav", "wav".to_string()))
        }
        other => {
            let mut raw = Vec::new();
            for chunk in chunks {
                raw.extend_from_slice(chunk);
            }
            let mime = output_mime_type_for_codec(other);
            let ext = file_extension_for_generated_mime(mime, other);
            Ok((raw, mime, ext))
        }
    }
}

async fn execute_openrouter_text_to_speech(
    args: &serde_json::Value,
    media_http_client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    input: &str,
) -> Result<String> {
    let voice = args
        .get("voice")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TTS_VOICE);
    let requested_format = args
        .get("response_format")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| OPENROUTER_TTS_DEFAULT_FORMAT.to_string());

    let chunks = chunk_tts_input(input, OPENROUTER_TTS_MAX_CHARS);
    if chunks.is_empty() {
        anyhow::bail!("missing 'input' argument");
    }
    let url = openai_like_endpoint(&provider_config.base_url, "audio/speech");

    let mut active_format = requested_format.clone();
    let mut collected: Vec<Vec<u8>> = Vec::new();
    loop {
        collected.clear();
        let mut fall_back_to_pcm = false;
        for (index, chunk) in chunks.iter().enumerate() {
            let body = serde_json::json!({
                "model": provider_config.model,
                "input": chunk,
                "voice": voice,
                "response_format": active_format,
            });
            let request = apply_media_auth_headers(
                media_http_client.post(&url),
                provider_id,
                provider_config,
            )?;
            let response = request.json(&body).send().await?;
            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if index == 0 && active_format != "pcm" {
                    active_format = "pcm".to_string();
                    fall_back_to_pcm = true;
                    break;
                }
                anyhow::bail!(
                    "text_to_speech failed: HTTP {status} {text} (provider='{}', model='{}', voice='{}', response_format='{}', chunk={}/{}). The voice or format is likely not valid for this model.",
                    provider_id,
                    provider_config.model,
                    voice,
                    active_format,
                    index + 1,
                    chunks.len()
                );
            }
            collected.push(response.bytes().await?.to_vec());
        }
        if !fall_back_to_pcm {
            break;
        }
    }

    let (bytes, mime_type, extension) = combine_openrouter_tts_chunks(&active_format, &collected)?;
    let output_path = temp_output_path("speech", &extension);
    tokio::fs::write(&output_path, &bytes).await?;

    serde_json::to_string_pretty(&serde_json::json!({
        "provider": provider_id,
        "model": provider_config.model,
        "voice": voice,
        "path": output_path,
        "mime_type": mime_type,
        "bytes": bytes.len(),
        "chunks": chunks.len(),
        "response_format": active_format,
    }))
    .map_err(Into::into)
}

fn push_nonempty_text_field(
    fields: &mut Vec<OrderedMultipartField>,
    name: &str,
    value: Option<&str>,
) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        fields.push(OrderedMultipartField::Text {
            name: name.to_string(),
            value: value.to_string(),
        });
    }
}

fn build_xai_stt_multipart_fields(
    args: &serde_json::Value,
    filename: &str,
    mime_type: &str,
) -> Vec<OrderedMultipartField> {
    let mut fields = Vec::new();
    push_nonempty_text_field(
        &mut fields,
        "language",
        args.get("language").and_then(|value| value.as_str()),
    );
    push_nonempty_text_field(
        &mut fields,
        "format",
        args.get("format")
            .map(|value| match value {
                serde_json::Value::Bool(boolean) => boolean.to_string(),
                serde_json::Value::String(string) => string.trim().to_string(),
                _ => String::new(),
            })
            .as_deref(),
    );
    push_nonempty_text_field(
        &mut fields,
        "audio_format",
        args.get("audio_format").and_then(|value| value.as_str()),
    );
    push_nonempty_text_field(
        &mut fields,
        "sample_rate",
        args.get("sample_rate")
            .map(|value| value.to_string())
            .as_deref(),
    );
    push_nonempty_text_field(
        &mut fields,
        "multichannel",
        args.get("multichannel")
            .map(|value| value.to_string())
            .as_deref(),
    );
    push_nonempty_text_field(
        &mut fields,
        "channels",
        args.get("channels")
            .map(|value| value.to_string())
            .as_deref(),
    );
    push_nonempty_text_field(
        &mut fields,
        "diarize",
        args.get("diarize")
            .map(|value| value.to_string())
            .as_deref(),
    );
    fields.push(OrderedMultipartField::File {
        name: "file".to_string(),
        filename: filename.to_string(),
        mime_type: mime_type.to_string(),
    });
    fields
}

fn ordered_fields_to_multipart_form(
    fields: &[OrderedMultipartField],
    bytes: &[u8],
) -> Result<reqwest::multipart::Form> {
    let mut form = reqwest::multipart::Form::new();
    for field in fields {
        match field {
            OrderedMultipartField::Text { name, value } => {
                form = form.text(name.clone(), value.clone());
            }
            OrderedMultipartField::File {
                name,
                filename,
                mime_type,
            } => {
                let part = reqwest::multipart::Part::bytes(bytes.to_vec())
                    .file_name(filename.clone())
                    .mime_str(mime_type)?;
                form = form.part(name.clone(), part);
            }
        }
    }
    Ok(form)
}

fn xai_tts_voice(args: &serde_json::Value) -> &str {
    args.get("voice")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_XAI_TTS_VOICE)
}

fn elevenlabs_api_endpoint(base_url: &str, endpoint: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let endpoint = endpoint.trim_start_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/{endpoint}")
    } else {
        format!("{base}/v1/{endpoint}")
    }
}

fn elevenlabs_stt_endpoint(base_url: &str) -> String {
    elevenlabs_api_endpoint(base_url, "speech-to-text")
}

fn elevenlabs_tts_endpoint(base_url: &str, voice_id: &str) -> Result<String> {
    let voice_id = voice_id.trim();
    if voice_id.is_empty() {
        anyhow::bail!(
            "ElevenLabs text_to_speech requires audio_tts_voice to be an ElevenLabs voice ID"
        );
    }
    let encoded_voice_id =
        percent_encoding::utf8_percent_encode(voice_id, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
    Ok(elevenlabs_api_endpoint(
        base_url,
        &format!("text-to-speech/{encoded_voice_id}"),
    ))
}

fn build_elevenlabs_stt_multipart_fields(
    args: &serde_json::Value,
    model: &str,
    filename: &str,
    mime_type: &str,
) -> Vec<OrderedMultipartField> {
    let mut fields = vec![OrderedMultipartField::Text {
        name: "model_id".to_string(),
        value: model.trim().to_string(),
    }];
    push_nonempty_text_field(
        &mut fields,
        "language_code",
        args.get("language").and_then(|value| value.as_str()),
    );
    fields.push(OrderedMultipartField::File {
        name: "file".to_string(),
        filename: filename.to_string(),
        mime_type: mime_type.to_string(),
    });
    fields
}

fn build_elevenlabs_tts_body(input: &str, model: &str) -> serde_json::Value {
    serde_json::json!({
        "text": input,
        "model_id": model.trim(),
    })
}

fn build_xai_tts_body(args: &serde_json::Value, input: &str) -> Result<serde_json::Value> {
    let language = args
        .get("language")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("xAI text_to_speech requires a non-empty 'language' argument")
        })?;
    let mut body = serde_json::json!({
        "text": input,
        "voice_id": xai_tts_voice(args),
        "language": language,
    });

    let mut output_format = serde_json::Map::new();
    let codec = args
        .get("response_format")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TTS_OUTPUT_FORMAT);
    output_format.insert(
        "codec".to_string(),
        serde_json::Value::String(codec.to_string()),
    );
    if let Some(sample_rate) = args.get("sample_rate") {
        output_format.insert("sample_rate".to_string(), sample_rate.clone());
    }
    if let Some(bit_rate) = args.get("bit_rate") {
        output_format.insert("bit_rate".to_string(), bit_rate.clone());
    }
    if !output_format.is_empty() {
        body["output_format"] = serde_json::Value::Object(output_format);
    }
    Ok(body)
}

fn format_speech_to_text_response(
    route: AudioToolRoute,
    response_format: &str,
    parsed: &serde_json::Value,
) -> Result<String> {
    if response_format == "text" {
        let transcript = parsed
            .get("text")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("speech_to_text returned no transcription text"))?;
        return Ok(transcript.to_string());
    }

    match route {
        AudioToolRoute::XaiStt => Ok(serde_json::to_string_pretty(parsed)?),
        _ => {
            if let Some(text) = parsed.get("text").and_then(|value| value.as_str()) {
                Ok(text.to_string())
            } else {
                Ok(serde_json::to_string_pretty(parsed)?)
            }
        }
    }
}

fn output_mime_type_for_codec(codec: &str) -> &'static str {
    match codec {
        "pcm" => "audio/L16",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" | "m4a" => "audio/mp4",
        "mulaw" => "audio/basic",
        "alaw" => "audio/alaw",
        _ => "audio/mpeg",
    }
}

fn build_audio_transcription_prompt(language: Option<&str>, prompt: Option<&str>) -> String {
    let mut instruction =
        "Transcribe the provided audio. Return only the transcription text.".to_string();
    if let Some(language) = language.map(str::trim).filter(|value| !value.is_empty()) {
        instruction.push_str(" The spoken language is ");
        instruction.push_str(language);
        instruction.push('.');
    }
    if let Some(prompt) = prompt.map(str::trim).filter(|value| !value.is_empty()) {
        instruction.push_str(" Prefer these terms or context hints: ");
        instruction.push_str(prompt);
        instruction.push('.');
    }
    instruction
}

fn build_audio_transcription_blocks(
    transport: ApiTransport,
    prompt: &str,
    base64_data: &str,
    format: &str,
) -> Vec<serde_json::Value> {
    let text_type = if transport == ApiTransport::ChatCompletions {
        "text"
    } else {
        "input_text"
    };

    vec![
        serde_json::json!({
            "type": text_type,
            "text": prompt,
        }),
        serde_json::json!({
            "type": "input_audio",
            "input_audio": {
                "data": base64_data,
                "format": format,
            }
        }),
    ]
}

async fn execute_multimodal_speech_to_text(
    args: &serde_json::Value,
    http_client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    mime_type: &str,
    bytes: &[u8],
) -> Result<String> {
    let transport = effective_multimodal_transport(provider_id, provider_config);
    let audio_format = audio_format_from_mime_type(mime_type).unwrap_or("wav");
    let prompt = build_audio_transcription_prompt(
        args.get("language").and_then(|value| value.as_str()),
        args.get("prompt").and_then(|value| value.as_str()),
    );
    let base64_data = base64::engine::general_purpose::STANDARD.encode(bytes);
    let blocks = build_audio_transcription_blocks(transport, &prompt, &base64_data, audio_format);
    let messages = vec![ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Blocks(blocks),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    }];

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
    let transcript = content.trim();
    if transcript.is_empty() {
        anyhow::bail!("speech_to_text returned no transcription text");
    }

    let response_format = args
        .get("response_format")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("json");
    if response_format == "text" {
        Ok(transcript.to_string())
    } else {
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "text": transcript,
        }))?)
    }
}

async fn collect_completion_output(
    mut stream: crate::agent::llm_client::CompletionStream,
) -> Result<(
    String,
    Option<String>,
    Option<CompletionProviderFinalResult>,
)> {
    let mut accumulated_content = String::new();
    let mut accumulated_reasoning = String::new();
    let mut provider_final_result = None;

    while let Some(chunk) = stream.next().await {
        match chunk? {
            CompletionChunk::Delta { content, reasoning } => {
                if !content.is_empty() {
                    accumulated_content.push_str(&content);
                }
                if let Some(reasoning) = reasoning.filter(|value| !value.is_empty()) {
                    if !accumulated_reasoning.is_empty() {
                        accumulated_reasoning.push('\n');
                    }
                    accumulated_reasoning.push_str(&reasoning);
                }
            }
            CompletionChunk::Done {
                content,
                reasoning,
                provider_final_result: final_result,
                ..
            } => {
                if !content.trim().is_empty() {
                    accumulated_content = content;
                }
                if let Some(reasoning) = reasoning.filter(|value| !value.trim().is_empty()) {
                    accumulated_reasoning = reasoning;
                }
                provider_final_result = final_result;
                break;
            }
            CompletionChunk::ToolCalls {
                tool_calls,
                content,
                reasoning,
                provider_final_result: final_result,
                ..
            } => {
                if !tool_calls.is_empty() {
                    anyhow::bail!(
                        "media analysis received unexpected tool calls from the provider"
                    );
                }
                if let Some(content) = content.filter(|value| !value.trim().is_empty()) {
                    accumulated_content = content;
                }
                if let Some(reasoning) = reasoning.filter(|value| !value.trim().is_empty()) {
                    accumulated_reasoning = reasoning;
                }
                provider_final_result = final_result;
                break;
            }
            CompletionChunk::TransportFallback { .. } | CompletionChunk::Retry { .. } => {}
            CompletionChunk::Error { message } => return Err(anyhow::anyhow!(message)),
        }
    }

    Ok((
        accumulated_content,
        (!accumulated_reasoning.trim().is_empty()).then_some(accumulated_reasoning),
        provider_final_result,
    ))
}

fn image_support_error(provider_id: &str, model: &str) -> anyhow::Error {
    let alternatives =
        crate::agent::types::models_supporting(provider_id, crate::agent::types::Modality::Image);
    if alternatives.is_empty() {
        anyhow::anyhow!(
            "model '{}' for provider '{}' is not known to support image inputs",
            model,
            provider_id
        )
    } else {
        anyhow::anyhow!(
            "model '{}' for provider '{}' is not known to support image inputs. Try one of: {}",
            model,
            provider_id,
            alternatives.join(", ")
        )
    }
}

fn temp_output_path(prefix: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "zorai-{}-{}.{}",
        prefix,
        uuid::Uuid::new_v4(),
        extension.trim_start_matches('.')
    ))
}

fn thread_media_output_path(
    agent_data_dir: &Path,
    thread_id: &str,
    prefix: &str,
    extension: &str,
) -> PathBuf {
    let data_root = agent_data_dir.parent().unwrap_or(agent_data_dir);
    zorai_protocol::thread_media_dir(data_root, thread_id).join(format!(
        "{prefix}-{}.{}",
        now_millis(),
        extension.trim_start_matches('.')
    ))
}

fn file_url_for_path(path: &Path) -> String {
    format!("file://{}", path.display())
}

async fn move_generated_media_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    match tokio::fs::rename(source, destination).await {
        Ok(_) => Ok(()),
        Err(_) => {
            tokio::fs::copy(source, destination).await?;
            tokio::fs::remove_file(source).await.ok();
            Ok(())
        }
    }
}

async fn persist_generated_image_for_thread(
    agent: &AgentEngine,
    thread_id: &str,
    prompt: &str,
    result_json: &str,
    include_prompt_message: bool,
) -> Result<String> {
    let payload: serde_json::Value = serde_json::from_str(result_json)?;
    let provider = payload
        .get("provider")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown")
        .to_string();
    let model = payload
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown")
        .to_string();
    let revised_prompt = payload
        .get("revised_prompt")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let mime_type = payload
        .get("mime_type")
        .and_then(|value| value.as_str())
        .unwrap_or("image/png")
        .to_string();

    let persisted_path = if let Some(path) = payload
        .get("path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let source = PathBuf::from(path);
        let extension = source
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_string)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                file_extension_for_generated_mime(&mime_type, DEFAULT_IMAGE_OUTPUT_FORMAT)
            });
        let destination = thread_media_output_path(&agent.data_dir, thread_id, "image", &extension);
        move_generated_media_file(&source, &destination).await?;
        Some(destination)
    } else {
        None
    };

    let image_url = persisted_path
        .as_deref()
        .map(file_url_for_path)
        .or_else(|| {
            payload
                .get("url")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });

    let assistant_content = revised_prompt
        .as_deref()
        .filter(|value| value.trim() != prompt.trim())
        .map(|value| format!("Generated image.\nRevised prompt: {value}"))
        .unwrap_or_else(|| "Generated image.".to_string());

    {
        let mut threads = agent.threads.write().await;
        let Some(thread) = threads.get_mut(thread_id) else {
            anyhow::bail!("thread not found while persisting generated image");
        };

        if include_prompt_message {
            thread.messages.push(AgentMessage::user(
                format!("🖼 {}", prompt.trim()),
                now_millis(),
            ));
        }
        thread.messages.push(AgentMessage {
            id: super::super::types::generate_message_id(),
            role: MessageRole::Assistant,
            content: assistant_content,
            content_blocks: image_url
                .clone()
                .map(|url| {
                    vec![AgentContentBlock::Image {
                        url: Some(url),
                        data_url: None,
                        mime_type: Some(mime_type.clone()),
                        inventory_path: None,
                    }]
                })
                .unwrap_or_default(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider: Some(provider.clone()),
            model: Some(model.clone()),
            api_transport: None,
            response_id: None,
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: Some(current_agent_scope_id()),
            author_agent_name: Some("assistant".to_string()),
            reasoning: None,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: now_millis(),
            feedback: None,
        });
        thread.updated_at = now_millis();
    }
    agent.persist_thread_by_id(thread_id).await;

    if let Some(path) = persisted_path.as_deref() {
        agent
            .record_file_work_context(
                thread_id,
                None,
                tool_names::GENERATE_IMAGE,
                &path.to_string_lossy(),
            )
            .await;
    }

    serde_json::to_string_pretty(&serde_json::json!({
        "ok": true,
        "thread_id": thread_id,
        "provider": provider,
        "model": model,
        "mime_type": mime_type,
        "path": persisted_path.as_ref().map(|path| path.to_string_lossy().to_string()),
        "file_url": persisted_path.as_deref().map(file_url_for_path),
        "url": payload.get("url").and_then(|value| value.as_str()),
        "revised_prompt": revised_prompt,
    }))
    .map_err(Into::into)
}

pub(crate) async fn execute_analyze_image(
    args: &serde_json::Value,
    agent: &AgentEngine,
    _http_client: &reqwest::Client,
) -> Result<String> {
    let media_http_client = fresh_media_http_client(tool_names::ANALYZE_IMAGE, args);
    let (provider_id, provider_config) = resolve_media_provider_config(agent, args, None).await?;
    if !crate::agent::types::model_supports(
        &provider_id,
        &provider_config.model,
        crate::agent::types::Modality::Image,
    ) {
        return Err(image_support_error(&provider_id, &provider_config.model));
    }

    let prompt = args
        .get("prompt")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_IMAGE_ANALYSIS_PROMPT);
    let source = resolve_media_input_source(args, MediaKind::Image).await?;
    let (transport, blocks) =
        build_image_analysis_blocks(&provider_id, &provider_config, prompt, &source)?;
    let messages = vec![ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Blocks(blocks),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    }];

    let stream = crate::agent::llm_client::send_completion_request(
        &media_http_client,
        &provider_id,
        &provider_config,
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
    let (content, reasoning, provider_final_result) = collect_completion_output(stream).await?;
    if content.trim().is_empty() {
        anyhow::bail!("image analysis returned no content");
    }

    let mut response = content;
    if args
        .get("include_reasoning")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        if let Some(reasoning) = reasoning.filter(|value| !value.trim().is_empty()) {
            response.push_str("\n\n[reasoning]\n");
            response.push_str(&reasoning);
        }
    }
    if args
        .get("include_provider_result")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        if let Some(provider_final_result) = provider_final_result {
            response.push_str("\n\n[provider_final_result]\n");
            response.push_str(&serde_json::to_string_pretty(&provider_final_result)?);
        }
    }

    Ok(response)
}

pub(crate) async fn execute_generate_image(
    args: &serde_json::Value,
    agent: &AgentEngine,
    _http_client: &reqwest::Client,
    thread_id: Option<&str>,
) -> Result<String> {
    let media_http_client = fresh_media_http_client(tool_names::GENERATE_IMAGE, args);
    let prompt = args
        .get("prompt")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'prompt' argument"))?;
    let (provider_id, provider_config) =
        resolve_media_provider_config(agent, args, Some(MediaEndpoint::ImageGeneration)).await?;
    let route = image_generation_route(&provider_id);

    if route == ImageGenerationRoute::OpenRouterChatCompletions {
        let url = openai_like_endpoint(&provider_config.base_url, "chat/completions");
        let body = build_openrouter_image_generation_body(args, &provider_config, prompt);
        let request =
            apply_media_auth_headers(media_http_client.post(&url), &provider_id, &provider_config)?;
        let response = request.json(&body).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("image generation failed: HTTP {status} {text}");
        }

        let payload: serde_json::Value = response.json().await?;
        let image_url = extract_openrouter_generated_image(&payload)?;
        if image_url.starts_with("data:") {
            let generated =
                generated_image_response_from_data_url(&provider_id, &provider_config, &image_url)?;
            if let Some(thread_id) = thread_id {
                return persist_generated_image_for_thread(
                    agent, thread_id, prompt, &generated, false,
                )
                .await;
            }
            return Ok(generated);
        }

        let generated = serde_json::to_string_pretty(&serde_json::json!({
            "provider": provider_id,
            "model": provider_config.model,
            "url": image_url,
        }))?;
        if let Some(thread_id) = thread_id {
            return persist_generated_image_for_thread(agent, thread_id, prompt, &generated, false)
                .await;
        }
        return Ok(generated);
    }

    if route == ImageGenerationRoute::MiniMaxDirect {
        let url = format!(
            "{}/image_generation",
            minimax_media_base_url(&provider_config.base_url)
        );
        let body = build_minimax_image_generation_body(args, &provider_config.model, prompt);
        let request =
            apply_media_auth_headers(media_http_client.post(&url), &provider_id, &provider_config)?;
        let response = request.json(&body).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("image generation failed: HTTP {status} {text}");
        }

        let payload: serde_json::Value = response.json().await?;
        let b64 = extract_minimax_generated_image_base64(&payload)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|error| anyhow::anyhow!("invalid image base64 payload: {error}"))?;
        let output_path = temp_output_path("image", "jpg");
        tokio::fs::write(&output_path, &bytes).await?;
        let generated = serde_json::to_string_pretty(&serde_json::json!({
            "provider": provider_id,
            "model": provider_config.model,
            "path": output_path,
            "mime_type": "image/jpeg",
            "bytes": bytes.len(),
        }))?;
        if let Some(thread_id) = thread_id {
            return persist_generated_image_for_thread(agent, thread_id, prompt, &generated, false)
                .await;
        }
        return Ok(generated);
    }

    ensure_openai_like_media_endpoint(&provider_id, &provider_config)?;

    let output_format = args
        .get("output_format")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_IMAGE_OUTPUT_FORMAT);
    let url = openai_like_endpoint(&provider_config.base_url, "images/generations");
    let mut body = serde_json::json!({
        "model": provider_config.model,
        "prompt": prompt,
        "response_format": "b64_json",
    });
    if let Some(size) = args
        .get("size")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["size"] = serde_json::Value::String(size.to_string());
    }
    if let Some(quality) = args
        .get("quality")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["quality"] = serde_json::Value::String(quality.to_string());
    }
    if let Some(background) = args
        .get("background")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["background"] = serde_json::Value::String(background.to_string());
    }
    if let Some(style) = args
        .get("style")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["style"] = serde_json::Value::String(style.to_string());
    }
    body["output_format"] = serde_json::Value::String(output_format.to_string());

    let request =
        apply_media_auth_headers(media_http_client.post(&url), &provider_id, &provider_config)?;
    let response = request.json(&body).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("image generation failed: HTTP {status} {text}");
    }

    let payload: serde_json::Value = response.json().await?;
    let first = payload
        .get("data")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .ok_or_else(|| anyhow::anyhow!("image generation returned no image payload"))?;

    let revised_prompt = first
        .get("revised_prompt")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);

    if let Some(b64_json) = first.get("b64_json").and_then(|value| value.as_str()) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64_json)
            .map_err(|error| anyhow::anyhow!("invalid image base64 payload: {error}"))?;
        let mime_type = match output_format {
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => "image/png",
        };
        let extension = file_extension_for_generated_mime(mime_type, output_format);
        let output_path = temp_output_path("image", &extension);
        tokio::fs::write(&output_path, &bytes).await?;
        let generated = serde_json::to_string_pretty(&serde_json::json!({
            "provider": provider_id,
            "model": provider_config.model,
            "path": output_path,
            "mime_type": mime_type,
            "bytes": bytes.len(),
            "revised_prompt": revised_prompt,
        }))?;
        if let Some(thread_id) = thread_id {
            return persist_generated_image_for_thread(agent, thread_id, prompt, &generated, false)
                .await;
        }
        return Ok(generated);
    }

    if let Some(url) = first.get("url").and_then(|value| value.as_str()) {
        let generated = serde_json::to_string_pretty(&serde_json::json!({
            "provider": provider_id,
            "model": provider_config.model,
            "url": url,
            "revised_prompt": revised_prompt,
        }))?;
        if let Some(thread_id) = thread_id {
            return persist_generated_image_for_thread(agent, thread_id, prompt, &generated, false)
                .await;
        }
        return Ok(generated);
    }

    anyhow::bail!("image generation returned neither 'b64_json' nor 'url'")
}

pub(crate) async fn execute_generate_image_for_ipc(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let prompt = args
        .get("prompt")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'prompt' argument"))?
        .to_string();
    let requested_thread_id = args
        .get("thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let (thread_id, is_new_thread) = agent
        .get_or_create_thread(requested_thread_id, &prompt)
        .await;
    agent.ensure_thread_messages_loaded(&thread_id).await;

    let upstream_result = execute_generate_image(args, agent, &agent.http_client, None).await?;
    let persisted =
        persist_generated_image_for_thread(agent, &thread_id, &prompt, &upstream_result, true)
            .await?;
    agent
        .record_operator_message(&thread_id, &format!("🖼 {}", prompt), is_new_thread)
        .await?;
    Ok(persisted)
}

pub(crate) async fn execute_speech_to_text(
    args: &serde_json::Value,
    agent: &AgentEngine,
    _http_client: &reqwest::Client,
) -> Result<String> {
    let media_http_client = fresh_media_http_client(tool_names::SPEECH_TO_TEXT, args);
    let path = args
        .get("path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_read_path(path)?;
    let resolved = resolve_tool_path(path, None);
    let bytes = tokio::fs::read(&resolved).await?;
    let filename = resolved
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("audio.bin")
        .to_string();
    let mime_type = args
        .get("mime_type")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| infer_media_mime(&resolved, MediaKind::Audio).to_string());
    let (provider_id, provider_config) =
        resolve_media_provider_config(agent, args, Some(MediaEndpoint::SpeechToText)).await?;
    let route = resolve_audio_tool_route(
        &provider_id,
        &provider_config,
        zorai_shared::providers::AudioToolKind::SpeechToText,
    )?;

    if route == AudioToolRoute::ProviderMultimodalCompletion {
        return execute_multimodal_speech_to_text(
            args,
            &media_http_client,
            &provider_id,
            &provider_config,
            &mime_type,
            &bytes,
        )
        .await;
    }

    if route == AudioToolRoute::OpenRouterJsonStt {
        return execute_openrouter_speech_to_text(
            args,
            &media_http_client,
            &provider_id,
            &provider_config,
            &mime_type,
            &bytes,
        )
        .await;
    }

    let response_format = args
        .get("response_format")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("json");
    let (url, form) = match route {
        AudioToolRoute::ElevenLabs => {
            let fields = build_elevenlabs_stt_multipart_fields(
                args,
                &provider_config.model,
                &filename,
                &mime_type,
            );
            (
                elevenlabs_stt_endpoint(&provider_config.base_url),
                ordered_fields_to_multipart_form(&fields, &bytes)?,
            )
        }
        AudioToolRoute::XaiStt => {
            let fields = build_xai_stt_multipart_fields(args, &filename, &mime_type);
            (
                stt_endpoint_for_route(&provider_config.base_url, route),
                ordered_fields_to_multipart_form(&fields, &bytes)?,
            )
        }
        _ => {
            let file_part = reqwest::multipart::Part::bytes(bytes)
                .file_name(filename)
                .mime_str(&mime_type)?;
            let mut form = reqwest::multipart::Form::new()
                .part("file", file_part)
                .text("model", provider_config.model.clone());
            if let Some(language) = args
                .get("language")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                form = form.text("language", language.to_string());
            }
            if let Some(prompt) = args
                .get("prompt")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                form = form.text("prompt", prompt.to_string());
            }
            (
                stt_endpoint_for_route(&provider_config.base_url, route),
                form.text("response_format", response_format.to_string()),
            )
        }
    };

    let request =
        apply_media_auth_headers(media_http_client.post(&url), &provider_id, &provider_config)?;
    let response = request.multipart(form).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("speech_to_text failed: HTTP {status} {text}");
    }

    let body = response.text().await?;
    if route != AudioToolRoute::XaiStt
        && route != AudioToolRoute::ElevenLabs
        && response_format == "text"
    {
        return Ok(body);
    }

    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| anyhow::anyhow!("speech_to_text returned invalid JSON: {error}"))?;
    format_speech_to_text_response(route, response_format, &parsed)
}

pub(crate) async fn execute_text_to_speech(
    args: &serde_json::Value,
    agent: &AgentEngine,
    _http_client: &reqwest::Client,
) -> Result<String> {
    let media_http_client = fresh_media_http_client(tool_names::TEXT_TO_SPEECH, args);
    let input = args
        .get("input")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'input' argument"))?;
    let (provider_id, provider_config) =
        resolve_media_provider_config(agent, args, Some(MediaEndpoint::TextToSpeech)).await?;
    let route = resolve_audio_tool_route(
        &provider_id,
        &provider_config,
        zorai_shared::providers::AudioToolKind::TextToSpeech,
    )?;

    if route == AudioToolRoute::XiaomiChatCompletionsTts {
        return execute_xiaomi_text_to_speech(
            args,
            &media_http_client,
            &provider_id,
            &provider_config,
            input,
        )
        .await;
    }

    if route == AudioToolRoute::MiniMaxTts {
        return execute_minimax_text_to_speech(
            args,
            &media_http_client,
            &provider_id,
            &provider_config,
            input,
        )
        .await;
    }

    if route == AudioToolRoute::OpenRouterTts {
        return execute_openrouter_text_to_speech(
            args,
            &media_http_client,
            &provider_id,
            &provider_config,
            input,
        )
        .await;
    }

    if route == AudioToolRoute::ElevenLabs {
        let voice = args
            .get("voice")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let url = elevenlabs_tts_endpoint(&provider_config.base_url, voice)?;
        let body = build_elevenlabs_tts_body(input, &provider_config.model);
        let request =
            apply_media_auth_headers(media_http_client.post(&url), &provider_id, &provider_config)?;
        let response = request.json(&body).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("text_to_speech failed: HTTP {status} {text}");
        }

        let bytes = response.bytes().await?;
        let output_path = temp_output_path("speech", "mp3");
        tokio::fs::write(&output_path, &bytes).await?;

        return serde_json::to_string_pretty(&serde_json::json!({
            "provider": provider_id,
            "model": provider_config.model,
            "voice": voice.trim(),
            "path": output_path,
            "mime_type": "audio/mpeg",
            "bytes": bytes.len(),
        }))
        .map_err(Into::into);
    }

    let voice = if route == AudioToolRoute::XaiTts {
        xai_tts_voice(args)
    } else {
        args.get("voice")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_TTS_VOICE)
    };
    let output_format = args
        .get("response_format")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TTS_OUTPUT_FORMAT);
    let url = tts_endpoint_for_route(&provider_config.base_url, route);
    let body = if route == AudioToolRoute::XaiTts {
        build_xai_tts_body(args, input)?
    } else {
        serde_json::json!({
            "model": provider_config.model,
            "input": input,
            "voice": voice,
            "response_format": output_format,
        })
    };

    let request =
        apply_media_auth_headers(media_http_client.post(&url), &provider_id, &provider_config)?;
    let response = request.json(&body).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "text_to_speech failed: HTTP {status} {text} (provider='{}', model='{}', voice='{}', response_format='{}'). If the provider rejected the request, the voice or format is likely not valid for this model.",
            provider_id,
            provider_config.model,
            voice,
            output_format
        );
    }

    let bytes = response.bytes().await?;
    let mime_type = output_mime_type_for_codec(output_format);
    let extension = file_extension_for_generated_mime(mime_type, output_format);
    let output_path = temp_output_path("speech", &extension);
    tokio::fs::write(&output_path, &bytes).await?;

    serde_json::to_string_pretty(&serde_json::json!({
        "provider": provider_id,
        "model": provider_config.model,
        "voice": voice,
        "path": output_path,
        "mime_type": mime_type,
        "bytes": bytes.len(),
    }))
    .map_err(Into::into)
}

#[cfg(test)]
mod media_tools_tests {
    use super::*;

    fn test_provider_config(model: &str) -> crate::agent::types::ProviderConfig {
        crate::agent::types::ProviderConfig {
            base_url: "https://token-plan-ams.xiaomimimo.com/v1".to_string(),
            model: model.to_string(),
            api_key: "sk-mimo".to_string(),
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

    #[test]
    fn infer_image_mime_from_extension() {
        assert_eq!(
            infer_media_mime(Path::new("/tmp/cat.png"), MediaKind::Image),
            "image/png"
        );
        assert_eq!(
            infer_media_mime(Path::new("/tmp/cat.jpeg"), MediaKind::Image),
            "image/jpeg"
        );
    }

    #[test]
    fn parse_data_url_extracts_mime_and_payload() {
        let (mime_type, data) =
            parse_data_url("data:image/png;base64,Zm9v").expect("data url should parse");
        assert_eq!(mime_type, "image/png");
        assert_eq!(data, "Zm9v");
    }

    #[test]
    fn openai_like_endpoint_respects_existing_v1_suffix() {
        assert_eq!(
            openai_like_endpoint("https://api.example.com/v1", "audio/speech"),
            "https://api.example.com/v1/audio/speech"
        );
        assert_eq!(
            openai_like_endpoint("https://api.example.com", "/audio/speech"),
            "https://api.example.com/v1/audio/speech"
        );
    }

    #[test]
    fn openrouter_speech_to_text_uses_json_input_audio_route() {
        assert_eq!(
            audio_tool_route(
                zorai_shared::providers::PROVIDER_ID_OPENROUTER,
                zorai_shared::providers::AudioToolKind::SpeechToText,
            ),
            AudioToolRoute::OpenRouterJsonStt
        );
    }

    #[test]
    fn openrouter_text_to_speech_uses_dedicated_audio_speech_route() {
        assert_eq!(
            audio_tool_route(
                zorai_shared::providers::PROVIDER_ID_OPENROUTER,
                zorai_shared::providers::AudioToolKind::TextToSpeech,
            ),
            AudioToolRoute::OpenRouterTts
        );
        assert_eq!(
            tts_endpoint_for_route(
                "https://openrouter.ai/api/v1",
                AudioToolRoute::OpenRouterTts
            ),
            "https://openrouter.ai/api/v1/audio/speech"
        );
    }

    #[test]
    fn chunk_tts_input_keeps_short_text_in_one_chunk() {
        let chunks = chunk_tts_input("Hello world.", 3_000);
        assert_eq!(chunks, vec!["Hello world.".to_string()]);
    }

    #[test]
    fn chunk_tts_input_splits_long_text_on_word_boundaries_within_limit() {
        let sentence = "word ".repeat(50);
        let chunks = chunk_tts_input(&sentence, 20);
        assert!(chunks.len() > 1, "expected multiple chunks, got {chunks:?}");
        for chunk in &chunks {
            assert!(
                chunk.chars().count() <= 20,
                "chunk exceeded limit: {chunk:?}"
            );
            assert_eq!(chunk.trim(), chunk, "chunks should be trimmed");
        }
        let rejoined = chunks.join(" ");
        assert_eq!(rejoined.split_whitespace().count(), 50);
    }

    #[test]
    fn chunk_tts_input_hard_splits_oversized_token() {
        let token = "a".repeat(45);
        let chunks = chunk_tts_input(&token, 10);
        assert_eq!(chunks.len(), 5);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 10));
        assert_eq!(chunks.concat(), token);
    }

    #[test]
    fn wrap_pcm_as_wav_writes_canonical_header() {
        let pcm = vec![1u8, 2, 3, 4];
        let wav = wrap_pcm_as_wav(&pcm, 24_000, 1, 16);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(
            u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]),
            pcm.len() as u32
        );
        assert_eq!(&wav[44..], pcm.as_slice());
        let (channels, sample_rate, bits, data) = parse_wav(&wav).expect("round-trips");
        assert_eq!((channels, sample_rate, bits), (1, 24_000, 16));
        assert_eq!(data, pcm);
    }

    #[test]
    fn merge_wav_chunks_concatenates_pcm_payloads() {
        let first = wrap_pcm_as_wav(&[1, 2, 3, 4], 24_000, 1, 16);
        let second = wrap_pcm_as_wav(&[5, 6, 7, 8], 24_000, 1, 16);
        let merged = merge_wav_chunks(&[first, second]).expect("merge should succeed");
        let (channels, sample_rate, bits, data) = parse_wav(&merged).expect("merged is valid wav");
        assert_eq!((channels, sample_rate, bits), (1, 24_000, 16));
        assert_eq!(data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn combine_openrouter_tts_chunks_wraps_pcm_into_wav() {
        let (bytes, mime, ext) =
            combine_openrouter_tts_chunks("pcm", &[vec![1, 2], vec![3, 4]]).expect("combine pcm");
        assert_eq!(mime, "audio/wav");
        assert_eq!(ext, "wav");
        let (_, _, _, data) = parse_wav(&bytes).expect("pcm wrapped into wav");
        assert_eq!(data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn openrouter_stt_body_uses_input_audio_with_base64_data() {
        let body = build_openrouter_stt_body(
            &serde_json::json!({"language": "pl", "temperature": 0.2}),
            "openai/whisper-1",
            "aGVsbG8=",
            "wav",
        );
        assert_eq!(body["model"], "openai/whisper-1");
        assert_eq!(body["input_audio"]["data"], "aGVsbG8=");
        assert_eq!(body["input_audio"]["format"], "wav");
        assert_eq!(body["language"], "pl");
        assert_eq!(body["temperature"], 0.2);
    }

    #[test]
    fn openrouter_defaults_use_openai_prefixed_model_ids() {
        assert_eq!(
            provider_specific_default_media_model(
                zorai_shared::providers::PROVIDER_ID_OPENROUTER,
                MediaEndpoint::TextToSpeech,
            ),
            Some("openai/gpt-4o-mini-tts")
        );
        assert_eq!(
            provider_specific_default_media_model(
                zorai_shared::providers::PROVIDER_ID_OPENROUTER,
                MediaEndpoint::SpeechToText,
            ),
            Some("openai/whisper-1")
        );
        assert_eq!(
            provider_specific_default_media_model(
                zorai_shared::providers::PROVIDER_ID_OPENAI,
                MediaEndpoint::TextToSpeech,
            ),
            None
        );
    }

    #[test]
    fn openrouter_transcription_blocks_include_audio_input() {
        let blocks = build_audio_transcription_blocks(
            ApiTransport::ChatCompletions,
            "Please transcribe this audio.",
            "UklGRg==",
            "wav",
        );

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "input_audio");
        assert_eq!(blocks[1]["input_audio"]["format"], "wav");
        assert_eq!(blocks[1]["input_audio"]["data"], "UklGRg==");
    }

    #[test]
    fn xai_speech_to_text_uses_xai_stt_route_and_endpoint() {
        let route = audio_tool_route(
            zorai_shared::providers::PROVIDER_ID_XAI,
            zorai_shared::providers::AudioToolKind::SpeechToText,
        );
        assert_eq!(route, AudioToolRoute::XaiStt);
        assert_eq!(
            stt_endpoint_for_route("https://api.x.ai/v1", route),
            "https://api.x.ai/v1/stt"
        );
    }

    #[test]
    fn xai_text_to_speech_uses_xai_tts_route_and_endpoint() {
        let route = audio_tool_route(
            zorai_shared::providers::PROVIDER_ID_XAI,
            zorai_shared::providers::AudioToolKind::TextToSpeech,
        );
        assert_eq!(route, AudioToolRoute::XaiTts);
        assert_eq!(
            tts_endpoint_for_route("https://api.x.ai/v1", route),
            "https://api.x.ai/v1/tts"
        );
    }

    #[test]
    fn elevenlabs_speech_to_text_uses_native_endpoint() {
        let route = audio_tool_route(
            zorai_shared::providers::PROVIDER_ID_ELEVENLABS,
            zorai_shared::providers::AudioToolKind::SpeechToText,
        );
        assert_eq!(route, AudioToolRoute::ElevenLabs);
        assert_eq!(
            elevenlabs_stt_endpoint("https://api.elevenlabs.io"),
            "https://api.elevenlabs.io/v1/speech-to-text"
        );
    }

    #[test]
    fn elevenlabs_text_to_speech_uses_voice_id_endpoint() {
        let route = audio_tool_route(
            zorai_shared::providers::PROVIDER_ID_ELEVENLABS,
            zorai_shared::providers::AudioToolKind::TextToSpeech,
        );
        assert_eq!(route, AudioToolRoute::ElevenLabs);
        assert_eq!(
            elevenlabs_tts_endpoint("https://api.elevenlabs.io", "JBFqnCBsd6RMkjVDRZzb")
                .expect("voice endpoint should build"),
            "https://api.elevenlabs.io/v1/text-to-speech/JBFqnCBsd6RMkjVDRZzb"
        );
    }

    #[test]
    fn elevenlabs_stt_multipart_fields_map_model_language_and_file() {
        let fields = build_elevenlabs_stt_multipart_fields(
            &serde_json::json!({
                "language": "pl"
            }),
            "scribe_v2",
            "capture.webm",
            "audio/webm",
        );

        assert_eq!(
            fields,
            vec![
                OrderedMultipartField::Text {
                    name: "model_id".to_string(),
                    value: "scribe_v2".to_string()
                },
                OrderedMultipartField::Text {
                    name: "language_code".to_string(),
                    value: "pl".to_string()
                },
                OrderedMultipartField::File {
                    name: "file".to_string(),
                    filename: "capture.webm".to_string(),
                    mime_type: "audio/webm".to_string()
                }
            ]
        );
    }

    #[test]
    fn elevenlabs_tts_body_maps_input_and_model_id() {
        let body = build_elevenlabs_tts_body("Read this aloud", "eleven_multilingual_v2");

        assert_eq!(body["text"], "Read this aloud");
        assert_eq!(body["model_id"], "eleven_multilingual_v2");
        assert!(body.get("voice").is_none());
    }

    #[test]
    fn elevenlabs_tts_endpoint_requires_voice_id() {
        let error = elevenlabs_tts_endpoint("https://api.elevenlabs.io", " ")
            .expect_err("empty voice id should be rejected");

        assert!(error
            .to_string()
            .contains("requires audio_tts_voice to be an ElevenLabs voice ID"));
    }

    #[test]
    fn xiaomi_text_to_speech_uses_completion_route() {
        let route = audio_tool_route(
            zorai_shared::providers::PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
            zorai_shared::providers::AudioToolKind::TextToSpeech,
        );
        assert_eq!(route, AudioToolRoute::XiaomiChatCompletionsTts);
    }

    #[test]
    fn minimax_text_to_speech_uses_minimax_native_route() {
        let route = audio_tool_route(
            zorai_shared::providers::PROVIDER_ID_MINIMAX,
            zorai_shared::providers::AudioToolKind::TextToSpeech,
        );
        assert_eq!(route, AudioToolRoute::MiniMaxTts);
    }

    #[test]
    fn minimax_image_generation_uses_native_route() {
        assert_eq!(
            image_generation_route(zorai_shared::providers::PROVIDER_ID_MINIMAX),
            ImageGenerationRoute::MiniMaxDirect
        );
        assert_eq!(
            image_generation_route(zorai_shared::providers::PROVIDER_ID_MINIMAX_CODING_PLAN),
            ImageGenerationRoute::MiniMaxDirect
        );
    }

    #[test]
    fn minimax_tts_body_maps_voice_and_audio_settings() {
        let body = build_minimax_tts_body(
            "speech-2.8-hd",
            "Read this aloud",
            Some("English_expressive_narrator"),
            "wav",
        );

        assert_eq!(body["model"], "speech-2.8-hd");
        assert_eq!(body["text"], "Read this aloud");
        assert_eq!(
            body["voice_setting"]["voice_id"],
            "English_expressive_narrator"
        );
        assert_eq!(body["audio_setting"]["format"], "wav");
        assert_eq!(body["output_format"], "hex");
    }

    #[test]
    fn minimax_tts_surfaces_base_resp_error_instead_of_missing_audio() {
        let payload = serde_json::json!({
            "base_resp": {
                "status_code": 1004,
                "status_msg": "insufficient balance"
            }
        });

        let error = extract_minimax_tts_audio_bytes(&payload)
            .expect_err("non-zero base_resp should surface as an error");
        assert_eq!(
            error.to_string(),
            "MiniMax error 1004: insufficient balance"
        );
    }

    #[test]
    fn minimax_tts_decodes_audio_when_base_resp_is_success() {
        let payload = serde_json::json!({
            "data": { "audio": "68656c6c6f" },
            "base_resp": { "status_code": 0, "status_msg": "success" }
        });

        let bytes = extract_minimax_tts_audio_bytes(&payload).expect("audio should decode");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn minimax_image_generation_body_uses_base64_response_format() {
        let body = build_minimax_image_generation_body(
            &serde_json::json!({
                "size": "1344x768",
            }),
            "image-01",
            "Generate a poster",
        );

        assert_eq!(body["model"], "image-01");
        assert_eq!(body["prompt"], "Generate a poster");
        assert_eq!(body["aspect_ratio"], "16:9");
        assert_eq!(body["response_format"], "base64");
    }

    #[test]
    fn xiaomi_tts_body_uses_assistant_content_and_audio_settings() {
        let provider_config = test_provider_config("mimo-v2.5-tts");

        let body = build_xiaomi_tts_body(&provider_config, "Read this aloud", Some("Chloe"), "wav")
            .expect("body should build");

        assert_eq!(body["model"], "mimo-v2.5-tts");
        assert_eq!(body["messages"][0]["role"], "assistant");
        assert_eq!(body["messages"][0]["content"], "Read this aloud");
        assert_eq!(body["audio"]["format"], "wav");
        assert_eq!(body["audio"]["voice"], "Chloe");
    }

    #[test]
    fn xiaomi_voice_design_uses_voice_as_style_prompt() {
        let provider_config = test_provider_config("mimo-v2.5-tts-voicedesign");

        let body = build_xiaomi_tts_body(
            &provider_config,
            "Say hello",
            Some("Young male tone"),
            "pcm",
        )
        .expect("body should build");

        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Young male tone");
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][1]["content"], "Say hello");
        assert_eq!(body["audio"]["format"], "pcm16");
    }

    #[test]
    fn xiaomi_voice_clone_requires_voice_sample() {
        let provider_config = test_provider_config("mimo-v2.5-tts-voiceclone");

        let error = build_xiaomi_tts_body(&provider_config, "Say hello", None, "wav")
            .expect_err("voiceclone should require a voice sample");
        assert!(error.to_string().contains("requires a non-empty 'voice'"));
    }

    #[test]
    fn xiaomi_tts_audio_bytes_decode_from_completion_payload() {
        let payload = serde_json::json!({
            "choices": [{
                "message": {
                    "audio": {
                        "data": "aGVsbG8="
                    }
                }
            }]
        });

        let bytes = extract_xiaomi_tts_audio_bytes(&payload).expect("audio bytes should decode");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn xai_stt_multipart_fields_append_file_last() {
        let fields = build_xai_stt_multipart_fields(
            &serde_json::json!({
                "language": "en",
                "format": true,
                "audio_format": "wav",
                "sample_rate": 16000,
                "multichannel": false,
                "channels": 1,
                "diarize": true
            }),
            "clip.wav",
            "audio/wav",
        );

        assert_eq!(
            fields,
            vec![
                OrderedMultipartField::Text {
                    name: "language".to_string(),
                    value: "en".to_string()
                },
                OrderedMultipartField::Text {
                    name: "format".to_string(),
                    value: "true".to_string()
                },
                OrderedMultipartField::Text {
                    name: "audio_format".to_string(),
                    value: "wav".to_string()
                },
                OrderedMultipartField::Text {
                    name: "sample_rate".to_string(),
                    value: "16000".to_string()
                },
                OrderedMultipartField::Text {
                    name: "multichannel".to_string(),
                    value: "false".to_string()
                },
                OrderedMultipartField::Text {
                    name: "channels".to_string(),
                    value: "1".to_string()
                },
                OrderedMultipartField::Text {
                    name: "diarize".to_string(),
                    value: "true".to_string()
                },
                OrderedMultipartField::File {
                    name: "file".to_string(),
                    filename: "clip.wav".to_string(),
                    mime_type: "audio/wav".to_string()
                }
            ]
        );
    }

    #[test]
    fn xai_tts_body_maps_input_voice_language_and_output_format() {
        let body = build_xai_tts_body(
            &serde_json::json!({
                "input": "Hello from zorai",
                "voice": "nova",
                "language": "pl",
                "response_format": "wav",
                "sample_rate": 24000,
                "bit_rate": 128000
            }),
            "Hello from zorai",
        )
        .expect("xai body should build");

        assert_eq!(body["text"], "Hello from zorai");
        assert_eq!(body["voice_id"], "nova");
        assert_eq!(body["language"], "pl");
        assert_eq!(body["output_format"]["codec"], "wav");
        assert_eq!(body["output_format"]["sample_rate"], 24000);
        assert_eq!(body["output_format"]["bit_rate"], 128000);
        assert!(body.get("model").is_none());
    }

    #[test]
    fn xai_tts_body_includes_default_mp3_codec_when_response_format_is_omitted() {
        let body = build_xai_tts_body(
            &serde_json::json!({
                "input": "Hello from zorai",
                "language": "en"
            }),
            "Hello from zorai",
        )
        .expect("xai body should build");

        assert_eq!(body["output_format"]["codec"], "mp3");
    }

    #[test]
    fn xai_tts_body_requires_language() {
        let error = build_xai_tts_body(
            &serde_json::json!({
                "input": "Hello from zorai"
            }),
            "Hello from zorai",
        )
        .expect_err("xai body should reject missing language");

        assert!(error
            .to_string()
            .contains("requires a non-empty 'language' argument"));
    }

    #[test]
    fn xai_stt_formats_json_response_according_to_daemon_contract() {
        let payload = serde_json::json!({
            "text": "hello world",
            "duration": 1.23
        });

        assert_eq!(
            format_speech_to_text_response(AudioToolRoute::XaiStt, "text", &payload)
                .expect("xai text contract should parse"),
            "hello world"
        );
        assert_eq!(
            format_speech_to_text_response(AudioToolRoute::XaiStt, "json", &payload)
                .expect("xai json contract should pretty-print transcript payload"),
            serde_json::to_string_pretty(&payload).expect("payload should serialize")
        );
    }

    #[test]
    fn direct_provider_stt_text_contract_still_returns_plain_text_body() {
        let payload = serde_json::json!({
            "text": "hello world"
        });

        assert_eq!(
            format_speech_to_text_response(
                AudioToolRoute::OpenAiCompatibleDirect,
                "text",
                &payload
            )
            .expect("text contract should extract transcript"),
            "hello world"
        );
    }

    #[test]
    fn telephony_codecs_map_to_correct_tts_mime_types() {
        assert_eq!(output_mime_type_for_codec("mulaw"), "audio/basic");
        assert_eq!(output_mime_type_for_codec("alaw"), "audio/alaw");
    }

    #[tokio::test]
    async fn resolve_media_provider_config_uses_nested_audio_tts_settings() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4".to_string();
        config.api_key = "sk-active".to_string();
        config.extra.insert(
            "audio".to_string(),
            serde_json::json!({
                "tts": {
                    "provider": "custom",
                    "model": "sonic-voice"
                }
            }),
        );
        config.providers.insert(
            "custom".to_string(),
            crate::agent::types::ProviderConfig {
                base_url: "https://audio.example/v1".to_string(),
                model: "fallback-model".to_string(),
                api_key: "sk-audio".to_string(),
                assistant_id: String::new(),
                auth_source: crate::agent::types::AuthSource::ApiKey,
                api_transport: crate::agent::types::ApiTransport::Responses,
                reasoning_effort: "medium".to_string(),
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
            },
        );

        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let (provider_id, provider_config) = resolve_media_provider_config(
            &engine,
            &serde_json::json!({}),
            Some(MediaEndpoint::TextToSpeech),
        )
        .await
        .expect("audio TTS settings should resolve");

        assert_eq!(provider_id, "custom");
        assert_eq!(provider_config.model, "sonic-voice");
        assert_eq!(provider_config.base_url, "https://audio.example/v1");
    }

    #[tokio::test]
    async fn resolve_media_provider_config_falls_back_to_flattened_legacy_audio_keys() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4".to_string();
        config.api_key = "sk-active".to_string();
        config.extra.insert(
            "audio_stt_provider".to_string(),
            serde_json::Value::String("custom".to_string()),
        );
        config.extra.insert(
            "audio_stt_model".to_string(),
            serde_json::Value::String("whisper-legacy".to_string()),
        );
        config.providers.insert(
            "custom".to_string(),
            crate::agent::types::ProviderConfig {
                base_url: "https://audio.example/v1".to_string(),
                model: "fallback-model".to_string(),
                api_key: "sk-audio".to_string(),
                assistant_id: String::new(),
                auth_source: crate::agent::types::AuthSource::ApiKey,
                api_transport: crate::agent::types::ApiTransport::Responses,
                reasoning_effort: "medium".to_string(),
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
            },
        );

        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let (provider_id, provider_config) = resolve_media_provider_config(
            &engine,
            &serde_json::json!({}),
            Some(MediaEndpoint::SpeechToText),
        )
        .await
        .expect("legacy STT settings should resolve");

        assert_eq!(provider_id, "custom");
        assert_eq!(provider_config.model, "whisper-legacy");
        assert_eq!(provider_config.base_url, "https://audio.example/v1");
    }

    #[tokio::test]
    async fn resolve_media_provider_config_uses_nested_image_generation_settings() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4".to_string();
        config.api_key = "sk-active".to_string();
        config.extra.insert(
            "image".to_string(),
            serde_json::json!({
                "generation": {
                    "provider": "custom",
                    "model": "flux-pro"
                }
            }),
        );
        config.providers.insert(
            "custom".to_string(),
            crate::agent::types::ProviderConfig {
                base_url: "https://images.example/v1".to_string(),
                model: "fallback-model".to_string(),
                api_key: "sk-image".to_string(),
                assistant_id: String::new(),
                auth_source: crate::agent::types::AuthSource::ApiKey,
                api_transport: crate::agent::types::ApiTransport::Responses,
                reasoning_effort: "medium".to_string(),
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
            },
        );

        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let (provider_id, provider_config) = resolve_media_provider_config(
            &engine,
            &serde_json::json!({}),
            Some(MediaEndpoint::ImageGeneration),
        )
        .await
        .expect("image generation settings should resolve");

        assert_eq!(provider_id, "custom");
        assert_eq!(provider_config.model, "flux-pro");
        assert_eq!(provider_config.base_url, "https://images.example/v1");
    }

    #[tokio::test]
    async fn persist_generated_image_for_thread_records_messages_and_work_context() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let (thread_id, _) = engine.get_or_create_thread(None, "retro robot").await;

        let temp_source = root.path().join("upstream-image.png");
        tokio::fs::write(&temp_source, b"png-bytes")
            .await
            .expect("temp image should write");

        let persisted = persist_generated_image_for_thread(
            &engine,
            &thread_id,
            "retro robot",
            &serde_json::json!({
                "provider": "openai",
                "model": "gpt-image-1",
                "path": temp_source.to_string_lossy(),
                "mime_type": "image/png",
                "revised_prompt": "retro robot"
            })
            .to_string(),
            true,
        )
        .await
        .expect("generated image should persist");
        let persisted_value: serde_json::Value =
            serde_json::from_str(&persisted).expect("result should deserialize");
        let persisted_path = persisted_value
            .get("path")
            .and_then(|value| value.as_str())
            .expect("persisted result should include final path");
        assert!(
            persisted_path.contains("/threads/") && persisted_path.contains("/artifacts/media/"),
            "expected generated image to move into thread artifact media dir, got {persisted_path}"
        );
        assert!(
            tokio::fs::metadata(persisted_path).await.is_ok(),
            "persisted thread image should exist on disk"
        );

        let thread = engine
            .get_thread_filtered(&thread_id, true, None, 0)
            .await
            .expect("thread should exist");
        assert_eq!(thread.thread.messages.len(), 2);
        assert_eq!(thread.thread.messages[0].role, MessageRole::User);
        assert_eq!(thread.thread.messages[0].content, "🖼 retro robot");
        assert_eq!(thread.thread.messages[1].role, MessageRole::Assistant);
        assert_eq!(
            thread.thread.messages[1].provider.as_deref(),
            Some("openai")
        );
        assert_eq!(
            thread.thread.messages[1].model.as_deref(),
            Some("gpt-image-1")
        );
        assert!(matches!(
            thread.thread.messages[1].content_blocks.first(),
            Some(AgentContentBlock::Image { url: Some(url), mime_type: Some(mime_type), .. })
                if url.starts_with("file://") && mime_type == "image/png"
        ));

        let context = engine
            .thread_work_contexts
            .read()
            .await
            .get(&thread_id)
            .cloned()
            .expect("work context should be recorded");
        assert_eq!(context.entries.len(), 1);
        assert_eq!(context.entries[0].source, tool_names::GENERATE_IMAGE);
    }

    #[tokio::test]
    async fn persist_generated_image_for_tool_call_skips_synthetic_prompt_message() {
        let root = tempfile::tempdir().expect("tempdir should succeed");
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let (thread_id, _) = engine.get_or_create_thread(None, "retro robot").await;

        let temp_source = root.path().join("tool-image.png");
        tokio::fs::write(&temp_source, b"png-bytes")
            .await
            .expect("temp image should write");

        persist_generated_image_for_thread(
            &engine,
            &thread_id,
            "retro robot",
            &serde_json::json!({
                "provider": "openai",
                "model": "gpt-image-1",
                "path": temp_source.to_string_lossy(),
                "mime_type": "image/png",
            })
            .to_string(),
            false,
        )
        .await
        .expect("generated image should persist");

        let thread = engine
            .get_thread_filtered(&thread_id, true, None, 0)
            .await
            .expect("thread should exist");
        assert_eq!(thread.thread.messages.len(), 1);
        assert_eq!(thread.thread.messages[0].role, MessageRole::Assistant);
        assert!(matches!(
            thread.thread.messages[0].content_blocks.first(),
            Some(AgentContentBlock::Image { .. })
        ));
    }

    #[test]
    fn image_generation_route_prefers_openrouter_chat_completions() {
        assert_eq!(
            image_generation_route(zorai_shared::providers::PROVIDER_ID_OPENROUTER),
            ImageGenerationRoute::OpenRouterChatCompletions
        );
        assert_eq!(
            image_generation_route(zorai_shared::providers::PROVIDER_ID_OPENAI),
            ImageGenerationRoute::OpenAiCompatibleDirect
        );
    }

    #[test]
    fn build_openrouter_image_generation_body_maps_size_into_image_config() {
        let provider_config = crate::agent::types::ProviderConfig {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model: "google/gemini-3-pro-image-preview".to_string(),
            api_key: "sk-openrouter".to_string(),
            assistant_id: String::new(),
            auth_source: crate::agent::types::AuthSource::ApiKey,
            api_transport: crate::agent::types::ApiTransport::Responses,
            reasoning_effort: "medium".to_string(),
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
        };

        let body = build_openrouter_image_generation_body(
            &serde_json::json!({
                "prompt": "forge a mythic icon",
                "size": "1024x1024",
                "quality": "high",
                "output_format": "png"
            }),
            &provider_config,
            "forge a mythic icon",
        );

        assert_eq!(
            body.get("model").and_then(|value| value.as_str()),
            Some("google/gemini-3-pro-image-preview")
        );
        assert_eq!(
            body.get("modalities")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(2)
        );
        assert_eq!(
            body.pointer("/image_config/aspect_ratio")
                .and_then(|value| value.as_str()),
            Some("1:1")
        );
        assert_eq!(
            body.pointer("/image_config/image_size")
                .and_then(|value| value.as_str()),
            Some("1K")
        );
        assert_eq!(
            body.get("messages")
                .and_then(|value| value.as_array())
                .and_then(|items| items.first())
                .and_then(|value| value.get("content"))
                .and_then(|value| value.as_str()),
            Some("forge a mythic icon")
        );
    }

    #[test]
    fn extract_openrouter_image_output_accepts_snake_case_image_urls() {
        let payload = serde_json::json!({
            "choices": [{
                "message": {
                    "images": [{
                        "image_url": {
                            "url": "data:image/png;base64,aGVsbG8="
                        }
                    }]
                }
            }]
        });

        let extracted =
            extract_openrouter_generated_image(&payload).expect("image should be extracted");
        assert_eq!(extracted, "data:image/png;base64,aGVsbG8=");
    }
}
