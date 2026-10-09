use crate::providers;
use zorai_shared::providers::PROVIDER_ID_OPENAI;

use super::json_u32_to_from_config::{
    embedding_dimensions_from_fetched_model, provider_supports_audio, provider_supports_embeddings,
    provider_supports_image_generation,
};
use super::{ConfigAction, ConfigState, FetchedModel};

impl ConfigState {
    pub fn new() -> Self {
        Self {
            provider: PROVIDER_ID_OPENAI.to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-5.5".to_string(),
            custom_model_name: String::new(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: "api_key".to_string(),
            api_transport: "responses".to_string(),
            reasoning_effort: String::new(),
            claude_permission_mode: String::new(),
            openrouter_provider_order: String::new(),
            openrouter_provider_ignore: String::new(),
            openrouter_allow_fallbacks: true,
            openrouter_response_cache_enabled: false,
            huggingface_provider: String::new(),
            openrouter_endpoint_providers: Vec::new(),
            custom_context_window_tokens: None,
            chatgpt_auth_available: false,
            db_backend: String::new(),
            db_sync_url: String::new(),
            db_has_token: false,
            db_seeded_at: None,
            chatgpt_auth_source: None,
            fetched_models: Vec::new(),
            agent_config_raw: None,
            tool_bash: true,
            tool_file_ops: true,
            tool_web_search: true,
            tool_web_browse: false,
            tool_vision: false,
            tool_system_info: true,
            tool_gateway: false,
            search_provider: "none".to_string(),
            duckduckgo_region: "us-en".to_string(),
            duckduckgo_safe_search: "moderate".to_string(),
            firecrawl_api_key: String::new(),
            exa_api_key: String::new(),
            tavily_api_key: String::new(),
            search_max_results: 8,
            search_timeout_secs: 20,
            browse_provider: "auto".to_string(),
            custom_modalities: "text".to_string(),
            gateway_enabled: false,
            gateway_default_agent: "rarog".to_string(),
            gateway_prefix: "!zorai".to_string(),
            slack_token: String::new(),
            slack_channel_filter: String::new(),
            telegram_token: String::new(),
            telegram_allowed_chats: String::new(),
            discord_token: String::new(),
            discord_channel_filter: String::new(),
            discord_allowed_users: String::new(),
            whatsapp_allowed_contacts: String::new(),
            whatsapp_token: String::new(),
            whatsapp_phone_id: String::new(),
            enable_streaming: true,
            enable_conversation_memory: true,
            enable_honcho_memory: false,
            honcho_api_key: String::new(),
            honcho_base_url: String::new(),
            honcho_workspace_id: "zorai".to_string(),
            honcho_editor: None,
            mlflow_enabled: false,
            mlflow_tracking_uri: "http://127.0.0.1:5000".to_string(),
            mlflow_experiment_name: "zorai-conversations".to_string(),
            mlflow_experiment_id: String::new(),
            mlflow_capture_mode: "guarded".to_string(),
            mlflow_visible_operator: true,
            mlflow_gateway: true,
            mlflow_goal_task: true,
            mlflow_subagent: true,
            mlflow_concierge: true,
            mlflow_heartbeat_autonomous: false,
            mlflow_runtime_status: None,
            mlflow_test_result: None,
            anticipatory_enabled: true,
            anticipatory_morning_brief: true,
            anticipatory_predictive_hydration: true,
            anticipatory_stuck_detection: true,
            operator_model_enabled: true,
            operator_model_allow_message_statistics: true,
            operator_model_allow_approval_learning: true,
            operator_model_allow_attention_tracking: true,
            operator_model_allow_implicit_feedback: true,
            collaboration_enabled: true,
            compliance_mode: "standard".to_string(),
            compliance_retention_days: 30,
            compliance_sign_all_events: true,
            tool_synthesis_enabled: true,
            tool_synthesis_require_activation: true,
            tool_synthesis_max_generated_tools: 24,
            managed_sandbox_enabled: false,
            managed_security_level: "lowest".to_string(),
            auto_compact_context: true,
            max_context_messages: 100,
            tui_chat_history_page_size: 100,
            participant_observer_restore_window_hours: 24,
            max_tool_loops: 25,
            max_retries: 3,
            auto_refresh_interval_secs: 300,
            retry_delay_ms: 5_000,
            message_loop_delay_ms: 100,
            tool_call_delay_ms: 100,
            llm_stream_chunk_timeout_secs: 300,
            auto_retry: true,
            context_window_tokens: 128_000,
            compact_threshold_pct: 80,
            keep_recent_on_compact: 10,
            bash_timeout_secs: 30,
            weles_max_concurrent_reviews: 2,
            compaction_strategy: "heuristic".to_string(),
            compaction_weles_provider: PROVIDER_ID_OPENAI.to_string(),
            compaction_weles_model: "gpt-5.4-mini".to_string(),
            compaction_weles_reasoning_effort: "medium".to_string(),
            compaction_weles_api_transport: String::new(),
            compaction_custom_provider: PROVIDER_ID_OPENAI.to_string(),
            compaction_custom_base_url: "https://api.openai.com/v1".to_string(),
            compaction_custom_model: "gpt-5.4-mini".to_string(),
            compaction_custom_api_key: String::new(),
            compaction_custom_assistant_id: String::new(),
            compaction_custom_auth_source: "api_key".to_string(),
            compaction_custom_api_transport: "responses".to_string(),
            compaction_custom_reasoning_effort: "high".to_string(),
            compaction_custom_context_window_tokens: 128_000,
            snapshot_max_count: 0,
            snapshot_max_size_mb: 10_240,
            snapshot_auto_cleanup: false,
            snapshot_count: 0,
            snapshot_total_size_bytes: 0,
            workspace_repo_monitor_enabled: false,
            workspace_repo_monitor_include_dirs: String::new(),
            workspace_repo_monitor_exclude_dirs: String::new(),
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn reasoning_effort(&self) -> &str {
        &self.reasoning_effort
    }

    pub fn api_transport(&self) -> &str {
        &self.api_transport
    }

    pub fn fetched_models(&self) -> &[FetchedModel] {
        &self.fetched_models
    }

    pub fn agent_config_raw(&self) -> Option<&serde_json::Value> {
        self.agent_config_raw.as_ref()
    }

    pub fn reduce(&mut self, action: ConfigAction) {
        match action {
            ConfigAction::ConfigReceived(snapshot) => {
                self.provider = snapshot.provider;
                self.base_url = snapshot.base_url;
                self.model = snapshot.model;
                self.custom_model_name.clear();
                self.api_key = snapshot.api_key;
                self.assistant_id = snapshot.assistant_id;
                self.auth_source = snapshot.auth_source;
                self.api_transport = snapshot.api_transport;
                self.reasoning_effort = snapshot.reasoning_effort;
                self.context_window_tokens = snapshot.context_window_tokens;
            }

            ConfigAction::ConfigRawReceived(raw) => {
                self.agent_config_raw = Some(raw);
            }

            ConfigAction::ModelsFetched(models) => {
                self.fetched_models = models;
            }

            ConfigAction::SetProvider(provider) => {
                if let Some(def) = providers::find_by_id(&provider) {
                    self.base_url = def.default_base_url.to_string();
                    self.model = def.default_model.to_string();
                    self.custom_model_name = String::new();
                    self.api_transport = providers::default_transport_for(&provider).to_string();
                    self.auth_source = providers::default_auth_source_for(&provider).to_string();
                }
                self.provider = provider;
            }

            ConfigAction::SetModel(model) => {
                self.model = model;
            }

            ConfigAction::SetReasoningEffort(effort) => {
                self.reasoning_effort = effort;
            }
            ConfigAction::ToggleTool(name) => match name.as_str() {
                "bash" => self.tool_bash = !self.tool_bash,
                "file_ops" => self.tool_file_ops = !self.tool_file_ops,
                zorai_protocol::tool_names::WEB_SEARCH => {
                    self.tool_web_search = !self.tool_web_search
                }
                "web_browse" => self.tool_web_browse = !self.tool_web_browse,
                "vision" => self.tool_vision = !self.tool_vision,
                "system_info" => self.tool_system_info = !self.tool_system_info,
                "gateway" => self.tool_gateway = !self.tool_gateway,
                _ => {}
            },
        }
    }

    fn get_audio_nested_field(&self, group: &str, field: &str) -> Option<&serde_json::Value> {
        self.agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get(group))
            .and_then(|group| group.get(field))
    }

    fn get_audio_field(
        &self,
        group: &str,
        nested_field: &str,
        legacy_flat_key: &str,
    ) -> Option<&serde_json::Value> {
        self.get_audio_nested_field(group, nested_field)
            .or_else(|| {
                self.agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get(legacy_flat_key))
            })
            .or_else(|| {
                self.agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get("extra"))
                    .and_then(|extra| extra.get(legacy_flat_key))
            })
    }

