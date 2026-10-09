use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zorai_protocol::{AGENT_NAME_RAROG, AGENT_NAME_SWAROG};

use zorai_shared::providers::PROVIDER_ID_OPENAI;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub slack_token: String,
    #[serde(default)]
    pub slack_channel_filter: String,
    #[serde(default)]
    pub telegram_token: String,
    #[serde(default)]
    pub telegram_allowed_chats: String,
    #[serde(default)]
    pub discord_token: String,
    #[serde(default)]
    pub discord_channel_filter: String,
    #[serde(default)]
    pub discord_allowed_users: String,
    #[serde(default)]
    pub whatsapp_allowed_contacts: String,
    #[serde(default)]
    pub whatsapp_token: String,
    #[serde(default)]
    pub whatsapp_phone_id: String,
    #[serde(default)]
    pub command_prefix: String,
    #[serde(default = "default_gateway_agent")]
    pub default_agent: String,
    /// Feature flag: when false (default), only daemon gateways run.
    /// When true, Electron bridges run alongside daemon gateways. Per D-07.
    /// Note: platform tokens may also be provided via env vars as a fallback
    /// (SLACK_TOKEN, TELEGRAM_TOKEN, etc.) per D-02 migration path.
    #[serde(default)]
    pub gateway_electron_bridges_enabled: bool,
    #[serde(default)]
    pub whatsapp_link_fallback_electron: bool,
}

