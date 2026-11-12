use super::*;
use zorai_shared::providers::{
    PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_CUSTOM, PROVIDER_ID_ELEVENLABS, PROVIDER_ID_GEMINI,
    PROVIDER_ID_GROQ, PROVIDER_ID_MINIMAX, PROVIDER_ID_MINIMAX_CODING_PLAN, PROVIDER_ID_OPENAI,
    PROVIDER_ID_OPENROUTER, PROVIDER_ID_QWEN, PROVIDER_ID_XAI, PROVIDER_ID_XIAOMI_MIMO,
    PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
};
impl TuiModel {
    fn image_remote_model_fetch_output_modalities(provider_id: &str) -> Option<String> {
        if provider_id == PROVIDER_ID_OPENROUTER {
            Some("image".to_string())
        } else {
            None
        }
    }

    fn video_remote_model_fetch_output_modalities(provider_id: &str) -> Option<String> {
        if provider_id == PROVIDER_ID_OPENROUTER {
            Some("video".to_string())
        } else {
            None
        }
    }

    fn audio_remote_model_fetch_output_modalities(
        endpoint: &str,
        provider_id: &str,
    ) -> Option<String> {
        if provider_id != PROVIDER_ID_OPENROUTER {
            return None;
        }

        match endpoint {
            "tts" => Some("speech".to_string()),
            "stt" => Some("transcription".to_string()),
            _ => None,
        }
    }

    pub(crate) fn audio_catalog_models(
        endpoint: &str,
        provider_id: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let model = |id: &str, name: &str, context_window: Option<u32>| {
            crate::state::config::FetchedModel {
                id: id.to_string(),
                name: Some(name.to_string()),
                context_window,
                pricing: None,
                metadata: None,
            }
        };
        match (provider_id, endpoint) {
            (PROVIDER_ID_OPENAI | PROVIDER_ID_AZURE_OPENAI, "stt") => vec![
                model("gpt-4o-transcribe", "GPT-4o Transcribe", Some(128_000)),
                model(
                    "gpt-4o-mini-transcribe",
                    "GPT-4o Mini Transcribe",
                    Some(128_000),
                ),
                model(
                    "gpt-4o-transcribe-diarize",
                    "GPT-4o Transcribe Diarize",
                    Some(16_000),
                ),
                model("whisper-1", "Whisper 1", None),
            ],
            (PROVIDER_ID_GROQ, "stt") => vec![
                model("whisper-large-v3-turbo", "Whisper Large V3 Turbo", None),
                model("whisper-large-v3", "Whisper Large V3", None),
            ],
            (PROVIDER_ID_GROQ, "tts") => vec![
                model(
                    "canopylabs/orpheus-v1-english",
                    "CanopyLabs Orpheus V1 English",
                    None,
                ),
                model(
                    "canopylabs/orpheus-arabic-saudi",
                    "CanopyLabs Orpheus Arabic Saudi",
                    None,
                ),
            ],
            (PROVIDER_ID_OPENAI | PROVIDER_ID_AZURE_OPENAI, "tts") => vec![
                model("gpt-4o-mini-tts", "GPT-4o Mini TTS", Some(128_000)),
                model("tts-1", "TTS 1", None),
                model("tts-1-hd", "TTS 1 HD", None),
            ],
            (PROVIDER_ID_XAI, "stt" | "tts") => {
                vec![model("grok-4.3", "Grok 4.3", Some(1_000_000))]
            }
            (PROVIDER_ID_ELEVENLABS, "stt") => vec![model("scribe_v2", "Scribe v2", None)],
            (PROVIDER_ID_ELEVENLABS, "tts") => vec![model(
                "eleven_multilingual_v2",
                "Eleven Multilingual v2",
                None,
            )],
            (PROVIDER_ID_XIAOMI_MIMO | PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN, "stt") => {
                vec![model("mimo-v2.5-asr", "MiMo V2.5 ASR", Some(128_000))]
            }
            (PROVIDER_ID_XIAOMI_MIMO | PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN, "tts") => vec![
                model("mimo-v2-tts", "MiMo V2 TTS", Some(128_000)),
                model("mimo-v2.5-tts", "MiMo V2.5 TTS", Some(128_000)),
                model(
                    "mimo-v2.5-tts-voiceclone",
                    "MiMo V2.5 TTS VoiceClone",
                    Some(128_000),
                ),
                model(
                    "mimo-v2.5-tts-voicedesign",
                    "MiMo V2.5 TTS VoiceDesign",
                    Some(128_000),
                ),
            ],
            (PROVIDER_ID_MINIMAX | PROVIDER_ID_MINIMAX_CODING_PLAN, "tts") => vec![
                model("speech-2.8-hd", "MiniMax Speech 2.8 HD", None),
                model("speech-2.8-turbo", "MiniMax Speech 2.8 Turbo", None),
                model("speech-2.6-hd", "MiniMax Speech 2.6 HD", None),
                model("speech-2.6-turbo", "MiniMax Speech 2.6 Turbo", None),
                model("speech-02-hd", "MiniMax Speech 02 HD", None),
                model("speech-02-turbo", "MiniMax Speech 02 Turbo", None),
                model("speech-01-hd", "MiniMax Speech 01 HD", None),
                model("speech-01-turbo", "MiniMax Speech 01 Turbo", None),
            ],
            _ => Vec::new(),
        }
    }

