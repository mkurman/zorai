use super::*;

const MUSIC_POLL_INTERVAL: std::time::Duration = if cfg!(test) {
    std::time::Duration::from_millis(10)
} else {
    std::time::Duration::from_secs(3)
};
const DEFAULT_MUSIC_MIME_TYPE: &str = "audio/mpeg";
const ELEVENLABS_MIN_MUSIC_MS: u64 = 3_000;
const ELEVENLABS_MAX_MUSIC_MS: u64 = 600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MusicGenerationRoute {
    ElevenLabs,
    GeminiLyria,
    MiniMax,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MusicJobPoll {
    Pending,
    Ready {
        mime_type: String,
        data: String,
        lyrics: Option<String>,
    },
}

struct GeneratedMusic {
    job_id: Option<String>,
    bytes: Vec<u8>,
    mime_type: String,
    lyrics: Option<String>,
}

fn music_generation_route(provider_id: &str) -> Option<MusicGenerationRoute> {
    if is_minimax_provider(provider_id) {
        return Some(MusicGenerationRoute::MiniMax);
    }
    match provider_id {
        zorai_shared::providers::PROVIDER_ID_ELEVENLABS => Some(MusicGenerationRoute::ElevenLabs),
        zorai_shared::providers::PROVIDER_ID_GEMINI => Some(MusicGenerationRoute::GeminiLyria),
        _ => None,
    }
}

pub(super) fn provider_specific_default_music_model(provider_id: &str) -> Option<&'static str> {
    match music_generation_route(provider_id)? {
        MusicGenerationRoute::ElevenLabs => Some("music_v2"),
        MusicGenerationRoute::GeminiLyria => Some("lyria-3.5"),
        MusicGenerationRoute::MiniMax => Some("music-3.0"),
    }
}

fn music_arg_str<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn music_instrumental(args: &serde_json::Value) -> bool {
    args.get("instrumental")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn music_duration_seconds(args: &serde_json::Value) -> Option<u64> {
    args.get("duration_seconds")
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.round() as u64)
}

fn music_prompt_text(args: &serde_json::Value, prompt: &str, describe_length: bool) -> String {
    let mut text = prompt.trim().to_string();
    if music_instrumental(args) {
        text.push_str("\n\nInstrumental only, no vocals.");
    } else if let Some(lyrics) = music_arg_str(args, "lyrics") {
        text.push_str("\n\nLyrics:\n");
        text.push_str(lyrics);
    }
    if describe_length {
        if let Some(seconds) = music_duration_seconds(args) {
            text.push_str(&format!("\n\nLength: about {seconds} seconds."));
        }
    }
    text
}

fn build_elevenlabs_music_body(
    args: &serde_json::Value,
    model: &str,
    prompt: &str,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "prompt": music_prompt_text(args, prompt, false),
        "model_id": model,
    });
    if let Some(seconds) = music_duration_seconds(args) {
        body["music_length_ms"] = (seconds * 1000)
            .clamp(ELEVENLABS_MIN_MUSIC_MS, ELEVENLABS_MAX_MUSIC_MS)
            .into();
    }
    if music_instrumental(args) {
        body["force_instrumental"] = true.into();
    }
    body
}

fn build_gemini_lyria_body(
    args: &serde_json::Value,
    model: &str,
    prompt: &str,
) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "input": music_prompt_text(args, prompt, true),
        "response_format": { "type": "audio" },
    })
}

fn build_minimax_music_body(
    args: &serde_json::Value,
    model: &str,
    prompt: &str,
) -> serde_json::Value {
    let instrumental = music_instrumental(args);
    let mut description = prompt.trim().to_string();
    if let Some(seconds) = music_duration_seconds(args) {
        description.push_str(&format!("\n\nLength: about {seconds} seconds."));
    }
    let mut body = serde_json::json!({
        "model": model,
        "prompt": description,
        "is_instrumental": instrumental,
        "audio_setting": { "sample_rate": 44100, "bitrate": 256000, "format": "mp3" },
        "output_format": "hex",
    });
    if !instrumental {
        match music_arg_str(args, "lyrics") {
            Some(lyrics) => body["lyrics"] = lyrics.into(),
            None => body["lyrics_optimizer"] = true.into(),
        }
    }
    body
}