    fn get_image_generation_nested_field(&self, field: &str) -> Option<&serde_json::Value> {
        self.agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("image"))
            .and_then(|image| image.get("generation"))
            .and_then(|generation| generation.get(field))
    }

    fn get_image_generation_field(
        &self,
        field: &str,
        legacy_flat_key: &str,
    ) -> Option<&serde_json::Value> {
        self.get_image_generation_nested_field(field)
            .or_else(|| {
                self.agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get(legacy_flat_key))
            })
            .or_else(|| {
                self.agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get("extra"))
                    .and_then(|extra| extra.get(legacy_flat_key))
            })
    }

    fn get_audio_bool(&self, group: &str, nested_field: &str, legacy_flat_key: &str) -> bool {
        self.get_audio_field(group, nested_field, legacy_flat_key)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn get_audio_string(&self, group: &str, nested_field: &str, legacy_flat_key: &str) -> String {
        self.get_audio_field(group, nested_field, legacy_flat_key)
            .and_then(|v| v.as_str())
            .or_else(|| {
                (nested_field == "provider" && provider_supports_audio(&self.provider, group))
                    .then_some(self.provider.as_str())
            })
            .unwrap_or("")
            .to_string()
    }

    pub fn audio_stt_enabled(&self) -> bool {
        self.get_audio_bool("stt", "enabled", "audio_stt_enabled")
    }

    pub fn audio_stt_provider(&self) -> String {
        self.get_audio_string("stt", "provider", "audio_stt_provider")
    }

    pub fn audio_stt_model(&self) -> String {
        self.get_audio_string("stt", "model", "audio_stt_model")
    }

    pub fn audio_tts_enabled(&self) -> bool {
        self.get_audio_bool("tts", "enabled", "audio_tts_enabled")
    }

    pub fn audio_tts_provider(&self) -> String {
        self.get_audio_string("tts", "provider", "audio_tts_provider")
    }

    pub fn audio_tts_model(&self) -> String {
        self.get_audio_string("tts", "model", "audio_tts_model")
    }

    pub fn audio_tts_voice(&self) -> String {
        self.get_audio_string("tts", "voice", "audio_tts_voice")
    }

    pub fn image_generation_provider(&self) -> String {
        self.get_image_generation_field("provider", "image_generation_provider")
            .and_then(|value| value.as_str())
            .or_else(|| {
                provider_supports_image_generation(&self.provider).then_some(self.provider.as_str())
            })
            .unwrap_or("")
            .to_string()
    }

    pub fn image_generation_model(&self) -> String {
        self.get_image_generation_field("model", "image_generation_model")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    }

    fn get_semantic_embedding_nested_field(&self, field: &str) -> Option<&serde_json::Value> {
        self.agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("semantic"))
            .and_then(|semantic| semantic.get("embedding"))
            .and_then(|embedding| embedding.get(field))
    }

    pub fn semantic_embedding_enabled(&self) -> bool {
        self.get_semantic_embedding_nested_field("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }

    pub fn semantic_embedding_provider(&self) -> String {
        self.get_semantic_embedding_nested_field("provider")
            .and_then(|value| value.as_str())
            .or_else(|| {
                provider_supports_embeddings(&self.provider).then_some(self.provider.as_str())
            })
            .unwrap_or("")
            .to_string()
    }

    pub fn semantic_embedding_model(&self) -> String {
        self.get_semantic_embedding_nested_field("model")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    }

    pub fn semantic_embedding_dimensions(&self) -> u32 {
        let model_id = self.semantic_embedding_model();
        if let Some(dimensions) = self
            .fetched_models
            .iter()
            .find(|model| model.id == model_id)
            .and_then(embedding_dimensions_from_fetched_model)
        {
            return dimensions;
        }

        self.get_semantic_embedding_nested_field("dimensions")
            .and_then(|value| value.as_u64())
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(1536)
    }
}

impl Default for ConfigState {
    fn default() -> Self {
        Self::new()
    }
}