    pub(crate) fn default_audio_model_for(endpoint: &str, provider_id: &str) -> String {
        Self::audio_catalog_models(endpoint, provider_id)
            .into_iter()
            .next()
            .map(|model| model.id)
            .unwrap_or_default()
    }

    pub(crate) fn image_generation_catalog_models(
        provider_id: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let model = |id: &str, name: &str, context_window: Option<u32>| {
            crate::state::config::FetchedModel {
                id: id.to_string(),
                name: Some(name.to_string()),
                context_window,
                pricing: None,
                metadata: None,
            }
        };
        match provider_id {
            PROVIDER_ID_OPENAI | PROVIDER_ID_AZURE_OPENAI | PROVIDER_ID_CUSTOM => {
                vec![
                    model("gpt-image-1", "GPT Image 1", None),
                    model("gpt-image-2", "GPT Image 2", None),
                ]
            }
            PROVIDER_ID_OPENROUTER => {
                vec![
                    model("openai/gpt-image-1", "OpenAI GPT Image 1", None),
                    model("openai/gpt-image-2", "OpenAI GPT Image 2", None),
                ]
            }
            PROVIDER_ID_MINIMAX | PROVIDER_ID_MINIMAX_CODING_PLAN => {
                vec![model("image-01", "MiniMax Image 01", None)]
            }
            _ => Vec::new(),
        }
    }

    pub(crate) fn default_image_generation_model_for(provider_id: &str) -> String {
        Self::image_generation_catalog_models(provider_id)
            .into_iter()
            .next()
            .map(|model| model.id)
            .unwrap_or_default()
    }

    pub(crate) fn video_generation_catalog_models(
        provider_id: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let model = |id: &str, name: &str| crate::state::config::FetchedModel {
            id: id.to_string(),
            name: Some(name.to_string()),
            context_window: None,
            pricing: None,
            metadata: None,
        };
        match provider_id {
            PROVIDER_ID_MINIMAX => vec![
                model("MiniMax-Hailuo-2.3", "MiniMax Hailuo 2.3"),
                model("MiniMax-Hailuo-02", "MiniMax Hailuo 02"),
                model("T2V-01-Director", "MiniMax T2V 01 Director"),
                model("T2V-01", "MiniMax T2V 01"),
            ],
            PROVIDER_ID_QWEN => vec![
                model("wan2.7-t2v", "Wan 2.7 T2V"),
                model("wan2.6-t2v", "Wan 2.6 T2V"),
                model("wan2.5-t2v-preview", "Wan 2.5 T2V Preview"),
                model("wan2.2-t2v-plus", "Wan 2.2 T2V Plus"),
            ],
            PROVIDER_ID_GEMINI => vec![
                model("gemini-omni-1.2-flash", "Gemini Omni 1.2 Flash"),
                model("gemini-omni-1.1-flash", "Gemini Omni 1.1 Flash"),
            ],
            _ => Vec::new(),
        }
    }

    pub(crate) fn default_video_generation_model_for(provider_id: &str) -> String {
        Self::video_generation_catalog_models(provider_id)
            .into_iter()
            .next()
            .map(|model| model.id)
            .unwrap_or_default()
    }