fn parse_gemini_lyria_interaction(payload: &serde_json::Value) -> Result<MusicJobPoll> {
    if let Some(error) = payload.get("error") {
        let message = error
            .get("message")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| error.to_string());
        anyhow::bail!("Gemini music generation failed: {message}");
    }
    match payload.get("status").and_then(|value| value.as_str()) {
        Some("completed") | None => {}
        Some("failed" | "cancelled") => anyhow::bail!("Gemini music generation failed"),
        _ => return Ok(MusicJobPoll::Pending),
    }
    let blocks = payload
        .get("steps")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter(|step| step.get("type").and_then(|value| value.as_str()) == Some("model_output"))
        .filter_map(|step| step.get("content").and_then(|value| value.as_array()))
        .flatten()
        .collect::<Vec<_>>();
    let audio = blocks
        .iter()
        .find(|block| block.get("type").and_then(|value| value.as_str()) == Some("audio"));
    let Some(audio) = audio else {
        if payload.get("status").is_none() {
            return Ok(MusicJobPoll::Pending);
        }
        anyhow::bail!("Gemini finished without returning audio");
    };
    let data = audio
        .get("data")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("Gemini audio output has no data"))?
        .to_string();
    let mime_type = audio
        .get("mime_type")
        .or_else(|| audio.get("mimeType"))
        .and_then(|value| value.as_str())
        .unwrap_or(DEFAULT_MUSIC_MIME_TYPE)
        .to_string();
    let lyrics = blocks
        .iter()
        .filter(|block| block.get("type").and_then(|value| value.as_str()) == Some("text"))
        .filter_map(|block| block.get("text").and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(MusicJobPoll::Ready {
        mime_type,
        data,
        lyrics: (!lyrics.is_empty()).then_some(lyrics),
    })
}

async fn music_json_response(request: reqwest::RequestBuilder) -> Result<serde_json::Value> {
    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("music generation failed: HTTP {status} {text}");
    }
    Ok(response.json().await?)
}

async fn generate_elevenlabs_music(
    client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    args: &serde_json::Value,
    prompt: &str,
) -> Result<GeneratedMusic> {
    let body = build_elevenlabs_music_body(args, &provider_config.model, prompt);
    let response = apply_media_auth_headers(
        client.post(elevenlabs_api_endpoint(&provider_config.base_url, "music")),
        provider_id,
        provider_config,
    )?
    .json(&body)
    .send()
    .await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("music generation failed: HTTP {status} {text}");
    }
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| value.starts_with("audio/"))
        .unwrap_or(DEFAULT_MUSIC_MIME_TYPE)
        .to_string();
    let job_id = response
        .headers()
        .get("song-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = response.bytes().await?.to_vec();
    if bytes.is_empty() {
        anyhow::bail!("ElevenLabs returned an empty song");
    }
    Ok(GeneratedMusic {
        job_id,
        bytes,
        mime_type,
        lyrics: None,
    })
}

async fn generate_minimax_music(
    client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    args: &serde_json::Value,
    prompt: &str,
) -> Result<GeneratedMusic> {
    let base = minimax_media_base_url(&provider_config.base_url);
    let body = build_minimax_music_body(args, &provider_config.model, prompt);
    let payload = music_json_response(
        apply_media_auth_headers(
            client.post(format!("{base}/music_generation")),
            provider_id,
            provider_config,
        )?
        .json(&body),
    )
    .await?;
    if let Some(error) = minimax_base_resp_error(&payload) {
        anyhow::bail!("MiniMax music generation failed: {error}");
    }
    let data = payload.get("data").unwrap_or(&serde_json::Value::Null);
    if data.get("status").and_then(|value| value.as_i64()) == Some(1) {
        anyhow::bail!("MiniMax returned the song before it finished generating; try again");
    }
    let audio = data
        .get("audio")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("MiniMax finished without returning audio"))?;
    Ok(GeneratedMusic {
        job_id: payload
            .get("trace_id")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        bytes: decode_hex_string(audio)?,
        mime_type: DEFAULT_MUSIC_MIME_TYPE.to_string(),
        lyrics: None,
    })
}

