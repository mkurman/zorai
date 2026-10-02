use serde::{Deserialize, Serialize};

mod json_string_or_value {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &str, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let json = serde_json::from_str::<serde_json::Value>(value)
                .map_err(serde::ser::Error::custom)?;
            json.serialize(serializer)
        } else {
            serializer.serialize_str(value)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let json = serde_json::Value::deserialize(deserializer)?;
            serde_json::to_string(&json).map_err(serde::de::Error::custom)
        } else {
            String::deserialize(deserializer)
        }
    }
}

use super::{SessionId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadHandoffResult {
    pub ok: bool,
    pub thread_id: String,
    #[serde(default)]
    pub active_agent_id: Option<String>,
    #[serde(default)]
    pub stack_depth: Option<usize>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHandoffThreadRequest {
    pub thread_id: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<String>,
    pub reason: String,
    pub summary: String,
    pub requested_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_surface: Option<ClientSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncCommandCapability {
    pub version: u32,
    pub supports_operation_acceptance: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientSurface {
    Tui,
    Electron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reaction {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationLifecycleState {
    Accepted,
    Started,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationStatusSnapshot {
    pub operation_id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedup: Option<String>,
    pub state: OperationLifecycleState,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntryPublic {
    pub id: String,
    pub timestamp: i64,
    pub action_type: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_band: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causal_trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillVariantPublic {
    pub variant_id: String,
    pub skill_name: String,
    pub variant_name: String,
    pub relative_path: String,
    pub status: String,
    pub use_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub context_tags: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunitySkillEntry {
    pub name: String,
    pub description: String,
    pub version: String,
    pub publisher_id: String,
    pub publisher_verified: bool,
    pub success_rate: f64,
    pub use_count: u32,
    pub content_hash: String,
    pub zorai_version: String,
    pub maturity_at_publish: String,
    pub tags: Vec<String>,
    pub published_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDiscoveryCandidatePublic {
    #[serde(default)]
    pub variant_id: String,
    #[serde(default)]
    pub skill_name: String,
    #[serde(default)]
    pub variant_name: String,
    #[serde(default)]
    pub relative_path: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub confidence_tier: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub matched_intents: Vec<String>,
    #[serde(default)]
    pub matched_trigger_phrases: Vec<String>,
    #[serde(default)]
    pub context_tags: Vec<String>,
    #[serde(default)]
    pub risk_level: String,
    #[serde(default)]
    pub trust_tier: String,
    #[serde(default)]
    pub source_kind: String,
    #[serde(default)]
    pub recommended_action: String,
    #[serde(default)]
    pub use_count: u32,
    #[serde(default)]
    pub success_count: u32,
    #[serde(default)]
    pub failure_count: u32,
    #[serde(default)]
    pub canonical_pack: bool,
    #[serde(default)]
    pub delivery_modes: Vec<String>,
    #[serde(default)]
    pub prerequisite_hints: Vec<String>,
    #[serde(default)]
    pub prerequisite_connectors: Vec<String>,
    #[serde(default)]
    pub source_links: Vec<String>,
    #[serde(default)]
    pub mobile_safe: bool,
    #[serde(default)]
    pub approval_behavior: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDiscoveryResultPublic {
    pub query: String,
    #[serde(default)]
    pub normalized_intent: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub confidence_tier: String,
    #[serde(default)]
    pub recommended_action: String,
    #[serde(default)]
    pub requires_approval: bool,
    #[serde(default)]
    pub mesh_state: String,
    #[serde(default)]
    pub rationale: Vec<String>,
    #[serde(default)]
    pub capability_family: Vec<String>,
    #[serde(default)]
    pub explicit_rationale_required: bool,
    #[serde(default)]
    pub workspace_tags: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<SkillDiscoveryCandidatePublic>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticDocumentSyncSummaryPublic {
    pub discovered: usize,
    pub changed: usize,
    pub queued_embeddings: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticDocumentIndexSyncResultPublic {
    pub embedding_model: String,
    pub dimensions: u32,
    pub skills: SemanticDocumentSyncSummaryPublic,
    pub guidelines: SemanticDocumentSyncSummaryPublic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticIndexRepairResultPublic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    pub removed_vector_index: bool,
    pub cleared_completions: u64,
    pub cleared_deletions: u64,
    pub reset_failed_jobs: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDescriptorPublic {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(with = "json_string_or_value")]
    pub parameters: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolListResultPublic {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(default)]
    pub items: Vec<ToolDescriptorPublic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSearchMatchPublic {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(with = "json_string_or_value")]
    pub parameters: String,
    pub score: u32,
    #[serde(default)]
    pub matched_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSearchResultPublic {
    pub query: String,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(default)]
    pub items: Vec<ToolSearchMatchPublic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanReportPublic {
    pub verdict: String,
    pub findings_count: u32,
    pub critical_count: u32,
    pub suspicious_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    pub enabled: bool,
    pub install_source: String,
    pub has_api: bool,
    pub has_auth: bool,
    pub has_commands: bool,
    pub has_skills: bool,
    pub endpoint_count: u32,
    pub settings_count: u32,
    pub installed_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub auth_status: String,
    #[serde(default)]
    pub connector_kind: Option<String>,
    #[serde(default)]
    pub connector_category: Option<String>,
    #[serde(default)]
    pub readiness_state: String,
    #[serde(default)]
    pub readiness_message: Option<String>,
    #[serde(default)]
    pub recovery_hint: Option<String>,
    #[serde(default)]
    pub setup_hint: Option<String>,
    #[serde(default)]
    pub docs_path: Option<String>,
    #[serde(default)]
    pub workflow_primitives: Vec<String>,
    #[serde(default)]
    pub read_actions: Vec<String>,
    #[serde(default)]
    pub write_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginCommandInfo {
    pub command: String,
    pub plugin_name: String,
    pub description: String,
    pub api_endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTopology {
    pub workspaces: Vec<WorkspaceTopologyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTopologyEntry {
    pub workspace_id: WorkspaceId,
    pub workspace_name: String,
    #[serde(default)]
    pub agent_owned: bool,
    #[serde(default)]
    pub last_activity_at: u64,
    pub surfaces: Vec<SurfaceTopologyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceTopologyEntry {
    pub surface_id: String,
    pub surface_name: String,
    pub layout_mode: String,
    pub is_active: bool,
    pub panes: Vec<PaneTopologyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneTopologyEntry {
    pub pane_id: String,
    pub pane_name: String,
    pub pane_type: String,
    pub is_active: bool,
    pub session_id: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub cwd: Option<String>,
}

pub fn format_topology(topology: &WorkspaceTopology, sessions: &[super::SessionInfo]) -> String {
    let session_map: std::collections::HashMap<String, &super::SessionInfo> =
        sessions.iter().map(|s| (s.id.to_string(), s)).collect();
    let mut lines = Vec::new();
    for ws in &topology.workspaces {
        lines.push(format!("Workspace \"{}\":", ws.workspace_name));
        for sf in &ws.surfaces {
            let active_tag = if sf.is_active { " (active)" } else { "" };
            lines.push(format!(
                "  Surface \"{}\" ({}{}):",
                sf.surface_name, sf.layout_mode, active_tag
            ));
            for pane in &sf.panes {
                let active_tag = if pane.is_active { " (active)" } else { "" };
                let mut parts = vec![format!(
                    "    - {} [{}] type={}",
                    pane.pane_name, pane.pane_id, pane.pane_type
                )];
                if pane.pane_type == "browser" {
                    if let Some(url) = &pane.url {
                        parts.push(format!("url={url}"));
                    }
                    if let Some(title) = &pane.title {
                        parts.push(format!("title={title}"));
                    }
                } else if let Some(sid) = &pane.session_id {
                    parts.push(format!("session={sid}"));
                    let cwd = pane
                        .cwd
                        .as_deref()
                        .or_else(|| session_map.get(sid).and_then(|s| s.cwd.as_deref()));
                    if let Some(cwd) = cwd {
                        parts.push(format!("cwd={cwd}"));
                    }
                    if let Some(s) = session_map.get(sid) {
                        if let Some(cmd) = s.active_command.as_deref() {
                            parts.push(format!("cmd={cmd}"));
                        }
                    }
                }
                if !active_tag.is_empty() {
                    parts.push(active_tag.trim().to_string());
                }
                lines.push(parts.join(" "));
            }
        }
    }
    lines.join("\n")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscNotificationPayload {
    pub source: OscSource,
    pub title: String,
    pub body: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub progress: Option<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OscSource {
    Osc9,
    Osc99,
    Osc777,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub is_dirty: bool,
    pub ahead: u32,
    pub behind: u32,
    pub untracked: u32,
    pub modified: u32,
    pub staged: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitChangeEntry {
    pub code: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedCommandSource {
    Human,
    Agent,
    Replay,
    Gateway,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalDecision {
    ApproveOnce,
    ApproveSession,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskApprovalRule {
    pub id: String,
    pub command: String,
    pub created_at: u64,
    #[serde(default)]
    pub last_used_at: Option<u64>,
    #[serde(default)]
    pub use_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SecurityLevel {
    Highest,
    #[default]
    Moderate,
    Lowest,
    Yolo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedCommandRequest {
    pub command: String,
    pub rationale: String,
    pub allow_network: bool,
    #[serde(default)]
    pub sandbox_enabled: bool,
    #[serde(default)]
    pub security_level: SecurityLevel,
    pub cwd: Option<String>,
    pub language_hint: Option<String>,
    pub source: ManagedCommandSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPayload {
    pub approval_id: String,
    pub execution_id: String,
    pub command: String,
    pub rationale: String,
    pub risk_level: String,
    pub blast_radius: String,
    pub reasons: Vec<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub allow_network: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub snapshot_id: String,
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: Option<SessionId>,
    pub command: Option<String>,
    pub kind: String,
    pub label: String,
    pub path: String,
    pub created_at: u64,
    pub status: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySearchHit {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub excerpt: String,
    pub path: Option<String>,
    pub timestamp: u64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandLogEntry {
    pub id: String,
    pub command: String,
    pub timestamp: i64,
    pub path: Option<String>,
    pub cwd: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub surface_id: Option<String>,
    pub pane_id: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDbThread {
    pub id: String,
    pub workspace_id: Option<WorkspaceId>,
    pub surface_id: Option<String>,
    pub pane_id: Option<String>,
    pub agent_name: Option<String>,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: i64,
    pub total_tokens: i64,
    pub last_preview: String,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDbMessage {
    pub id: String,
    pub thread_id: String,
    pub created_at: i64,
    pub role: String,
    pub content: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub reasoning: Option<String>,
    pub tool_calls_json: Option<String>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentMessageCursor {
    pub created_at: i64,
    pub message_id: String,
}

impl AgentMessageCursor {
    pub fn from_message(message: &AgentDbMessage) -> Self {
        Self {
            created_at: message.created_at,
            message_id: message.id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentMessageSpan {
    Range {
        start: AgentMessageCursor,
        end: AgentMessageCursor,
    },
    LastTurn {
        message: AgentMessageCursor,
    },
}

impl AgentMessageSpan {
    pub fn legacy_label(&self) -> String {
        match self {
            Self::Range { start, end } => format!("{}..{}", start.message_id, end.message_id),
            Self::LastTurn { .. } => "last_turn".to_string(),
        }
    }

    pub fn end_cursor(&self) -> AgentMessageCursor {
        match self {
            Self::Range { end, .. } => end.clone(),
            Self::LastTurn { message } => message.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryDistillationProgressRow {
    pub source_thread_id: String,
    pub last_processed_cursor: AgentMessageCursor,
    pub last_processed_span: Option<AgentMessageSpan>,
    pub last_run_at_ms: i64,
    pub updated_at_ms: i64,
    pub agent_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AgentStatisticsWindow {
    #[serde(rename = "today")]
    Today,
    #[serde(rename = "7d")]
    Last7Days,
    #[serde(rename = "30d")]
    Last30Days,
    #[default]
    #[serde(rename = "all")]
    All,
}

impl AgentStatisticsWindow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Last7Days => "7d",
            Self::Last30Days => "30d",
            Self::All => "all",
        }
    }

    pub fn from_wire(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "today" => Self::Today,
            "7d" => Self::Last7Days,
            "30d" => Self::Last30Days,
            "all" => Self::All,
            _ => Self::All,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentStatisticsTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub provider_count: u64,
    pub model_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderStatisticsRow {
    pub provider: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelStatisticsRow {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DailyStatisticsRow {
    pub day_start: u64,
    pub day_key: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub request_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionStatisticsRow {
    pub thread_id: String,
    pub title: String,
    pub updated_at: u64,
    pub provider_models: Vec<String>,
    pub request_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentStatisticsSnapshot {
    pub window: AgentStatisticsWindow,
    pub generated_at: u64,
    pub has_incomplete_cost_history: bool,
    pub totals: AgentStatisticsTotals,
    pub providers: Vec<ProviderStatisticsRow>,
    pub models: Vec<ModelStatisticsRow>,
    pub top_models_by_tokens: Vec<ModelStatisticsRow>,
    pub top_models_by_cost: Vec<ModelStatisticsRow>,
    #[serde(default)]
    pub daily: Vec<DailyStatisticsRow>,
    #[serde(default)]
    pub sessions: Vec<SessionStatisticsRow>,
    #[serde(default)]
    pub session_total: u64,
    #[serde(default)]
    pub session_limit: u64,
    #[serde(default)]
    pub session_offset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WormChainTip {
    pub kind: String,
    pub seq: i64,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptIndexEntry {
    pub id: String,
    pub pane_id: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub surface_id: Option<String>,
    pub filename: String,
    pub reason: Option<String>,
    pub captured_at: i64,
    pub size_bytes: Option<i64>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotIndexEntry {
    pub snapshot_id: String,
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: Option<String>,
    pub kind: String,
    pub label: Option<String>,
    pub path: String,
    pub created_at: i64,
    pub details_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEventRow {
    pub id: String,
    pub category: String,
    pub kind: String,
    pub pane_id: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub surface_id: Option<String>,
    pub session_id: Option<String>,
    pub payload_json: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxNotificationAction {
    pub id: String,
    pub label: String,
    pub action_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxNotification {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub severity: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
    #[serde(default)]
    pub actions: Vec<InboxNotificationAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    pub path: String,
    pub line: usize,
    pub kind: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryLedgerStatus {
    pub kind: String,
    pub total_entries: usize,
    pub valid: bool,
    pub first_invalid_seq: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceOperator {
    User,
    Svarog,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceTaskType {
    Thread,
    Goal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceTaskStatus {
    Todo,
    InProgress,
    InReview,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspacePriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl Default for WorkspacePriority {
    fn default() -> Self {
        Self::Low
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceActor {
    User,
    Agent(String),
    Subagent(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceTaskRuntimeHistoryEntry {
    pub task_type: WorkspaceTaskType,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub goal_run_id: Option<String>,
    #[serde(default)]
    pub agent_task_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub review_path: Option<String>,
    #[serde(default)]
    pub review_feedback: Option<String>,
    pub archived_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSettings {
    pub workspace_id: String,
    #[serde(default)]
    pub workspace_root: Option<String>,
    pub operator: WorkspaceOperator,
    #[serde(default)]
    pub repo_monitor_enabled: bool,
    #[serde(default)]
    pub repo_monitor_include_dirs: Vec<String>,
    #[serde(default)]
    pub repo_monitor_exclude_dirs: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceTask {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub task_type: WorkspaceTaskType,
    pub description: String,
    #[serde(default)]
    pub definition_of_done: Option<String>,
    #[serde(default)]
    pub priority: WorkspacePriority,
    pub status: WorkspaceTaskStatus,
    pub sort_order: i64,
    pub reporter: WorkspaceActor,
    #[serde(default)]
    pub assignee: Option<WorkspaceActor>,
    #[serde(default)]
    pub reviewer: Option<WorkspaceActor>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub goal_run_id: Option<String>,
    #[serde(default)]
    pub runtime_history: Vec<WorkspaceTaskRuntimeHistoryEntry>,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub started_at: Option<u64>,
    #[serde(default)]
    pub completed_at: Option<u64>,
    #[serde(default)]
    pub deleted_at: Option<u64>,
    #[serde(default)]
    pub last_notice_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceTaskCreate {
    pub workspace_id: String,
    pub title: String,
    pub task_type: WorkspaceTaskType,
    pub description: String,
    #[serde(default)]
    pub definition_of_done: Option<String>,
    #[serde(default)]
    pub priority: Option<WorkspacePriority>,
    #[serde(default)]
    pub assignee: Option<WorkspaceActor>,
    #[serde(default)]
    pub reviewer: Option<WorkspaceActor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspaceTaskUpdate {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub definition_of_done: Option<Option<String>>,
    #[serde(default)]
    pub priority: Option<WorkspacePriority>,
    #[serde(default)]
    pub assignee: Option<Option<WorkspaceActor>>,
    #[serde(default)]
    pub reviewer: Option<Option<WorkspaceActor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceTaskMove {
    pub task_id: String,
    pub status: WorkspaceTaskStatus,
    #[serde(default)]
    pub sort_order: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceReviewVerdict {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceReviewSubmission {
    pub task_id: String,
    pub verdict: WorkspaceReviewVerdict,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceCompletionSubmission {
    pub task_id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceNotice {
    pub id: String,
    pub workspace_id: String,
    pub task_id: String,
    pub notice_type: String,
    pub message: String,
    #[serde(default)]
    pub actor: Option<WorkspaceActor>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueuedPromptRecord {
    pub id: String,
    pub thread_id: String,
    pub content: String,
    #[serde(default)]
    pub content_blocks_json: Option<String>,
    pub created_at: u64,
    pub position: i64,
}