    pub(crate) fn music_generation_catalog_models(
        provider_id: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let model = |id: &str, name: &str| crate::state::config::FetchedModel {
            id: id.to_string(),
            name: Some(name.to_string()),
            context_window: None,
            pricing: None,
            metadata: None,
        };
        match provider_id {
            PROVIDER_ID_ELEVENLABS => vec![
                model("music_v2", "ElevenLabs Music v2"),
                model("music_v2_5", "ElevenLabs Music v2.5"),
                model("music_v1", "ElevenLabs Music v1"),
            ],
            PROVIDER_ID_GEMINI => vec![
                model("lyria-3.5", "Lyria 3.5 (full songs)"),
                model("lyria-3-clip-preview", "Lyria 3 Clip (30 s)"),
                model("lyria-3-pro-preview", "Lyria 3 Pro (full songs)"),
            ],
            PROVIDER_ID_MINIMAX | PROVIDER_ID_MINIMAX_CODING_PLAN => vec![
                model("music-3.0", "MiniMax Music 3.0"),
                model("music-2.6", "MiniMax Music 2.6"),
                model("music-3.0-free", "MiniMax Music 3.0 (free tier)"),
                model("music-2.6-free", "MiniMax Music 2.6 (free tier)"),
            ],
            _ => Vec::new(),
        }
    }

    pub(crate) fn default_music_generation_model_for(provider_id: &str) -> String {
        Self::music_generation_catalog_models(provider_id)
            .into_iter()
            .next()
            .map(|model| model.id)
            .unwrap_or_default()
    }

    pub(crate) fn embedding_catalog_models(
        provider_id: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let model = |id: &str, name: &str, context_window: Option<u32>| {
            crate::state::config::FetchedModel {
                id: id.to_string(),
                name: Some(name.to_string()),
                context_window,
                pricing: None,
                metadata: None,
            }
        };
        match provider_id {
            PROVIDER_ID_OPENAI | PROVIDER_ID_AZURE_OPENAI | PROVIDER_ID_CUSTOM => {
                vec![
                    model(
                        "text-embedding-3-small",
                        "Text Embedding 3 Small",
                        Some(8192),
                    ),
                    model(
                        "text-embedding-3-large",
                        "Text Embedding 3 Large",
                        Some(8192),
                    ),
                ]
            }
            PROVIDER_ID_OPENROUTER => {
                vec![
                    model(
                        "openai/text-embedding-3-small",
                        "OpenAI Text Embedding 3 Small",
                        Some(8192),
                    ),
                    model(
                        "openai/text-embedding-3-large",
                        "OpenAI Text Embedding 3 Large",
                        Some(8192),
                    ),
                ]
            }
            _ => Vec::new(),
        }
    }

    pub(crate) fn default_embedding_model_for(provider_id: &str) -> String {
        Self::embedding_catalog_models(provider_id)
            .into_iter()
            .next()
            .map(|model| model.id)
            .unwrap_or_default()
    }

    pub(crate) fn set_audio_config_string(&mut self, endpoint: &str, field: &str, value: String) {
        self.send_daemon_command(DaemonCommand::SetConfigItem {
            key_path: format!("/audio/{endpoint}/{field}"),
            value_json: serde_json::Value::String(value.clone()).to_string(),
        });
        if let Some(ref mut raw) = self.config.agent_config_raw {
            if raw.get("audio").is_none() {
                raw["audio"] = serde_json::json!({});
            }
            if raw["audio"].get(endpoint).is_none() {
                raw["audio"][endpoint] = serde_json::json!({});
            }
            raw["audio"][endpoint][field] = serde_json::Value::String(value);
        }
    }

    pub(crate) fn set_image_generation_config_string(&mut self, field: &str, value: String) {
        self.send_daemon_command(DaemonCommand::SetConfigItem {
            key_path: format!("/image/generation/{field}"),
            value_json: serde_json::Value::String(value.clone()).to_string(),
        });
        if let Some(ref mut raw) = self.config.agent_config_raw {
            if raw.get("image").is_none() {
                raw["image"] = serde_json::json!({});
            }
            if raw["image"].get("generation").is_none() {
                raw["image"]["generation"] = serde_json::json!({});
            }
            raw["image"]["generation"][field] = serde_json::Value::String(value);
        }
    }

    pub(crate) fn set_video_generation_config_string(&mut self, field: &str, value: String) {
        self.send_daemon_command(DaemonCommand::SetConfigItem {
            key_path: format!("/video/generation/{field}"),
            value_json: serde_json::Value::String(value.clone()).to_string(),
        });
        if let Some(ref mut raw) = self.config.agent_config_raw {
            if raw.get("video").is_none() {
                raw["video"] = serde_json::json!({});
            }
            if raw["video"].get("generation").is_none() {
                raw["video"]["generation"] = serde_json::json!({});
            }
            raw["video"]["generation"][field] = serde_json::Value::String(value);
        }
    }

