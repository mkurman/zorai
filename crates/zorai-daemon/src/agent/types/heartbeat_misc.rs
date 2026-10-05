use super::*;
use serde::{Deserialize, Serialize};

use crate::agent::llm_client::OpenAiResponsesTerminalResponse;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HeartbeatOutcome {
    Ok,
    Alert,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatItem {
    pub id: String,
    pub label: String,
    pub prompt: String,
    #[serde(default = "default_zero")]
    pub interval_minutes: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_result: Option<HeartbeatOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    #[serde(default)]
    pub notify_on_alert: bool,
    #[serde(default)]
    pub notify_channels: Vec<String>,
}

fn default_zero() -> u64 {
    0
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HeartbeatCheckType {
    StaleTodos,
    StuckGoalRuns,
    UnrepliedGatewayMessages,
    RepoChanges,
    PluginAuth,
    SkillLifecycle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckDetail {
    pub id: String,
    pub label: String,
    pub age_hours: f64,
    pub severity: CheckSeverity,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatCheckResult {
    pub check_type: HeartbeatCheckType,
    pub items_found: usize,
    pub summary: String,
    pub details: Vec<CheckDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatDigestItem {
    pub priority: u8,
    pub check_type: HeartbeatCheckType,
    pub title: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatChecksConfig {
    #[serde(default = "default_true")]
    pub stale_todos_enabled: bool,
    #[serde(default = "default_stale_todo_threshold_hours")]
    pub stale_todo_threshold_hours: u64,
    #[serde(default = "default_true")]
    pub stuck_goals_enabled: bool,
    #[serde(default = "default_stuck_goal_threshold_hours")]
    pub stuck_goal_threshold_hours: u64,
    #[serde(default = "default_true")]
    pub unreplied_messages_enabled: bool,
    #[serde(default = "default_unreplied_threshold_hours")]
    pub unreplied_message_threshold_hours: u64,
    #[serde(default = "default_true")]
    pub repo_changes_enabled: bool,
    #[serde(default = "default_true")]
    pub plugin_auth_enabled: bool,
    #[serde(default)]
    pub stale_todos_cron: Option<String>,
    #[serde(default)]
    pub stuck_goals_cron: Option<String>,
    #[serde(default)]
    pub unreplied_messages_cron: Option<String>,
    #[serde(default)]
    pub repo_changes_cron: Option<String>,
    #[serde(default)]
    pub plugin_auth_cron: Option<String>,
    #[serde(default = "default_priority_weight")]
    pub stale_todos_priority_weight: f64,
    #[serde(default = "default_priority_weight")]
    pub stuck_goals_priority_weight: f64,
    #[serde(default = "default_priority_weight")]
    pub unreplied_messages_priority_weight: f64,
    #[serde(default = "default_priority_weight")]
    pub repo_changes_priority_weight: f64,
    #[serde(default = "default_priority_weight")]
    pub plugin_auth_priority_weight: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_todos_priority_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stuck_goals_priority_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreplied_messages_priority_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_changes_priority_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_auth_priority_override: Option<f64>,
    /// Per D-11: Global reset action — when true, resets all learned priority weights to 1.0.
    #[serde(default)]
    pub reset_learned_priorities: bool,
}

fn default_priority_weight() -> f64 {
    1.0
}

fn default_stale_todo_threshold_hours() -> u64 {
    24
}
fn default_stuck_goal_threshold_hours() -> u64 {
    2
}
fn default_unreplied_threshold_hours() -> u64 {
    1
}

/// Which action types to include in the audit feed. Per D-05.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditScopeConfig {
    #[serde(default = "default_true")]
    pub heartbeat: bool,
    #[serde(default = "default_true")]
    pub tool: bool,
    #[serde(default = "default_true")]
    pub escalation: bool,
    #[serde(default = "default_true")]
    pub skill: bool,
    #[serde(default = "default_true")]
    pub subagent: bool,
}

impl Default for AuditScopeConfig {
    fn default() -> Self {
        Self {
            heartbeat: true,
            tool: true,
            escalation: true,
            skill: true,
            subagent: true,
        }
    }
}

/// Audit trail configuration: scope, confidence thresholds, retention. Per D-05/D-10.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Which action types to include in the audit feed.
    #[serde(default)]
    pub scope: AuditScopeConfig,
    /// Confidence threshold below which to show qualifiers. Per D-10.
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
    /// Whether to always show confidence (overrides threshold). Per D-10.
    #[serde(default)]
    pub always_show_confidence: bool,
    /// Maximum audit entries to retain. Per Pitfall 4.
    #[serde(default = "default_max_audit_entries")]
    pub max_entries: usize,
    /// Maximum age of audit entries in days.
    #[serde(default = "default_max_audit_age_days")]
    pub max_age_days: u32,
}

fn default_confidence_threshold() -> f64 {
    0.80
}
fn default_max_audit_entries() -> usize {
    10_000
}
fn default_max_audit_age_days() -> u32 {
    30
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            scope: AuditScopeConfig::default(),
            confidence_threshold: default_confidence_threshold(),
            always_show_confidence: false,
            max_entries: default_max_audit_entries(),
            max_age_days: default_max_audit_age_days(),
        }
    }
}

impl Default for HeartbeatChecksConfig {
    fn default() -> Self {
        Self {
            stale_todos_enabled: true,
            stale_todo_threshold_hours: default_stale_todo_threshold_hours(),
            stuck_goals_enabled: true,
            stuck_goal_threshold_hours: default_stuck_goal_threshold_hours(),
            unreplied_messages_enabled: true,
            unreplied_message_threshold_hours: default_unreplied_threshold_hours(),
            repo_changes_enabled: true,
            plugin_auth_enabled: true,
            stale_todos_cron: None,
            stuck_goals_cron: None,
            unreplied_messages_cron: None,
            repo_changes_cron: None,
            plugin_auth_cron: None,
            stale_todos_priority_weight: default_priority_weight(),
            stuck_goals_priority_weight: default_priority_weight(),
            unreplied_messages_priority_weight: default_priority_weight(),
            repo_changes_priority_weight: default_priority_weight(),
            plugin_auth_priority_weight: default_priority_weight(),
            stale_todos_priority_override: None,
            stuck_goals_priority_override: None,
            unreplied_messages_priority_override: None,
            repo_changes_priority_override: None,
            plugin_auth_priority_override: None,
            reset_learned_priorities: false,
        }
    }
}

/// Convert legacy heartbeat_interval_mins to a cron expression. Per D-08.
pub fn interval_mins_to_cron(mins: u64) -> String {
    match mins {
        0 | 1 => "* * * * *".to_string(),
        m if m <= 59 && 60 % m == 0 => format!("*/{} * * * *", m),
        60 => "0 * * * *".to_string(),
        m if m > 60 && m % 60 == 0 => format!("0 */{} * * *", m / 60),
        m => format!("*/{} * * * *", m.min(59)),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationSeverity {
    Info,
    Warning,
    Alert,
    Error,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMemory {
    pub soul: String,
    pub memory: String,
    pub user_profile: String,
}

/// Compute tokens-per-second and generation duration from timing data.
/// Compute generation_ms and tokens-per-second from the elapsed duration and
/// output token count. Pass `first_token_at.unwrap_or(started_at).elapsed()`
/// as `generation_secs`.
pub fn compute_generation_stats(
    generation_secs: f64,
    output_tokens: u64,
) -> (Option<u64>, Option<f64>) {
    let generation_ms = Some((generation_secs * 1000.0).round() as u64);
    let tps = if output_tokens > 0 && generation_secs > 0.0 {
        Some(output_tokens as f64 / generation_secs)
    } else {
        None
    };
    (generation_ms, tps)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionServerToolUsage {
    pub web_fetch_requests: Option<u64>,
    pub web_search_requests: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionContainerInfo {
    pub id: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionUpstreamContentBlock {
    pub block_type: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub text: Option<String>,
    pub thinking: Option<String>,
    pub signature: Option<String>,
    pub input_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionUpstreamMessage {
    pub id: Option<String>,
    pub message_type: Option<String>,
    pub role: Option<String>,
    pub model: Option<String>,
    pub container: Option<CompletionContainerInfo>,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub content_blocks: Vec<CompletionUpstreamContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionOpenAiResponsesFinalResult {
    pub id: Option<String>,
    pub output_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing, default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) response: Option<OpenAiResponsesTerminalResponse>,
    #[serde(skip)]
    pub response_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionOpenAiChatCompletionsFinalResult {
    pub id: Option<String>,
    pub model: Option<String>,
    pub output_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing, default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum CompletionProviderFinalResult {
    AnthropicMessage(CompletionUpstreamMessage),
    OpenAiChatCompletions(CompletionOpenAiChatCompletionsFinalResult),
    OpenAiResponses(CompletionOpenAiResponsesFinalResult),
}

pub fn apply_completion_cache_usage(
    upstream_message: &mut Option<CompletionUpstreamMessage>,
    provider_final_result: &mut Option<CompletionProviderFinalResult>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
) {
    if let Some(message) = upstream_message.as_mut() {
        message.apply_cache_usage(cache_creation_input_tokens, cache_read_input_tokens);
    }
    if let Some(result) = provider_final_result.as_mut() {
        result.apply_cache_usage(cache_creation_input_tokens, cache_read_input_tokens);
    }
}

impl CompletionUpstreamMessage {
    fn apply_cache_usage(
        &mut self,
        cache_creation_input_tokens: Option<u64>,
        cache_read_input_tokens: Option<u64>,
    ) {
        if cache_creation_input_tokens.is_some() {
            self.cache_creation_input_tokens = cache_creation_input_tokens;
        }
        if cache_read_input_tokens.is_some() {
            self.cache_read_input_tokens = cache_read_input_tokens;
        }
    }

    pub(crate) fn cache_usage_tokens(&self) -> (u64, u64) {
        (
            self.cache_read_input_tokens.unwrap_or(0),
            self.cache_creation_input_tokens.unwrap_or(0),
        )
    }
}

impl CompletionProviderFinalResult {
    fn apply_cache_usage(
        &mut self,
        cache_creation_input_tokens: Option<u64>,
        cache_read_input_tokens: Option<u64>,
    ) {
        match self {
            Self::AnthropicMessage(message) => {
                message.apply_cache_usage(cache_creation_input_tokens, cache_read_input_tokens);
            }
            Self::OpenAiChatCompletions(result) => {
                if cache_creation_input_tokens.is_some() {
                    result.cache_creation_input_tokens = cache_creation_input_tokens;
                }
                if cache_read_input_tokens.is_some() {
                    result.cache_read_input_tokens = cache_read_input_tokens;
                }
            }
            Self::OpenAiResponses(result) => {
                if cache_creation_input_tokens.is_some() {
                    result.cache_creation_input_tokens = cache_creation_input_tokens;
                }
                if cache_read_input_tokens.is_some() {
                    result.cache_read_input_tokens = cache_read_input_tokens;
                }
            }
        }
    }

    pub(crate) fn cache_usage_tokens(&self) -> (u64, u64) {
        match self {
            Self::AnthropicMessage(message) => message.cache_usage_tokens(),
            Self::OpenAiChatCompletions(result) => (
                result.cache_read_input_tokens.unwrap_or(0),
                result.cache_creation_input_tokens.unwrap_or(0),
            ),
            Self::OpenAiResponses(result) => {
                let cache_read = result.cache_read_input_tokens.or_else(|| {
                    result
                        .response
                        .as_ref()
                        .and_then(|response| response.usage.input_tokens_details.as_ref())
                        .and_then(|details| details.cached_tokens)
                });
                (
                    cache_read.unwrap_or(0),
                    result.cache_creation_input_tokens.unwrap_or(0),
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum CompletionChunk {
    Delta {
        content: String,
        reasoning: Option<String>,
    },
    ToolCalls {
        tool_calls: Vec<ToolCall>,
        content: Option<String>,
        reasoning: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        stop_reason: Option<String>,
        stop_sequence: Option<String>,
        response_id: Option<String>,
        request_id: Option<String>,
        upstream_model: Option<String>,
        upstream_role: Option<String>,
        upstream_message_type: Option<String>,
        upstream_container: Option<CompletionContainerInfo>,
        upstream_message: Option<CompletionUpstreamMessage>,
        provider_final_result: Option<CompletionProviderFinalResult>,
        upstream_thread_id: Option<String>,
        cache_creation_input_tokens: Option<u64>,
        cache_read_input_tokens: Option<u64>,
        server_tool_use: Option<CompletionServerToolUsage>,
    },
    Done {
        content: String,
        reasoning: Option<String>,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: Option<f64>,
        stop_reason: Option<String>,
        stop_sequence: Option<String>,
        cache_creation_input_tokens: Option<u64>,
        cache_read_input_tokens: Option<u64>,
        server_tool_use: Option<CompletionServerToolUsage>,
        response_id: Option<String>,
        request_id: Option<String>,
        upstream_model: Option<String>,
        upstream_role: Option<String>,
        upstream_message_type: Option<String>,
        upstream_container: Option<CompletionContainerInfo>,
        upstream_message: Option<CompletionUpstreamMessage>,
        provider_final_result: Option<CompletionProviderFinalResult>,
        upstream_thread_id: Option<String>,
    },
    TransportFallback {
        from: ApiTransport,
        to: ApiTransport,
        message: String,
    },
    Retry {
        attempt: u32,
        max_retries: u32,
        delay_ms: u64,
        failure_class: String,
        message: String,
    },
    Error {
        message: String,
    },
}