pub(crate) fn default_gateway_agent() -> String {
    zorai_protocol::AGENT_ID_RAROG.into()
}
pub(crate) fn default_provider() -> String {
    PROVIDER_ID_OPENAI.into()
}
pub(crate) fn default_embedding_provider() -> String {
    PROVIDER_ID_OPENAI.into()
}
pub(crate) fn default_embedding_model() -> String {
    "text-embedding-3-small".into()
}
pub(crate) fn default_embedding_dimensions() -> u32 {
    1536
}
pub(crate) fn default_embedding_batch_size() -> u32 {
    64
}
pub(crate) fn default_embedding_max_concurrency() -> u32 {
    2
}
pub(crate) fn default_api_transport() -> ApiTransport {
    default_api_transport_for_provider(PROVIDER_ID_OPENAI)
}
pub(crate) fn default_auth_source() -> AuthSource {
    AuthSource::ApiKey
}
pub(crate) fn default_system_prompt() -> String {
    format!(
        "You are {} - The Smith (He is a blacksmith god, the creator and craftsman of the heavens in ancient Slavic belief. As an AI agent:\n- Creation: Ideal for tasks intended for use from scratch (coding, writing, design).\n- Rhythm: Associated with the sun and fire, he naturally determines the daily cycles (sunrise-sunset).\n- Personality: Strict but fair; an accessible \"doer\" who ensures this through perfect tools.) operating in zorai, an always-on agentic runtime assistant. {} is your concierge counterpart: lighter, faster, and operator-facing. You can execute terminal commands, monitor systems, and send messages to connected chat platforms. Use your tools proactively. Be concise and direct.",
        AGENT_NAME_SWAROG, AGENT_NAME_RAROG
    )
}
pub(crate) fn default_reasoning_effort() -> String {
    "high".into()
}
pub(crate) fn default_max_tool_loops() -> u32 {
    // Generous but finite safety floor. Operators with long workflows can
    // raise this; goal-runner durable tasks override to 0 (unbounded) at the
    // call site in `agent_loop::send_message::setup`. Previously this returned
    // `0` (unbounded by default), which contradicted the architecture page's
    // "bounded loop of model and tool calls" claim — every non-goal turn ran
    // without an iteration cap unless the operator opted in.
    64
}
pub(crate) fn default_pty_channel_capacity() -> usize {
    1024
}
pub(crate) fn default_agent_event_channel_capacity() -> usize {
    512
}
pub(crate) fn default_max_retries() -> u32 {
    3
}
pub(crate) fn default_retry_delay_ms() -> u64 {
    5000
}
pub(crate) fn default_message_loop_delay_ms() -> u64 {
    100
}
pub(crate) fn default_tool_call_delay_ms() -> u64 {
    100
}
pub(crate) fn default_llm_stream_chunk_timeout_secs() -> u64 {
    300
}
pub(crate) fn default_auto_retry() -> bool {
    true
}
pub(crate) fn default_auto_compact_context() -> bool {
    true
}
pub(crate) fn default_max_context_messages() -> u32 {
    100
}
pub(crate) fn default_participant_observer_restore_window_hours() -> u32 {
    24
}
pub(crate) fn default_context_window_tokens() -> u32 {
    128_000
}
pub(crate) fn default_compact_threshold_pct() -> u32 {
    80
}
pub(crate) fn default_keep_recent_on_compact() -> u32 {
    10
}
pub(crate) fn default_weles_compaction_reasoning_effort() -> String {
    "medium".into()
}
pub(crate) fn default_task_poll_secs() -> u64 {
    10
}
pub(crate) fn default_ema_alpha() -> f64 {
    0.3
}
pub(crate) fn default_low_activity_frequency_factor() -> u64 {
    4
}
pub(crate) fn default_ema_activity_threshold() -> f64 {
    2.0
}
pub(crate) fn default_heartbeat_mins() -> u64 {
    30
}
pub(crate) fn default_morning_brief_window_minutes() -> u32 {
    30
}
pub(crate) fn default_stuck_detection_delay_seconds() -> u64 {
    45
}
pub(crate) fn default_surfacing_min_confidence() -> f64 {
    0.7
}
pub(crate) fn default_surface_cooldown_seconds() -> u64 {
    300
}
pub(crate) fn default_honcho_workspace_id() -> String {
    "zorai".to_string()
}
pub(crate) fn default_compliance_retention_days() -> u32 {
    90
}
pub(crate) fn default_generated_tool_limit() -> usize {
    20
}
pub(crate) fn default_generated_tool_auto_promote_threshold() -> f64 {
    0.85
}
pub(crate) fn default_generated_tool_timeout_secs() -> u64 {
    30
}
pub(crate) fn default_generated_tool_output_kb() -> usize {
    512
}
pub(crate) fn default_snapshot_max_total_size_mb() -> u64 {
    10_240
}
pub(crate) fn default_offload_tool_result_threshold_bytes() -> usize {
    50 * 1024
}
impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_provider(),
            base_url: String::new(),
            model: String::new(),
            api_key: String::new(),
            assistant_id: String::new(),
            enable_honcho_memory: false,
            mlflow_tracing: crate::agent::mlflow_tracing::MlflowTracingConfig::default(),
            honcho_api_key: String::new(),
            honcho_base_url: String::new(),
            honcho_workspace_id: default_honcho_workspace_id(),
            auth_source: default_auth_source(),
            api_transport: default_api_transport(),
            reasoning_effort: default_reasoning_effort(),
            claude_permission_mode: None,
            system_prompt: default_system_prompt(),
            max_tool_loops: default_max_tool_loops(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            message_loop_delay_ms: default_message_loop_delay_ms(),
            tool_call_delay_ms: default_tool_call_delay_ms(),
            llm_stream_chunk_timeout_secs: default_llm_stream_chunk_timeout_secs(),
            auto_retry: default_auto_retry(),
            auto_compact_context: default_auto_compact_context(),
            max_context_messages: default_max_context_messages(),
            participant_observer_restore_window_hours:
                default_participant_observer_restore_window_hours(),
            context_window_tokens: default_context_window_tokens(),
            compact_threshold_pct: default_compact_threshold_pct(),
            keep_recent_on_compact: default_keep_recent_on_compact(),
            compaction: CompactionConfig::default(),
            task_poll_interval_secs: default_task_poll_secs(),
            heartbeat_interval_mins: default_heartbeat_mins(),
            heartbeat_cron: None,
            heartbeat_checks: HeartbeatChecksConfig::default(),
            audit: AuditConfig::default(),
            quiet_hours_start: None,
            quiet_hours_end: None,
            dnd_enabled: false,
            tools: ToolsConfig::default(),
            providers: HashMap::new(),
            gateway: GatewayConfig::default(),
            snapshot_retention: SnapshotRetentionSettings::default(),
            offload_tool_result_threshold_bytes: default_offload_tool_result_threshold_bytes(),
            agent_backend: AgentBackend::default(),
            sub_agents: Vec::new(),
            builtin_sub_agents: BuiltinSubAgentOverrides::default(),
            concierge: ConciergeConfig::default(),
            anticipatory: AnticipatoryConfig::default(),
            operator_model: OperatorModelConfig::default(),
            collaboration: CollaborationConfig::default(),
            debate: DebateConfig::default(),
            critique: CritiqueConfig::default(),
            compliance: ComplianceConfig::default(),
            tool_synthesis: ToolSynthesisConfig::default(),
            managed_execution: ManagedExecutionConfig::default(),
            pty_channel_capacity: default_pty_channel_capacity(),
            agent_event_channel_capacity: default_agent_event_channel_capacity(),
            ema_alpha: default_ema_alpha(),
            low_activity_frequency_factor: default_low_activity_frequency_factor(),
            ema_activity_threshold: default_ema_activity_threshold(),
            consolidation: ConsolidationConfig::default(),
            skill_discovery: SkillDiscoveryConfig::default(),
            skill_recommendation: SkillRecommendationConfig::default(),
            semantic: SemanticConfig::default(),
            routing: RoutingConfig::default(),
            skill_promotion: SkillPromotionConfig::default(),
            tier: TierConfig::default(),
            episodic: super::super::episodic::EpisodicConfig::default(),
            uncertainty: super::super::uncertainty::UncertaintyConfig::default(),
            cost: super::super::cost::CostConfig::default(),
            auto_thread_title: AutoThreadTitleMode::Off,
            extra: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicRequestMetadata {
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicCacheControlEphemeral {
    #[serde(rename = "type")]
    pub cache_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicToolChoice {
    Auto {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Any {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Tool {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    None {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    #[serde(default)]
    pub assistant_id: String,
    #[serde(default = "default_auth_source")]
    pub auth_source: AuthSource,
    #[serde(default = "default_api_transport")]
    pub api_transport: ApiTransport,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
    #[serde(default = "default_context_window_tokens")]
    pub context_window_tokens: u32,
    /// When set, request structured output with this JSON schema from the API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AnthropicRequestMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<AnthropicCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_tool_choice: Option<AnthropicToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_ignore: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_allow_fallbacks: Option<bool>,
    #[serde(default)]
    pub openrouter_response_cache_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub huggingface_provider: Option<String>,
}

/// A named sub-agent definition that the orchestration engine can dispatch work to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentDefinition {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_whitelist: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_blacklist: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_budget_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supervisor_config: Option<SupervisorConfig>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub immutable_identity: bool,
    #[serde(default = "default_sub_agent_disable_allowed")]
    pub disable_allowed: bool,
    #[serde(default = "default_sub_agent_delete_allowed")]
    pub delete_allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_transport: Option<ApiTransport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_ignore: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_allow_fallbacks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub huggingface_provider: Option<String>,
    #[serde(default)]
    pub created_at: u64,
}

impl SubAgentDefinition {
    pub fn is_spawnable(&self) -> bool {
        self.enabled && self.protected_reason.is_none()
    }

    pub fn matches_spawn_request(&self, requested_title: &str) -> bool {
        self.name.eq_ignore_ascii_case(requested_title)
            || self
                .role
                .as_deref()
                .is_some_and(|role| role.eq_ignore_ascii_case(requested_title))
    }
}

/// Snapshot of a provider's authentication status for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAuthState {
    pub provider_id: String,
    pub provider_name: String,
    pub authenticated: bool,
    pub auth_source: AuthSource,
    pub model: String,
    pub base_url: String,
}

/// A structured fallback suggestion for an outage or degraded provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderAlternativeSuggestion {
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub reason: String,
}

/// Structured outage metadata attached to provider circuit-breaker events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCircuitOpenDetails {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_model: Option<String>,
    pub trip_count: u32,
    pub reason: String,
    #[serde(default)]
    pub suggested_alternatives: Vec<ProviderAlternativeSuggestion>,
}

/// Structured provider-health snapshot entry exposed in `provider_health_json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderHealthSnapshot {
    pub provider_id: String,
    pub can_execute: bool,
    pub trip_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub suggested_alternatives: Vec<ProviderAlternativeSuggestion>,
}

pub(crate) fn default_provider_circuit_reason() -> String {
    "circuit breaker open".to_string()
}

/// How much context the concierge gathers for its welcome greeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConciergeDetailLevel {
    Minimal,
    #[default]
    ContextSummary,
    ProactiveTriage,
    DailyBriefing,
}

/// The type of quick-action a concierge welcome button triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConciergeActionType {
    ContinueSession,
    StartNew,
    Search,
    Dismiss,
    StartGoalRun,
    DismissWelcome,
    FocusChat,
    OpenSettings,
}

