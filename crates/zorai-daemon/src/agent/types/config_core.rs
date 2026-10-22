use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use zorai_protocol::SecurityLevel;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentBackend {
    #[default]
    Daemon,
    Openclaw,
    Hermes,
    Legacy,
}

impl std::fmt::Display for AgentBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daemon => f.write_str("daemon"),
            Self::Openclaw => f.write_str("openclaw"),
            Self::Hermes => f.write_str("hermes"),
            Self::Legacy => f.write_str("legacy"),
        }
    }
}

impl AgentBackend {
    /// Return the variant as a static string slice matching the serde representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daemon => "daemon",
            Self::Openclaw => "openclaw",
            Self::Hermes => "hermes",
            Self::Legacy => "legacy",
        }
    }

    /// Parse a backend name, falling back to [`AgentBackend::Daemon`] for
    /// unrecognised values.
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "openclaw" => Self::Openclaw,
            "hermes" => Self::Hermes,
            "legacy" => Self::Legacy,
            _ => Self::Daemon,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotRetentionSettings {
    #[serde(default)]
    pub max_snapshots: usize,
    #[serde(default = "default_snapshot_max_total_size_mb")]
    pub max_total_size_mb: u64,
    #[serde(default)]
    pub auto_cleanup: bool,
}

impl Default for SnapshotRetentionSettings {
    fn default() -> Self {
        Self {
            max_snapshots: 0,
            max_total_size_mb: default_snapshot_max_total_size_mb(),
            auto_cleanup: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticEmbeddingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_embedding_provider")]
    pub provider: String,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_embedding_dimensions")]
    pub dimensions: u32,
    #[serde(default = "default_embedding_batch_size")]
    pub batch_size: u32,
    #[serde(default = "default_embedding_max_concurrency")]
    pub max_concurrency: u32,
}

impl Default for SemanticEmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_embedding_provider(),
            model: default_embedding_model(),
            dimensions: default_embedding_dimensions(),
            batch_size: default_embedding_batch_size(),
            max_concurrency: default_embedding_max_concurrency(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SemanticConfig {
    #[serde(default)]
    pub embedding: SemanticEmbeddingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub assistant_id: String,
    #[serde(default)]
    pub enable_honcho_memory: bool,
    #[serde(default = "default_true")]
    pub desktop_notifications: bool,
    /// Optional fail-open export of agent turns to MLflow via OTLP/HTTP.
    #[serde(default)]
    pub mlflow_tracing: crate::agent::mlflow_tracing::MlflowTracingConfig,
    #[serde(default)]
    pub honcho_api_key: String,
    #[serde(default)]
    pub honcho_base_url: String,
    #[serde(default = "default_honcho_workspace_id")]
    pub honcho_workspace_id: String,
    #[serde(default = "default_auth_source")]
    pub auth_source: AuthSource,
    #[serde(default = "default_api_transport")]
    pub api_transport: ApiTransport,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_permission_mode: Option<String>,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub agent_aliases: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_max_tool_loops")]
    pub max_tool_loops: u32,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    #[serde(default = "default_message_loop_delay_ms")]
    pub message_loop_delay_ms: u64,
    #[serde(default = "default_tool_call_delay_ms")]
    pub tool_call_delay_ms: u64,
    #[serde(default = "default_llm_stream_chunk_timeout_secs")]
    pub llm_stream_chunk_timeout_secs: u64,
    #[serde(default = "default_auto_retry")]
    pub auto_retry: bool,
    #[serde(default = "default_auto_compact_context")]
    pub auto_compact_context: bool,
    #[serde(default = "default_max_context_messages")]
    pub max_context_messages: u32,
    #[serde(default = "default_participant_observer_restore_window_hours")]
    pub participant_observer_restore_window_hours: u32,
    #[serde(default = "default_context_window_tokens")]
    pub context_window_tokens: u32,
    #[serde(default = "default_compact_threshold_pct")]
    pub compact_threshold_pct: u32,
    #[serde(default = "default_keep_recent_on_compact")]
    pub keep_recent_on_compact: u32,
    #[serde(default)]
    pub compaction: CompactionConfig,
    #[serde(default = "default_task_poll_secs")]
    pub task_poll_interval_secs: u64,
    #[serde(default = "default_heartbeat_mins")]
    pub heartbeat_interval_mins: u64,
    /// Cron expression for heartbeat schedule (overrides heartbeat_interval_mins). Per D-06.
    #[serde(default)]
    pub heartbeat_cron: Option<String>,
    /// Heartbeat check configuration. Per D-04.
    #[serde(default)]
    pub heartbeat_checks: HeartbeatChecksConfig,
    /// Audit trail configuration: scope, confidence, retention. Per D-05/D-10.
    #[serde(default)]
    pub audit: AuditConfig,
    /// Quiet hours start (hour 0-23, local time). Per D-07.
    #[serde(default)]
    pub quiet_hours_start: Option<u32>,
    /// Quiet hours end (hour 0-23, local time). Per D-07.
    #[serde(default)]
    pub quiet_hours_end: Option<u32>,
    /// Manual do-not-disturb toggle. Per D-07.
    #[serde(default)]
    pub dnd_enabled: bool,
    #[serde(default)]
    pub tools: ToolsConfig,
    /// Additional provider configurations keyed by provider name.
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    /// Gateway configuration for chat platform connections.
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub snapshot_retention: SnapshotRetentionSettings,
    #[serde(default = "default_offload_tool_result_threshold_bytes")]
    pub offload_tool_result_threshold_bytes: usize,
    /// Agent backend: daemon (built-in LLM), openclaw, hermes, or legacy.
    #[serde(default)]
    pub agent_backend: AgentBackend,
    /// Registry of named sub-agents for orchestration dispatch.
    #[serde(default)]
    pub sub_agents: Vec<SubAgentDefinition>,
    /// Daemon-owned built-in subagent overrides kept separate from user subagents.
    #[serde(default)]
    pub builtin_sub_agents: BuiltinSubAgentOverrides,
    /// Concierge agent configuration.
    #[serde(default)]
    pub concierge: ConciergeConfig,
    /// Anticipatory pre-loading and surfacing controls.
    #[serde(default)]
    pub anticipatory: AnticipatoryConfig,
    /// Learned operator-model controls.
    #[serde(default)]
    pub operator_model: OperatorModelConfig,
    /// Multi-agent collaboration controls.
    #[serde(default)]
    pub collaboration: CollaborationConfig,
    /// Multi-round debate protocol controls.
    #[serde(default)]
    pub debate: DebateConfig,
    /// Adversarial self-critique controls.
    #[serde(default)]
    pub critique: CritiqueConfig,
    /// Trusted provenance and compliance controls.
    #[serde(default)]
    pub compliance: ComplianceConfig,
    /// Runtime tool synthesis controls.
    #[serde(default)]
    pub tool_synthesis: ToolSynthesisConfig,
    /// Default managed-command policy applied when tools do not override it.
    #[serde(default)]
    pub managed_execution: ManagedExecutionConfig,
    /// Broadcast channel capacity for PTY session output fanout.
    #[serde(default = "default_pty_channel_capacity")]
    pub pty_channel_capacity: usize,
    /// Broadcast channel capacity for agent event fanout.
    #[serde(default = "default_agent_event_channel_capacity")]
    pub agent_event_channel_capacity: usize,
    /// EMA smoothing factor for activity histogram adaptation. Per D-02.
    #[serde(default = "default_ema_alpha")]
    pub ema_alpha: f64,
    /// Heartbeat frequency reduction factor during low-activity hours. Per D-03.
    #[serde(default = "default_low_activity_frequency_factor")]
    pub low_activity_frequency_factor: u64,
    /// Minimum smoothed count to consider an hour "active". Per D-02.
    #[serde(default = "default_ema_activity_threshold")]
    pub ema_activity_threshold: f64,
    /// Memory consolidation controls (Phase 5).
    #[serde(default)]
    pub consolidation: ConsolidationConfig,
    /// Skill discovery thresholds (Phase 6).
    #[serde(default)]
    pub skill_discovery: SkillDiscoveryConfig,
    /// Runtime skill recommender controls.
    #[serde(default)]
    pub skill_recommendation: SkillRecommendationConfig,
    /// Semantic search and embedding model controls.
    #[serde(default)]
    pub semantic: SemanticConfig,
    /// Specialist routing controls (Spec-03 probabilistic agent routing).
    #[serde(default)]
    pub routing: RoutingConfig,
    /// Skill promotion thresholds (Phase 6).
    #[serde(default)]
    pub skill_promotion: SkillPromotionConfig,
    /// Capability tier configuration (Phase 10).
    #[serde(default)]
    pub tier: TierConfig,
    /// Episodic memory configuration (Phase v3.0).
    #[serde(default)]
    pub episodic: super::super::episodic::EpisodicConfig,
    /// Uncertainty quantification configuration (Phase v3.0: UNCR-01 through UNCR-08).
    #[serde(default)]
    pub uncertainty: super::super::uncertainty::UncertaintyConfig,
    /// Cost tracking configuration (Phase v3.0: COST-01 through COST-04).
    #[serde(default)]
    pub cost: super::super::cost::CostConfig,
    #[serde(default)]
    pub auto_thread_title: AutoThreadTitleMode,
    /// Additional persisted agent settings used by richer frontends and the TUI.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoThreadTitleMode {
    #[default]
    Off,
    Rarog,
    Weles,
}

impl AutoThreadTitleMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Rarog => "rarog",
            Self::Weles => "weles",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "rarog" => Self::Rarog,
            "weles" => Self::Weles,
            _ => Self::Off,
        }
    }

    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }
}