    pub(crate) fn set_music_generation_config_string(&mut self, field: &str, value: String) {
        self.send_daemon_command(DaemonCommand::SetConfigItem {
            key_path: format!("/music/generation/{field}"),
            value_json: serde_json::Value::String(value.clone()).to_string(),
        });
        if let Some(ref mut raw) = self.config.agent_config_raw {
            if raw.get("music").is_none() {
                raw["music"] = serde_json::json!({});
            }
            if raw["music"].get("generation").is_none() {
                raw["music"]["generation"] = serde_json::json!({});
            }
            raw["music"]["generation"][field] = serde_json::Value::String(value);
        }
    }

    pub(crate) fn set_embedding_config_string(&mut self, field: &str, value: String) {
        self.send_daemon_command(DaemonCommand::SetConfigItem {
            key_path: format!("/semantic/embedding/{field}"),
            value_json: serde_json::Value::String(value.clone()).to_string(),
        });
        if let Some(ref mut raw) = self.config.agent_config_raw {
            if raw.get("semantic").is_none() {
                raw["semantic"] = serde_json::json!({});
            }
            if raw["semantic"].get("embedding").is_none() {
                raw["semantic"]["embedding"] = serde_json::json!({});
            }
            raw["semantic"]["embedding"][field] = serde_json::Value::String(value);
        }
    }