/// A structured quick-action button in the concierge welcome message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConciergeAction {
    pub label: String,
    pub action_type: ConciergeActionType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

/// Configuration for the concierge agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConciergeConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub detail_level: ConciergeDetailLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_transport: Option<ApiTransport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_ignore: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_allow_fallbacks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub huggingface_provider: Option<String>,
    #[serde(default = "default_true")]
    pub auto_cleanup_on_navigate: bool,
}

impl Default for ConciergeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            detail_level: ConciergeDetailLevel::default(),
            provider: None,
            model: None,
            base_url: None,
            reasoning_effort: None,
            api_transport: None,
            claude_permission_mode: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            huggingface_provider: None,
            auto_cleanup_on_navigate: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_true")]
    pub bash: bool,
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub web_browse: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default = "default_true")]
    pub gateway_messaging: bool,
    #[serde(default = "default_true")]
    pub file_operations: bool,
    #[serde(default = "default_true")]
    pub system_info: bool,
    #[serde(default = "default_true")]
    pub deferred_tool_loading: bool,
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn default_sub_agent_disable_allowed() -> bool {
    true
}

pub(crate) fn default_sub_agent_delete_allowed() -> bool {
    true
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            bash: true,
            web_search: false,
            web_browse: false,
            vision: false,
            gateway_messaging: true,
            file_operations: true,
            system_info: true,
            deferred_tool_loading: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    pub position: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_index: Option<usize>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkContextEntryKind {
    RepoChange,
    Artifact,
    GeneratedSkill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkContextEntry {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub kind: WorkContextEntryKind,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_hash: Option<String>,
    #[serde(default)]
    pub is_text: bool,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadWorkContext {
    pub thread_id: String,
    #[serde(default)]
    pub entries: Vec<WorkContextEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntentPredictionCandidate {
    pub rank: u8,
    pub action: String,
    pub confidence: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntentPredictionPayload {
    pub primary_action: String,
    pub confidence: f64,
    #[serde(default)]
    pub ranked_actions: Vec<IntentPredictionCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpeculativeOpportunityStatus {
    Queued,
    Running,
    Completed,
    Expired,
    Consumed,
    Dropped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpeculativeOpportunity {
    pub id: String,
    pub thread_id: Option<String>,
    pub source_kind: String,
    pub action_kind: String,
    pub confidence: f64,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub status: SpeculativeOpportunityStatus,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpeculativeResult {
    pub opportunity_id: String,
    pub action_kind: String,
    pub thread_id: Option<String>,
    pub summary: String,
    pub artifact: serde_json::Value,
    pub completed_at_ms: u64,
    pub expires_at_ms: u64,
    pub used_at_ms: Option<u64>,
    pub precomputation_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnticipatoryItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub bullets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_prediction: Option<IntentPredictionPayload>,
    pub confidence: f64,
    #[serde(default)]
    pub goal_run_id: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_client_surface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_attention_surface: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_max_tool_loops_is_a_finite_safety_floor() {
        // Production default must be > 0 so non-goal turns get a real bound.
        // Goal-runner durable tasks override to 0 (unbounded) at the call site
        // in agent_loop::send_message::setup, so this default only governs
        // non-goal threads.
        let cap = default_max_tool_loops();
        assert!(
            cap > 0,
            "default_max_tool_loops must be > 0 so non-goal turns are bounded by default"
        );
        // Sanity ceiling: catch a future change that accidentally pushes the
        // default into the thousands (which would functionally re-enable
        // unbounded behavior).
        assert!(
            cap <= 1024,
            "default_max_tool_loops should remain a *floor*, not approach unbounded; got {cap}"
        );
    }
}
