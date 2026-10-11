#[derive(Debug, Clone, Default)]
pub struct FetchedModelPricing {
    pub prompt: Option<String>,
    pub completion: Option<String>,
    pub image: Option<String>,
    pub request: Option<String>,
    pub web_search: Option<String>,
    pub internal_reasoning: Option<String>,
    pub input_cache_read: Option<String>,
    pub input_cache_write: Option<String>,
    pub audio: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FetchedModel {
    pub id: String,
    pub name: Option<String>,
    pub context_window: Option<u32>,
    pub pricing: Option<FetchedModelPricing>,
    pub metadata: Option<serde_json::Value>,
}

pub(super) fn json_u32(value: &serde_json::Value) -> Option<u32> {
    match value {
        serde_json::Value::Number(number) => number.as_u64().and_then(|n| u32::try_from(n).ok()),
        serde_json::Value::String(text) => text.trim().parse::<u32>().ok(),
        _ => None,
    }
    .filter(|value| *value > 0)
}

pub(super) fn setting_name_matches(value: &serde_json::Value) -> bool {
    value
        .as_str()
        .map(|text| {
            matches!(
                text.trim().to_ascii_lowercase().as_str(),
                "dimensions"
                    | "dimension"
                    | "embedding_dimensions"
                    | "embedding_dimension"
                    | "output_dimensions"
                    | "vector_dimensions"
            )
        })
        .unwrap_or(false)
}

pub(super) fn provider_supports_audio(provider: &str, group: &str) -> bool {
    match group {
        "stt" | "tts" => {
            provider == zorai_shared::providers::PROVIDER_ID_OPENROUTER
                || provider == zorai_shared::providers::PROVIDER_ID_ELEVENLABS
        }
        _ => false,
    }
}

pub(super) fn provider_supports_image_generation(provider: &str) -> bool {
    provider == "openrouter"
}

pub(super) fn provider_supports_embeddings(provider: &str) -> bool {
    provider == "openrouter"
}

pub(super) fn dimensions_from_settings_array(settings: &[serde_json::Value]) -> Option<u32> {
    settings.iter().find_map(|setting| {
        let object = setting.as_object()?;
        let name_matches = ["id", "key", "name", "param", "parameter"]
            .iter()
            .any(|key| object.get(*key).is_some_and(setting_name_matches));
        if !name_matches {
            return None;
        }

        ["value", "default", "default_value", "current"]
            .iter()
            .find_map(|key| object.get(*key).and_then(json_u32))
    })
}

pub(crate) fn embedding_dimensions_from_fetched_model(model: &FetchedModel) -> Option<u32> {
    let metadata = model.metadata.as_ref()?;
    [
        "/settings/dimensions",
        "/settings/dimension",
        "/settings/embedding_dimensions",
        "/settings/embedding_dimension",
        "/settings/output_dimensions",
        "/settings/vector_dimensions",
        "/dimensions",
        "/dimension",
        "/embedding_dimensions",
        "/embedding_dimension",
        "/output_dimensions",
        "/vector_dimensions",
        "/architecture/dimensions",
        "/architecture/embedding_dimensions",
        "/top_provider/dimensions",
    ]
    .iter()
    .find_map(|path| metadata.pointer(path).and_then(json_u32))
    .or_else(|| {
        metadata
            .get("settings")
            .and_then(|settings| settings.as_array())
            .and_then(|settings| dimensions_from_settings_array(settings))
    })
}

#[derive(Debug, Clone, Default)]
pub struct AgentConfigSnapshot {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub custom_model_name: String,
    pub api_key: String,
    pub assistant_id: String,
    pub auth_source: String,
    pub api_transport: String,
    pub reasoning_effort: String,
    pub context_window_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HonchoEditorField {
    Enabled,
    ApiKey,
    BaseUrl,
    WorkspaceId,
    Save,
    Cancel,
}

impl HonchoEditorField {
    pub fn next(self) -> Self {
        match self {
            Self::Enabled => Self::ApiKey,
            Self::ApiKey => Self::BaseUrl,
            Self::BaseUrl => Self::WorkspaceId,
            Self::WorkspaceId => Self::Save,
            Self::Save => Self::Cancel,
            Self::Cancel => Self::Enabled,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Enabled => Self::Cancel,
            Self::ApiKey => Self::Enabled,
            Self::BaseUrl => Self::ApiKey,
            Self::WorkspaceId => Self::BaseUrl,
            Self::Save => Self::WorkspaceId,
            Self::Cancel => Self::Save,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HonchoEditorState {
    pub enabled: bool,
    pub api_key: String,
    pub base_url: String,
    pub workspace_id: String,
    pub field: HonchoEditorField,
}

impl HonchoEditorState {
    pub fn from_config(config: &ConfigState) -> Self {
        Self {
            enabled: config.enable_honcho_memory,
            api_key: config.honcho_api_key.clone(),
            base_url: config.honcho_base_url.clone(),
            workspace_id: config.honcho_workspace_id.clone(),
            field: HonchoEditorField::Enabled,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfigAction {
    ConfigReceived(AgentConfigSnapshot),
    ConfigRawReceived(serde_json::Value),
    ModelsFetched(Vec<FetchedModel>),
    SetProvider(String),
    SetModel(String),
    SetReasoningEffort(String),
    ToggleTool(String),
}

pub struct ConfigState {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub custom_model_name: String,
    pub api_key: String,
    pub assistant_id: String,
    pub auth_source: String,
    pub api_transport: String,
    pub reasoning_effort: String,
    pub claude_permission_mode: String,
    pub openrouter_provider_order: String,
    pub openrouter_provider_ignore: String,
    pub openrouter_allow_fallbacks: bool,
    pub openrouter_response_cache_enabled: bool,
    pub huggingface_provider: String,
    pub openrouter_endpoint_providers: Vec<String>,
    pub custom_context_window_tokens: Option<u32>,
    pub chatgpt_auth_available: bool,
    pub db_backend: String,
    pub db_sync_url: String,
    pub db_has_token: bool,
    pub db_seeded_at: Option<i64>,
    pub chatgpt_auth_source: Option<String>,
    pub fetched_models: Vec<FetchedModel>,
    pub agent_config_raw: Option<serde_json::Value>,

    pub tool_bash: bool,
    pub tool_file_ops: bool,
    pub tool_web_search: bool,
    pub tool_web_browse: bool,
    pub tool_vision: bool,
    pub tool_system_info: bool,
    pub tool_gateway: bool,

    pub search_provider: String,
    pub duckduckgo_region: String,
    pub duckduckgo_safe_search: String,
    pub firecrawl_api_key: String,
    pub exa_api_key: String,
    pub tavily_api_key: String,
    pub search_max_results: u32,
    pub search_timeout_secs: u32,

    pub browse_provider: String,

    pub custom_modalities: String,

    pub gateway_enabled: bool,
    pub gateway_default_agent: String,
    pub slack_default_agent: String,
    pub discord_default_agent: String,
    pub telegram_default_agent: String,
    pub whatsapp_default_agent: String,
    pub gateway_prefix: String,
    pub slack_token: String,
    pub slack_channel_filter: String,
    pub telegram_token: String,
    pub telegram_allowed_chats: String,
    pub discord_token: String,
    pub discord_channel_filter: String,
    pub discord_allowed_users: String,
    pub whatsapp_allowed_contacts: String,
    pub whatsapp_token: String,
    pub whatsapp_phone_id: String,

    pub enable_streaming: bool,
    pub enable_conversation_memory: bool,
    pub enable_honcho_memory: bool,
    pub honcho_api_key: String,
    pub honcho_base_url: String,
    pub honcho_workspace_id: String,
    pub honcho_editor: Option<HonchoEditorState>,
    pub mlflow_enabled: bool,
    pub mlflow_tracking_uri: String,
    pub mlflow_experiment_name: String,
    pub mlflow_experiment_id: String,
    pub mlflow_capture_mode: String,
    pub mlflow_visible_operator: bool,
    pub mlflow_gateway: bool,
    pub mlflow_goal_task: bool,
    pub mlflow_subagent: bool,
    pub mlflow_concierge: bool,
    pub mlflow_heartbeat_autonomous: bool,
    pub mlflow_runtime_status: Option<serde_json::Value>,
    pub mlflow_test_result: Option<serde_json::Value>,
    pub anticipatory_enabled: bool,
    pub anticipatory_morning_brief: bool,
    pub anticipatory_predictive_hydration: bool,
    pub anticipatory_stuck_detection: bool,
    pub operator_model_enabled: bool,
    pub operator_model_allow_message_statistics: bool,
    pub operator_model_allow_approval_learning: bool,
    pub operator_model_allow_attention_tracking: bool,
    pub operator_model_allow_implicit_feedback: bool,
    pub collaboration_enabled: bool,
    pub compliance_mode: String,
    pub compliance_retention_days: u32,
    pub compliance_sign_all_events: bool,
    pub tool_synthesis_enabled: bool,
    pub tool_synthesis_require_activation: bool,
    pub tool_synthesis_max_generated_tools: u32,
    pub managed_sandbox_enabled: bool,
    pub managed_security_level: String,

    pub auto_compact_context: bool,
    pub max_context_messages: u32,
    pub tui_chat_history_page_size: u32,
    pub participant_observer_restore_window_hours: u32,
    pub max_tool_loops: u32,
    pub max_retries: u32,
    pub auto_refresh_interval_secs: u32,
    pub retry_delay_ms: u32,
    pub message_loop_delay_ms: u32,
    pub tool_call_delay_ms: u32,
    pub llm_stream_chunk_timeout_secs: u32,
    pub auto_retry: bool,
    pub context_window_tokens: u32,
    pub compact_threshold_pct: u32,
    pub keep_recent_on_compact: u32,
    pub bash_timeout_secs: u32,
    pub weles_max_concurrent_reviews: u32,
    pub compaction_strategy: String,
    pub compaction_weles_provider: String,
    pub compaction_weles_model: String,
    pub compaction_weles_reasoning_effort: String,
    pub compaction_weles_api_transport: String,
    pub compaction_custom_provider: String,
    pub compaction_custom_base_url: String,
    pub compaction_custom_model: String,
    pub compaction_custom_api_key: String,
    pub compaction_custom_assistant_id: String,
    pub compaction_custom_auth_source: String,
    pub compaction_custom_api_transport: String,
    pub compaction_custom_reasoning_effort: String,
    pub compaction_custom_context_window_tokens: u32,

    pub snapshot_max_count: u32,
    pub snapshot_max_size_mb: u32,
    pub snapshot_auto_cleanup: bool,

    pub snapshot_count: usize,
    pub snapshot_total_size_bytes: u64,

    pub workspace_repo_monitor_enabled: bool,
    pub workspace_repo_monitor_include_dirs: String,
    pub workspace_repo_monitor_exclude_dirs: String,
}