    pub(crate) fn open_audio_model_picker(&mut self, endpoint: &str) {
        let provider_id = match endpoint {
            "stt" => {
                let provider = self.config.audio_stt_provider();
                if provider.trim().is_empty() {
                    "openai".to_string()
                } else {
                    provider
                }
            }
            "tts" => {
                let provider = self.config.audio_tts_provider();
                if provider.trim().is_empty() {
                    "openai".to_string()
                } else {
                    provider
                }
            }
            _ => return,
        };
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        let models = Self::audio_catalog_models(endpoint, &provider_id);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            let output_modalities =
                Self::audio_remote_model_fetch_output_modalities(endpoint, &provider_id);
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities,
            });
        }
        self.settings_picker_target = Some(match endpoint {
            "stt" => SettingsPickerTarget::AudioSttModel,
            "tts" => SettingsPickerTarget::AudioTtsModel,
            _ => return,
        });
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    pub(crate) fn open_image_generation_model_picker(&mut self) {
        let provider_id = {
            let provider = self.config.image_generation_provider();
            if provider.trim().is_empty() {
                "openai".to_string()
            } else {
                provider
            }
        };
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        let models = Self::image_generation_catalog_models(&provider_id);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            let output_modalities = Self::image_remote_model_fetch_output_modalities(&provider_id);
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities,
            });
        }
        self.settings_picker_target = Some(SettingsPickerTarget::ImageGenerationModel);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    pub(crate) fn open_music_generation_model_picker(&mut self) {
        let provider_id = {
            let provider = self.config.music_generation_provider();
            if provider.trim().is_empty() {
                PROVIDER_ID_ELEVENLABS.to_string()
            } else {
                provider
            }
        };
        let models = Self::music_generation_catalog_models(&provider_id);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        self.settings_picker_target = Some(SettingsPickerTarget::MusicGenerationModel);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    pub(crate) fn open_video_generation_model_picker(&mut self) {
        let provider_id = {
            let provider = self.config.video_generation_provider();
            if provider.trim().is_empty() {
                PROVIDER_ID_MINIMAX.to_string()
            } else {
                provider
            }
        };
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        let models = Self::video_generation_catalog_models(&provider_id);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            let output_modalities = Self::video_remote_model_fetch_output_modalities(&provider_id);
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities,
            });
        }
        self.settings_picker_target = Some(SettingsPickerTarget::VideoGenerationModel);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    pub(crate) fn open_embedding_model_picker(&mut self) {
        let provider_id = {
            let provider = self.config.semantic_embedding_provider();
            if provider.trim().is_empty() {
                "openai".to_string()
            } else {
                provider
            }
        };
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        let models = Self::embedding_catalog_models(&provider_id);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities: Some("embedding".to_string()),
            });
        }
        self.settings_picker_target = Some(SettingsPickerTarget::EmbeddingModel);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    pub(crate) fn open_provider_backed_model_picker(
        &mut self,
        target: SettingsPickerTarget,
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    ) {
        let models = providers::known_models_for_provider_auth(&provider_id, &auth_source);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        self.model_picker_provider_id = Some(provider_id.clone());
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities: None,
            });
        }
        self.settings_picker_target = Some(target);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    fn json_array_contains_audio(value: Option<&serde_json::Value>) -> bool {
        Self::json_array_contains_modality(value, "audio")
    }

    fn json_array_contains_modality(value: Option<&serde_json::Value>, modality: &str) -> bool {
        value
            .and_then(|value| value.as_array())
            .map(|items| {
                items.iter().any(|item| {
                    item.as_str()
                        .map(str::trim)
                        .map(|value| value.eq_ignore_ascii_case(modality))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }

    fn modality_side_has_audio(modality: &str, side: &str) -> bool {
        let trimmed = modality.trim().to_ascii_lowercase();
        if trimmed.is_empty() {
            return false;
        }

        let Some((input, output)) = trimmed.split_once("->") else {
            return false;
        };
        let directional = match side {
            "input" => input,
            "output" => output,
            _ => return false,
        };

        directional
            .split(|ch: char| matches!(ch, '+' | ',' | '|' | '/' | ' '))
            .any(|token| token.trim() == "audio")
    }

    fn json_string_has_directional_audio(value: Option<&serde_json::Value>, side: &str) -> bool {
        value
            .and_then(|value| value.as_str())
            .map(|value| Self::modality_side_has_audio(value, side))
            .unwrap_or(false)
    }

    fn fetched_model_audio_direction_override(
        model: &crate::state::config::FetchedModel,
        endpoint: &str,
    ) -> Option<bool> {
        let provider_prefix_sensitive = model.id.starts_with("xai/")
            || model.id.starts_with("openai/")
            || model.id.starts_with(&format!("{PROVIDER_ID_XAI}/"))
            || model.id.starts_with(&format!("{PROVIDER_ID_OPENROUTER}/"));
        let name = model
            .name
            .as_deref()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let id = model.id.to_ascii_lowercase();
        let haystack = format!("{id} {name}");

        let looks_like_stt = haystack.contains("transcribe")
            || haystack.contains("transcription")
            || haystack.contains("speech-to-text")
            || haystack.contains("speech to text")
            || haystack.contains("whisper")
            || (provider_prefix_sensitive && haystack.contains("listen"));
        let looks_like_tts = haystack.contains("text-to-speech")
            || haystack.contains("text to speech")
            || haystack.contains("-tts")
            || haystack.contains(" tts")
            || (provider_prefix_sensitive && haystack.contains("speak"));

        match endpoint {
            "stt" if looks_like_stt && !looks_like_tts => Some(true),
            "stt" if looks_like_tts && !looks_like_stt => Some(false),
            "tts" if looks_like_tts && !looks_like_stt => Some(true),
            "tts" if looks_like_stt && !looks_like_tts => Some(false),
            _ => None,
        }
    }

    pub(crate) fn fetched_model_supports_audio_endpoint(
        model: &crate::state::config::FetchedModel,
        endpoint: &str,
    ) -> bool {
        let metadata = model.metadata.as_ref();
        let input_audio = Self::json_array_contains_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/input_modalities"))
                .or_else(|| metadata.and_then(|value| value.pointer("/input_modalities"))),
        );
        let output_audio = Self::json_array_contains_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/output_modalities"))
                .or_else(|| metadata.and_then(|value| value.pointer("/output_modalities"))),
        );
        let output_speech = Self::json_array_contains_modality(
            metadata
                .and_then(|value| value.pointer("/architecture/output_modalities"))
                .or_else(|| metadata.and_then(|value| value.pointer("/output_modalities"))),
            "speech",
        );
        let output_transcription = Self::json_array_contains_modality(
            metadata
                .and_then(|value| value.pointer("/architecture/output_modalities"))
                .or_else(|| metadata.and_then(|value| value.pointer("/output_modalities"))),
            "transcription",
        );
        let modality_input_audio = Self::json_string_has_directional_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/modality"))
                .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
            "input",
        );
        let modality_output_audio = Self::json_string_has_directional_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/modality"))
                .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
            "output",
        );

        let directional_match = match endpoint {
            "stt" => input_audio || modality_input_audio || output_transcription,
            "tts" => output_audio || modality_output_audio || output_speech,
            _ => false,
        };
        if directional_match {
            return true;
        }

        if let Some(override_result) = Self::fetched_model_audio_direction_override(model, endpoint)
        {
            return override_result;
        }

        false
    }
}
