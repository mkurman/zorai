use super::*;
use base64::Engine as _;

pub(crate) const MAX_INLINE_VIDEO_BYTES: u64 = 20 * 1024 * 1024;

pub(crate) fn video_api_block(
    url: Option<&str>,
    data_url: Option<&str>,
    mime_type: Option<&str>,
) -> Option<serde_json::Value> {
    let url = url.map(str::trim).filter(|value| !value.is_empty());
    let data_url = data_url.map(str::trim).filter(|value| !value.is_empty());
    let path = url.and_then(|value| value.strip_prefix("file://"));
    let video_url = data_url.or(url.filter(|value| !value.starts_with("file://")));
    if path.is_none() && video_url.is_none() {
        return None;
    }
    Some(serde_json::json!({
        "type": "input_video",
        "video_url": video_url,
        "path": path,
        "mime_type": mime_type,
    }))
}

pub(crate) fn native_video_supported(
    provider: &str,
    model: &str,
    api_type: ApiType,
    transport: ApiTransport,
) -> bool {
    crate::agent::types::model_supports(provider, model, crate::agent::types::Modality::Video)
        && (api_type == ApiType::Gemini
            || (api_type == ApiType::OpenAI && transport == ApiTransport::ChatCompletions))
}

pub(crate) fn prepare_video_blocks(messages: Vec<ApiMessage>, native: bool) -> Vec<ApiMessage> {
    messages
        .into_iter()
        .map(|mut message| {
            if let ApiContent::Blocks(blocks) = &mut message.content {
                for block in blocks.iter_mut() {
                    if block.get("type").and_then(|value| value.as_str()) == Some("input_video") {
                        *block = prepare_video_block(block, native);
                    }
                }
            }
            message
        })
        .collect()
}

fn prepare_video_block(block: &serde_json::Value, native: bool) -> serde_json::Value {
    let path = block.get("path").and_then(|value| value.as_str());
    let mime_type = block
        .get("mime_type")
        .and_then(|value| value.as_str())
        .unwrap_or("video/mp4");
    if !native {
        return video_hint(block, None);
    }
    if block
        .get("video_url")
        .and_then(|value| value.as_str())
        .is_some()
    {
        return block.clone();
    }
    let Some(path) = path else {
        return video_hint(block, None);
    };
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_INLINE_VIDEO_BYTES => video_hint(
            block,
            Some(format!(
                "the file is {} MB, over the {} MB inline limit",
                meta.len() / (1024 * 1024),
                MAX_INLINE_VIDEO_BYTES / (1024 * 1024)
            )),
        ),
        Ok(_) => match std::fs::read(path) {
            Ok(bytes) => {
                let mut prepared = block.clone();
                prepared["video_url"] = serde_json::Value::String(format!(
                    "data:{mime_type};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ));
                prepared
            }
            Err(error) => video_hint(block, Some(format!("it could not be read: {error}"))),
        },
        Err(error) => video_hint(block, Some(format!("it could not be read: {error}"))),
    }
}

fn video_hint(block: &serde_json::Value, reason: Option<String>) -> serde_json::Value {
    let path = block.get("path").and_then(|value| value.as_str());
    let url = block
        .get("video_url")
        .and_then(|value| value.as_str())
        .filter(|value| !value.starts_with("data:"));
    let (label, argument) = match (path, url) {
        (Some(path), _) => (path.to_string(), format!("path=\"{path}\"")),
        (None, Some(url)) => (url.to_string(), format!("url=\"{url}\"")),
        (None, None) => (
            "inline video".to_string(),
            "the original file path".to_string(),
        ),
    };
    let why = reason.unwrap_or_else(|| "the current model cannot watch video".to_string());
    serde_json::json!({
        "type": "input_text",
        "text": format!(
            "[Video attachment: {label}. It was not sent to the model because {why}. Call the analyze_video tool with {argument} to inspect it.]"
        ),
    })
}