async fn generate_gemini_lyria_music(
    client: &reqwest::Client,
    provider_id: &str,
    provider_config: &crate::agent::types::ProviderConfig,
    args: &serde_json::Value,
    prompt: &str,
    deadline: tokio::time::Instant,
) -> Result<GeneratedMusic> {
    let base = provider_config.base_url.trim().trim_end_matches('/');
    let body = build_gemini_lyria_body(args, &provider_config.model, prompt);
    let mut interaction = music_json_response(
        apply_media_auth_headers(
            client.post(format!("{base}/interactions")),
            provider_id,
            provider_config,
        )?
        .json(&body),
    )
    .await?;
    let interaction_id = interaction
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let (mime_type, data, lyrics) = loop {
        if let MusicJobPoll::Ready {
            mime_type,
            data,
            lyrics,
        } = parse_gemini_lyria_interaction(&interaction)?
        {
            break (mime_type, data, lyrics);
        }
        let Some(id) = interaction_id.as_deref() else {
            anyhow::bail!("Gemini music generation is still running but returned no id to poll");
        };
        if tokio::time::Instant::now() + MUSIC_POLL_INTERVAL > deadline {
            anyhow::bail!("music generation {id} did not finish before the timeout");
        }
        tokio::time::sleep(MUSIC_POLL_INTERVAL).await;
        interaction = music_json_response(apply_media_auth_headers(
            client.get(format!("{base}/interactions/{id}")),
            provider_id,
            provider_config,
        )?)
        .await?;
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|error| anyhow::anyhow!("invalid music base64 payload: {error}"))?;
    Ok(GeneratedMusic {
        job_id: interaction_id,
        bytes,
        mime_type,
        lyrics,
    })
}

async fn persist_generated_music_for_thread(
    agent: &AgentEngine,
    thread_id: &str,
    prompt: &str,
    result_json: &str,
    include_prompt_message: bool,
) -> Result<String> {
    let payload: serde_json::Value = serde_json::from_str(result_json)?;
    let provider = music_arg_str(&payload, "provider")
        .unwrap_or("unknown")
        .to_string();
    let model = music_arg_str(&payload, "model")
        .unwrap_or("unknown")
        .to_string();
    let mime_type = music_arg_str(&payload, "mime_type")
        .unwrap_or(DEFAULT_MUSIC_MIME_TYPE)
        .to_string();
    let lyrics = music_arg_str(&payload, "lyrics").map(str::to_string);
    let source = PathBuf::from(
        music_arg_str(&payload, "path")
            .ok_or_else(|| anyhow::anyhow!("generated music result has no path"))?,
    );
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| file_extension_for_generated_mime(&mime_type, "mp3"));
    let destination = thread_media_output_path(&agent.data_dir, thread_id, "music", &extension);
    move_generated_media_file(&source, &destination).await?;
    let file_url = file_url_for_path(&destination);

    {
        let mut threads = agent.threads.write().await;
        let Some(thread) = threads.get_mut(thread_id) else {
            anyhow::bail!("thread not found while persisting generated music");
        };
        if include_prompt_message {
            thread.messages.push(AgentMessage::user(
                format!("🎵 {}", prompt.trim()),
                now_millis(),
            ));
        }
        let content = match lyrics.as_deref() {
            Some(lyrics) => format!("Generated music.\n\n{lyrics}"),
            None => "Generated music.".to_string(),
        };
        let mut message = AgentMessage::user(content, now_millis());
        message.role = MessageRole::Assistant;
        message.content_blocks = vec![AgentContentBlock::Audio {
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
            tool_names::GENERATE_MUSIC,
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
        "lyrics": lyrics,
        "job_id": payload.get("job_id").and_then(|value| value.as_str()),
    }))
    .map_err(Into::into)
}