impl<'de> Deserialize<'de> for AutoThreadTitleMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Option::<String>::deserialize(deserializer)?;
        Ok(value.as_deref().map(Self::parse).unwrap_or_default())
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionStrategy {
    #[default]
    Heuristic,
    Weles,
    CustomModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CompactionConfig {
    #[serde(default)]
    pub strategy: CompactionStrategy,
    #[serde(default)]
    pub weles: WelesCompactionConfig,
    #[serde(default)]
    pub custom_model: CustomModelCompactionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WelesCompactionConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_weles_compaction_reasoning_effort")]
    pub reasoning_effort: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_transport: Option<ApiTransport>,
}

impl Default for WelesCompactionConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            model: String::new(),
            reasoning_effort: default_weles_compaction_reasoning_effort(),
            api_transport: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomModelCompactionConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
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
}

impl Default for CustomModelCompactionConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            base_url: String::new(),
            model: String::new(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: default_auth_source(),
            api_transport: default_api_transport(),
            reasoning_effort: default_reasoning_effort(),
            context_window_tokens: default_context_window_tokens(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuiltinSubAgentOverrides {
    #[serde(default)]
    pub weles: WelesBuiltinOverrides,
    #[serde(default)]
    pub swarozyc: BuiltinPersonaOverrides,
    #[serde(default)]
    pub radogost: BuiltinPersonaOverrides,
    #[serde(default)]
    pub domowoj: BuiltinPersonaOverrides,
    #[serde(default)]
    pub swietowit: BuiltinPersonaOverrides,
    #[serde(default)]
    pub perun: BuiltinPersonaOverrides,
    #[serde(default)]
    pub mokosh: BuiltinPersonaOverrides,
    #[serde(default)]
    pub dazhbog: BuiltinPersonaOverrides,
    #[serde(default)]
    pub rod: BuiltinPersonaOverrides,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WelesBuiltinOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_transport: Option<ApiTransport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_ignore: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_allow_fallbacks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub huggingface_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_reviews: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuiltinPersonaOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openrouter_provider_ignore: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_allow_fallbacks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub huggingface_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnticipatoryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub morning_brief: bool,
    #[serde(default)]
    pub predictive_hydration: bool,
    #[serde(default)]
    pub stuck_detection: bool,
    #[serde(default = "default_morning_brief_window_minutes")]
    pub morning_brief_window_minutes: u32,
    #[serde(default = "default_stuck_detection_delay_seconds")]
    pub stuck_detection_delay_seconds: u64,
    #[serde(default = "default_surfacing_min_confidence")]
    pub surfacing_min_confidence: f64,
    #[serde(default = "default_surface_cooldown_seconds")]
    pub surface_cooldown_seconds: u64,
}

impl Default for AnticipatoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            morning_brief: true,
            predictive_hydration: true,
            stuck_detection: true,
            morning_brief_window_minutes: default_morning_brief_window_minutes(),
            stuck_detection_delay_seconds: default_stuck_detection_delay_seconds(),
            surfacing_min_confidence: default_surfacing_min_confidence(),
            surface_cooldown_seconds: default_surface_cooldown_seconds(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorModelConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allow_message_statistics: bool,
    #[serde(default)]
    pub allow_approval_learning: bool,
    #[serde(default)]
    pub allow_attention_tracking: bool,
    #[serde(default)]
    pub allow_implicit_feedback: bool,
}

impl Default for OperatorModelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_message_statistics: true,
            allow_approval_learning: true,
            allow_attention_tracking: true,
            allow_implicit_feedback: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationConfig {
    #[serde(default)]
    pub enabled: bool,
}

impl Default for CollaborationConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CritiqueConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: CritiqueMode,
    #[serde(default = "default_true")]
    pub guard_suspicious_tool_calls_only: bool,
}

impl Default for CritiqueConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: CritiqueMode::Disabled,
            guard_suspicious_tool_calls_only: default_true(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CritiqueMode {
    #[default]
    Disabled,
    Deterministic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_debate_max_rounds")]
    pub default_max_rounds: u8,
    #[serde(default = "default_debate_min_evidence_refs")]
    pub min_evidence_refs: u8,
    #[serde(default = "default_true")]
    pub role_rotation: bool,
    #[serde(default = "default_debate_required_sections")]
    pub verdict_required_sections: Vec<String>,
}

fn default_debate_max_rounds() -> u8 {
    3
}

fn default_debate_min_evidence_refs() -> u8 {
    1
}

fn default_debate_required_sections() -> Vec<String> {
    vec![
        "consensus_points".to_string(),
        "unresolved_tensions".to_string(),
        "recommended_action".to_string(),
    ]
}

impl Default for DebateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_max_rounds: default_debate_max_rounds(),
            min_evidence_refs: default_debate_min_evidence_refs(),
            role_rotation: default_true(),
            verdict_required_sections: default_debate_required_sections(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceMode {
    #[default]
    Standard,
    Soc2,
    Hipaa,
    Fedramp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    #[serde(default)]
    pub mode: ComplianceMode,
    #[serde(default = "default_compliance_retention_days")]
    pub retention_days: u32,
    #[serde(default)]
    pub sign_all_events: bool,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            mode: ComplianceMode::default(),
            retention_days: default_compliance_retention_days(),
            sign_all_events: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSynthesisConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub require_activation: bool,
    #[serde(default = "default_generated_tool_limit")]
    pub max_generated_tools: usize,
    #[serde(default = "default_generated_tool_auto_promote_threshold")]
    pub auto_promote_threshold: f64,
    #[serde(default)]
    pub sandbox: ToolSynthesisSandboxConfig,
}

impl Default for ToolSynthesisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_activation: true,
            max_generated_tools: default_generated_tool_limit(),
            auto_promote_threshold: default_generated_tool_auto_promote_threshold(),
            sandbox: ToolSynthesisSandboxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedExecutionConfig {
    #[serde(default)]
    pub sandbox_enabled: bool,
    #[serde(default)]
    pub security_level: SecurityLevel,
}

impl Default for ManagedExecutionConfig {
    fn default() -> Self {
        Self {
            sandbox_enabled: false,
            security_level: SecurityLevel::Lowest,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSynthesisSandboxConfig {
    #[serde(default = "default_generated_tool_timeout_secs")]
    pub max_execution_time_secs: u64,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default)]
    pub allow_filesystem: bool,
    #[serde(default = "default_generated_tool_output_kb")]
    pub max_output_kb: usize,
}

impl Default for ToolSynthesisSandboxConfig {
    fn default() -> Self {
        Self {
            max_execution_time_secs: default_generated_tool_timeout_secs(),
            allow_network: false,
            allow_filesystem: false,
            max_output_kb: default_generated_tool_output_kb(),
        }
    }
}