pub(crate) async fn execute_generate_music(
    args: &serde_json::Value,
    agent: &AgentEngine,
    _http_client: &reqwest::Client,
    thread_id: Option<&str>,
) -> Result<String> {
    let client = fresh_media_http_client(tool_names::GENERATE_MUSIC, args);
    let deadline =
        tokio::time::Instant::now() + media_tool_timeout(tool_names::GENERATE_MUSIC, args);
    let prompt = music_arg_str(args, "prompt")
        .ok_or_else(|| anyhow::anyhow!("missing 'prompt' argument"))?;
    let (provider_id, provider_config) =
        resolve_media_provider_config(agent, args, Some(MediaEndpoint::MusicGeneration)).await?;
    let route = music_generation_route(&provider_id).ok_or_else(|| {
        anyhow::anyhow!(
            "provider '{provider_id}' does not support music generation; use elevenlabs, gemini or minimax"
        )
    })?;
    if provider_config.model.trim().is_empty() {
        anyhow::bail!("no music generation model configured for provider '{provider_id}'; set music.generation.model");
    }

    let generated = match route {
        MusicGenerationRoute::ElevenLabs => {
            generate_elevenlabs_music(&client, &provider_id, &provider_config, args, prompt).await?
        }
        MusicGenerationRoute::MiniMax => {
            generate_minimax_music(&client, &provider_id, &provider_config, args, prompt).await?
        }
        MusicGenerationRoute::GeminiLyria => {
            generate_gemini_lyria_music(
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

    let extension = file_extension_for_generated_mime(&generated.mime_type, "mp3");
    let output_path = temp_output_path("music", &extension);
    tokio::fs::write(&output_path, &generated.bytes).await?;
    let result = serde_json::to_string_pretty(&serde_json::json!({
        "provider": provider_id,
        "model": provider_config.model,
        "path": output_path,
        "mime_type": generated.mime_type,
        "bytes": generated.bytes.len(),
        "job_id": generated.job_id,
        "lyrics": generated.lyrics,
    }))?;
    if let Some(thread_id) = thread_id {
        return persist_generated_music_for_thread(agent, thread_id, prompt, &result, false).await;
    }
    Ok(result)
}

pub(crate) async fn execute_generate_music_for_ipc(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let prompt = music_arg_str(args, "prompt")
        .ok_or_else(|| anyhow::anyhow!("missing 'prompt' argument"))?
        .to_string();
    let requested_thread_id = music_arg_str(args, "thread_id");
    let (thread_id, is_new_thread) = agent
        .get_or_create_thread(requested_thread_id, &prompt)
        .await;
    agent.ensure_thread_messages_loaded(&thread_id).await;

    let upstream_result = execute_generate_music(args, agent, &agent.http_client, None).await?;
    let persisted =
        persist_generated_music_for_thread(agent, &thread_id, &prompt, &upstream_result, true)
            .await?;
    agent
        .record_operator_message(&thread_id, &format!("🎵 {}", prompt), is_new_thread)
        .await?;
    Ok(persisted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    type SeenRequests = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

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

    async fn spawn_music_provider_mock(
        responses: Vec<(&'static str, &'static str, Vec<u8>)>,
    ) -> (String, SeenRequests) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: SeenRequests = Default::default();
        let recorded = seen.clone();
        tokio::spawn(async move {
            let mut responses = std::collections::VecDeque::from(responses);
            while let Ok((mut socket, _)) = listener.accept().await {
                let request = read_http_request(&mut socket).await;
                recorded.lock().unwrap().push(request);
                let (content_type, body) = responses
                    .pop_front()
                    .map(|(_, content_type, body)| (content_type, body))
                    .unwrap_or(("text/plain", b"unexpected request".to_vec()));
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(head.as_bytes()).await.ok();
                socket.write_all(&body).await.ok();
                socket.shutdown().await.ok();
            }
        });
        (base, seen)
    }

    async fn engine_with_music_provider(
        root: &std::path::Path,
        provider_id: &str,
        base_url: String,
        model: &str,
    ) -> std::sync::Arc<AgentEngine> {
        let manager = crate::session_manager::SessionManager::new_test(root).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4".to_string();
        config.api_key = "sk-chat".to_string();
        config.extra.insert(
            "music".to_string(),
            serde_json::json!({ "generation": { "provider": provider_id, "model": model } }),
        );
        let provider = crate::agent::types::ProviderConfig {
            base_url,
            model: "chat-model".to_string(),
            api_key: "sk-music".to_string(),
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
        };
        config.providers.insert(provider_id.to_string(), provider);
        AgentEngine::new_test(manager, config, root).await
    }

    async fn assert_thread_plays_generated_music(
        engine: &AgentEngine,
        thread_id: &str,
        result: &str,
        expected: &[u8],
    ) -> serde_json::Value {
        let result: serde_json::Value = serde_json::from_str(result).expect("result is JSON");
        let path = result["path"].as_str().expect("saved path");
        assert!(
            path.contains("/threads/") && path.ends_with(".mp3"),
            "music should be saved as an mp3 in the thread media dir, got {path}"
        );
        assert_eq!(tokio::fs::read(path).await.unwrap(), expected);
        let thread = engine
            .get_thread_filtered(thread_id, true, None, 0)
            .await
            .expect("thread");
        let last = thread.thread.messages.last().expect("message");
        assert_eq!(last.role, MessageRole::Assistant);
        assert!(
            matches!(
                last.content_blocks.first(),
                Some(AgentContentBlock::Audio { url: Some(url), mime_type: Some(mime), .. })
                    if url == &format!("file://{path}") && mime == "audio/mpeg"
            ),
            "the operator should get a playable audio block: {:?}",
            last.content_blocks
        );
        let context = engine
            .thread_work_contexts
            .read()
            .await
            .get(thread_id)
            .cloned()
            .expect("work context");
        assert_eq!(context.entries[0].source, tool_names::GENERATE_MUSIC);
        result
    }

    #[test]
    fn generate_music_is_offered_only_once_a_music_model_is_configured() {
        let root = tempfile::tempdir().unwrap();
        let mut config = AgentConfig::default();
        let offered = |config: &AgentConfig| {
            crate::agent::tool_executor::get_available_tools(config, root.path(), false)
                .iter()
                .any(|tool| tool.function.name == tool_names::GENERATE_MUSIC)
        };
        assert!(!offered(&config));
        config.extra.insert(
            "music".to_string(),
            serde_json::json!({ "generation": { "provider": "elevenlabs", "model": "music_v2" } }),
        );
        assert!(offered(&config));
    }

    #[test]
    fn music_defaults_to_each_providers_full_song_model_and_rejects_others() {
        assert_eq!(
            provider_specific_default_music_model(zorai_shared::providers::PROVIDER_ID_ELEVENLABS),
            Some("music_v2")
        );
        assert_eq!(
            provider_specific_default_music_model(zorai_shared::providers::PROVIDER_ID_GEMINI),
            Some("lyria-3.5")
        );
        assert_eq!(
            provider_specific_default_music_model(zorai_shared::providers::PROVIDER_ID_MINIMAX),
            Some("music-3.0")
        );
        assert_eq!(
            provider_specific_default_music_model(zorai_shared::providers::PROVIDER_ID_OPENROUTER),
            None
        );
    }

    #[test]
    fn elevenlabs_body_clamps_length_into_its_range_and_forces_instrumental() {
        let body = build_elevenlabs_music_body(
            &serde_json::json!({ "duration_seconds": 1200, "instrumental": true, "lyrics": "la la" }),
            "music_v2",
            "ambient pads",
        );
        assert_eq!(body["music_length_ms"], 600_000);
        assert_eq!(body["force_instrumental"], true);
        assert_eq!(body["model_id"], "music_v2");
        assert!(
            !body["prompt"].as_str().unwrap().contains("la la"),
            "lyrics make no sense for an instrumental"
        );

        let short = build_elevenlabs_music_body(
            &serde_json::json!({ "duration_seconds": 1 }),
            "music_v2",
            "jingle",
        );
        assert_eq!(short["music_length_ms"], 3_000);
        assert!(short.get("force_instrumental").is_none());
    }

    #[test]
    fn lyria_has_no_length_or_lyrics_fields_so_they_are_written_into_the_prompt() {
        let body = build_gemini_lyria_body(
            &serde_json::json!({ "duration_seconds": 90, "lyrics": "[Chorus] neon rain" }),
            "lyria-3.5",
            "synthwave ballad",
        );
        let input = body["input"].as_str().unwrap();
        assert!(input.starts_with("synthwave ballad"));
        assert!(input.contains("Lyrics:\n[Chorus] neon rain"));
        assert!(input.contains("about 90 seconds"));
        assert_eq!(body["response_format"]["type"], "audio");
    }

    #[test]
    fn lyria_interaction_waits_while_running_and_returns_audio_with_lyrics() {
        assert_eq!(
            parse_gemini_lyria_interaction(
                &serde_json::json!({ "id": "i1", "status": "in_progress" })
            )
            .unwrap(),
            MusicJobPoll::Pending
        );
        let ready = parse_gemini_lyria_interaction(&serde_json::json!({
            "status": "completed",
            "steps": [{ "type": "model_output", "content": [
                { "type": "text", "text": "[Verse] neon rain" },
                { "type": "audio", "data": "bXAz", "mime_type": "audio/mpeg" }
            ]}]
        }))
        .unwrap();
        assert_eq!(
            ready,
            MusicJobPoll::Ready {
                mime_type: "audio/mpeg".into(),
                data: "bXAz".into(),
                lyrics: Some("[Verse] neon rain".into())
            }
        );
        assert!(parse_gemini_lyria_interaction(
            &serde_json::json!({ "status": "completed", "steps": [] })
        )
        .is_err());
        assert!(parse_gemini_lyria_interaction(
            &serde_json::json!({ "error": { "message": "blocked" } })
        )
        .unwrap_err()
        .to_string()
        .contains("blocked"));
    }

    #[tokio::test]
    async fn elevenlabs_song_is_saved_into_the_thread_as_a_playable_track() {
        let root = tempfile::tempdir().unwrap();
        let (base, seen) =
            spawn_music_provider_mock(vec![("/v1/music", "audio/mpeg", b"eleven-mp3".to_vec())])
                .await;
        let engine = engine_with_music_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_ELEVENLABS,
            base,
            "music_v2",
        )
        .await;
        let (thread_id, _) = engine.get_or_create_thread(None, "lofi").await;

        let result = execute_generate_music(
            &serde_json::json!({ "prompt": "lofi hip hop", "duration_seconds": 45 }),
            &engine,
            &engine.http_client,
            Some(&thread_id),
        )
        .await
        .expect("music generation");

        assert_thread_plays_generated_music(&engine, &thread_id, &result, b"eleven-mp3").await;
        let request = seen.lock().unwrap()[0].clone();
        assert!(request.starts_with("POST /v1/music "), "{request}");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("xi-api-key: sk-music"),
            "{request}"
        );
        assert!(request.contains("\"music_length_ms\":45000"), "{request}");
    }

    #[tokio::test]
    async fn lyria_interaction_is_polled_until_done_and_its_lyrics_are_kept_with_the_track() {
        let root = tempfile::tempdir().unwrap();
        let done = serde_json::json!({
            "id": "int-1",
            "status": "completed",
            "steps": [{ "type": "model_output", "content": [
                { "type": "text", "text": "[Chorus] neon rain" },
                { "type": "audio", "data": base64::engine::general_purpose::STANDARD.encode(b"lyria-mp3"), "mime_type": "audio/mpeg" }
            ]}]
        });
        let (base, seen) = spawn_music_provider_mock(vec![
            (
                "/v1beta/interactions",
                "application/json",
                serde_json::json!({ "id": "int-1", "status": "in_progress" })
                    .to_string()
                    .into_bytes(),
            ),
            (
                "/v1beta/interactions/int-1",
                "application/json",
                done.to_string().into_bytes(),
            ),
        ])
        .await;
        let engine = engine_with_music_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_GEMINI,
            format!("{base}/v1beta"),
            "lyria-3.5",
        )
        .await;
        let (thread_id, _) = engine.get_or_create_thread(None, "synthwave").await;

        let result = execute_generate_music(
            &serde_json::json!({ "prompt": "synthwave" }),
            &engine,
            &engine.http_client,
            Some(&thread_id),
        )
        .await
        .expect("music generation");

        let result =
            assert_thread_plays_generated_music(&engine, &thread_id, &result, b"lyria-mp3").await;
        assert_eq!(result["lyrics"], "[Chorus] neon rain");
        let thread = engine
            .get_thread_filtered(&thread_id, true, None, 0)
            .await
            .unwrap();
        assert!(thread
            .thread
            .messages
            .last()
            .unwrap()
            .content
            .contains("[Chorus] neon rain"));
        let requests = seen.lock().unwrap().clone();
        assert!(requests[0].starts_with("POST /v1beta/interactions "));
        assert!(requests[1].starts_with("GET /v1beta/interactions/int-1 "));
    }

    #[test]
    fn minimax_writes_lyrics_itself_unless_given_lyrics_or_asked_for_an_instrumental() {
        let auto = build_minimax_music_body(&serde_json::json!({}), "music-3.0", "city pop");
        assert_eq!(auto["lyrics_optimizer"], true);
        assert_eq!(auto["is_instrumental"], false);
        assert_eq!(auto["output_format"], "hex");

        let given = build_minimax_music_body(
            &serde_json::json!({ "lyrics": "[Verse] hi" }),
            "music-3.0",
            "city pop",
        );
        assert_eq!(given["lyrics"], "[Verse] hi");
        assert!(given.get("lyrics_optimizer").is_none());

        let instrumental = build_minimax_music_body(
            &serde_json::json!({ "instrumental": true, "lyrics": "x" }),
            "music-3.0",
            "ambient",
        );
        assert_eq!(instrumental["is_instrumental"], true);
        assert!(
            instrumental.get("lyrics").is_none() && instrumental.get("lyrics_optimizer").is_none()
        );
    }

    #[tokio::test]
    async fn minimax_hex_song_is_decoded_and_saved_and_api_errors_are_reported() {
        let root = tempfile::tempdir().unwrap();
        let ok = serde_json::json!({
            "data": { "status": 2, "audio": "6d696e696d61782d6d7033" },
            "trace_id": "trace-1",
            "base_resp": { "status_code": 0, "status_msg": "success" }
        });
        let broke = serde_json::json!({ "base_resp": { "status_code": 1008, "status_msg": "insufficient balance" } });
        let (base, seen) = spawn_music_provider_mock(vec![
            (
                "/v1/music_generation",
                "application/json",
                ok.to_string().into_bytes(),
            ),
            (
                "/v1/music_generation",
                "application/json",
                broke.to_string().into_bytes(),
            ),
        ])
        .await;
        let engine = engine_with_music_provider(
            root.path(),
            zorai_shared::providers::PROVIDER_ID_MINIMAX,
            base,
            "music-3.0",
        )
        .await;
        let (thread_id, _) = engine.get_or_create_thread(None, "city pop").await;

        let result = execute_generate_music(
            &serde_json::json!({ "prompt": "city pop" }),
            &engine,
            &engine.http_client,
            Some(&thread_id),
        )
        .await
        .expect("music generation");
        assert_thread_plays_generated_music(&engine, &thread_id, &result, b"minimax-mp3").await;
        let request = seen.lock().unwrap()[0].clone();
        assert!(
            request.starts_with("POST /v1/music_generation "),
            "{request}"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer sk-music"),
            "{request}"
        );

        let error = execute_generate_music(
            &serde_json::json!({ "prompt": "city pop" }),
            &engine,
            &engine.http_client,
            Some(&thread_id),
        )
        .await
        .expect_err("balance error");
        assert!(
            error.to_string().contains("insufficient balance"),
            "{error}"
        );
    }
}
