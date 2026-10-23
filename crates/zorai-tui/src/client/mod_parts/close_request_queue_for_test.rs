use serde::Deserialize;
use serde_json::Value;
use std::sync::Mutex;
use tokio::sync::mpsc;

#[cfg(not(unix))]
use zorai_protocol::default_tcp_addr;
use zorai_protocol::ClientMessage;

use crate::wire::{
    AgentConfigSnapshot, AgentTask, AgentThread, AnticipatoryItem, CheckpointSummary, FetchedModel,
    GoalRun, HeartbeatItem, ThreadParticipantSuggestion, ThreadWorkContext,
};

#[derive(Debug, Clone)]
pub struct WelesReviewMetaVm {
    pub weles_reviewed: bool,
    pub verdict: String,
    pub reasons: Vec<String>,
    pub audit_id: Option<String>,
    pub security_override_mode: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WelesHealthVm {
    pub state: String,
    pub reason: Option<String>,
    pub checked_at: u64,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct GatewayChatVm {
    pub channel_key: String,
    pub platform: String,
    pub channel_id: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub thread_title: Option<String>,
    #[serde(default)]
    pub last_thread_id: String,
    #[serde(default)]
    pub last_thread_title: Option<String>,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ThreadSearchResultVm {
    pub thread_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ThreadSearchResultsVm {
    pub query: String,
    #[serde(default)]
    pub semantic: bool,
    #[serde(default)]
    pub semantic_error: Option<String>,
    pub results: Vec<ThreadSearchResultVm>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ThreadMessagePinResultVm {
    pub ok: bool,
    pub thread_id: String,
    pub message_id: String,
    #[serde(default)]
    pub error: Option<String>,
    pub current_pinned_chars: usize,
    pub pinned_budget_chars: usize,
    #[serde(default)]
    pub candidate_pinned_chars: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenAICodexAuthStatusVm {
    pub available: bool,
    pub auth_mode: Option<String>,
    pub account_id: Option<String>,
    pub expires_at: Option<i64>,
    pub source: Option<String>,
    pub error: Option<String>,
    pub auth_url: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ClientEvent {
    Connected,
    Disconnected,
    Reconnecting {
        delay_secs: u64,
    },
    SessionSpawned {
        session_id: String,
    },

    ThreadList(Vec<AgentThread>),
    ThreadDetail(Option<AgentThread>),
    ThreadCreated {
        thread_id: String,
        title: String,
        agent_name: Option<String>,
    },
    ThreadTitleUpdated {
        thread_id: String,
        title: String,
    },
    ThreadDeleted {
        thread_id: String,
        deleted: bool,
    },
    ThreadExported {
        thread_id: String,
        file_path: String,
    },
    ThreadMessagePinResult(ThreadMessagePinResultVm),
    ThreadReloadRequired {
        thread_id: String,
    },
    ThreadHandoffResult(zorai_protocol::ThreadHandoffResult),
    GatewayChats(Vec<GatewayChatVm>),
    ThreadSearchResults(ThreadSearchResultsVm),
    MessageFeedbackUpdated {
        thread_id: String,
        message_id: String,
        reaction: Option<zorai_protocol::Reaction>,
    },
    ContextWindowUpdate {
        thread_id: String,
        active_context_window_start: usize,
        active_context_window_end: usize,
        active_context_window_tokens: u64,
    },
    ParticipantSuggestion {
        thread_id: String,
        suggestion: ThreadParticipantSuggestion,
    },
    TaskList(Vec<AgentTask>),
    TaskUpdate(AgentTask),
    GoalRunList(Vec<GoalRun>),
    GoalRunStarted(GoalRun),
    GoalRunDetail(Option<GoalRun>),
    GoalRunUpdate(GoalRun),
    GoalRunControlled {
        goal_run_id: String,
        ok: bool,
    },
    GoalRunDeleted {
        goal_run_id: String,
        deleted: bool,
    },
    GoalRunCheckpoints {
        goal_run_id: String,
        checkpoints: Vec<CheckpointSummary>,
    },
    GoalHydrationScheduleFailed {
        goal_run_id: String,
    },
    AgentExplanation(serde_json::Value),
    DivergentSessionStarted(serde_json::Value),
    DivergentSession(serde_json::Value),
    ThreadTodos {
        thread_id: String,
        goal_run_id: Option<String>,
        step_index: Option<usize>,
        items: Vec<crate::wire::TodoItem>,
    },
    WorkContext(ThreadWorkContext),
    GitDiff {
        repo_path: String,
        file_path: Option<String>,
        diff: String,
    },
    FilePreview {
        path: String,
        content: String,
        truncated: bool,
        is_text: bool,
    },
    AgentConfig(AgentConfigSnapshot),
    AgentConfigRaw(Value),
    MlflowTracingStatus(Value),
    MlflowTracingTestResult(Value),
    ExternalRuntimeMigrationResult(Value),
    ModelsFetched(Vec<FetchedModel>),
    HeartbeatItems(Vec<HeartbeatItem>),
    HeartbeatDigest {
        cycle_id: String,
        actionable: bool,
        digest: String,
        items: Vec<(u8, String, String, String)>,
        checked_at: u64,
        explanation: Option<String>,
    },
    AuditEntry {
        id: String,
        timestamp: u64,
        action_type: String,
        summary: String,
        explanation: Option<String>,
        confidence: Option<f64>,
        confidence_band: Option<String>,
        causal_trace_id: Option<String>,
        thread_id: Option<String>,
    },
    EscalationUpdate {
        thread_id: String,
        from_level: String,
        to_level: String,
        reason: String,
        attempts: u32,
        audit_id: Option<String>,
    },
    AnticipatoryItems(Vec<AnticipatoryItem>),
    GatewayStatus {
        platform: String,
        status: String,
        last_error: Option<String>,
        consecutive_failures: u32,
    },
    WhatsAppLinkStatus {
        state: String,
        phone: Option<String>,
        last_error: Option<String>,
    },
    WhatsAppLinkQr {
        ascii_qr: String,
        expires_at_ms: Option<u64>,
    },
    WhatsAppLinked {
        phone: Option<String>,
    },
    WhatsAppLinkError {
        message: String,
        recoverable: bool,
    },
    WhatsAppLinkDisconnected {
        reason: Option<String>,
    },

    Delta {
        thread_id: String,
        content: String,
    },
    Reasoning {
        thread_id: String,
        content: String,
    },
    ToolCall {
        thread_id: String,
        call_id: String,
        name: String,
        arguments: String,
        weles_review: Option<WelesReviewMetaVm>,
        message_id: Option<String>,
    },
    ToolResult {
        thread_id: String,
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
        weles_review: Option<WelesReviewMetaVm>,
        message_id: Option<String>,
    },
    Done {
        thread_id: String,
        input_tokens: u64,
        output_tokens: u64,
        cost: Option<f64>,
        provider: Option<String>,
        model: Option<String>,
        tps: Option<f64>,
        generation_ms: Option<u64>,
        reasoning: Option<String>,
        provider_final_result_json: Option<String>,
        message_id: Option<String>,
    },
    WorkflowNotice {
        thread_id: Option<String>,
        kind: String,
        message: String,
        details: Option<String>,
    },
    WelesHealthUpdate {
        state: String,
        reason: Option<String>,
        checked_at: u64,
    },
    RetryStatus {
        thread_id: String,
        phase: String,
        attempt: u32,
        max_retries: u32,
        delay_ms: u64,
        failure_class: String,
        message: String,
    },
    ApprovalRequired {
        approval_id: String,
        command: String,
        rationale: Option<String>,
        reasons: Vec<String>,
        risk_level: String,
        blast_radius: String,
    },
    ApprovalResolved {
        approval_id: String,
        decision: String,
    },
    TaskApprovalRules(Vec<zorai_protocol::TaskApprovalRule>),

    ProviderAuthStates(Vec<crate::state::ProviderAuthEntry>),
    OpenAICodexAuthStatus(OpenAICodexAuthStatusVm),
    OpenAICodexAuthLoginResult(OpenAICodexAuthStatusVm),
    OpenAICodexAuthLogoutResult {
        ok: bool,
        error: Option<String>,
    },
    ProviderValidation {
        provider_id: String,
        valid: bool,
        error: Option<String>,
    },
    SubAgentList(Vec<crate::state::SubAgentEntry>),
    SubAgentUpdated(crate::state::SubAgentEntry),
    SubAgentRemoved {
        sub_agent_id: String,
    },
    ConciergeConfig(Value),

    ConciergeWelcome {
        content: String,
        actions: Vec<crate::state::ConciergeActionVm>,
    },
    ConciergeWelcomeDismissed,
    OperatorProfileSessionStarted {
        session_id: String,
        kind: String,
    },
    OperatorProfileQuestion {
        session_id: String,
        question_id: String,
        field_key: String,
        prompt: String,
        input_kind: String,
        optional: bool,
    },
    OperatorQuestion {
        question_id: String,
        content: String,
        options: Vec<String>,
        session_id: Option<String>,
        thread_id: Option<String>,
    },
    OperatorQuestionResolved {
        question_id: String,
        answer: String,
    },
    OperatorProfileProgress {
        session_id: String,
        answered: u32,
        remaining: u32,
        completion_ratio: f64,
    },
    OperatorProfileSummary {
        summary_json: String,
    },
    OperatorModelSummary {
        model_json: String,
    },
    OperatorModelReset {
        ok: bool,
    },
    CollaborationSessions {
        sessions_json: String,
    },
    CollaborationVoteResult {
        report_json: String,
    },
    GeneratedTools {
        tools_json: String,
    },
    SpeechToTextResult {
        content: String,
    },
    TextToSpeechResult {
        content: String,
    },
    GenerateImageResult {
        content: String,
    },
    OperatorProfileSessionCompleted {
        session_id: String,
        updated_fields: Vec<String>,
    },
    StatusDiagnostics {
        operator_profile_sync_state: String,
        operator_profile_sync_dirty: bool,
        operator_profile_scheduler_fallback: bool,
        diagnostics_json: String,
    },
    StatusSnapshot(AgentStatusSnapshotVm),
    StatisticsSnapshot(zorai_protocol::AgentStatisticsSnapshot),
    PromptInspection(AgentPromptInspectionVm),

    TierChanged {
        new_tier: String,
    },

    SemanticIndexRepaired {
        summary: String,
    },

    PluginList(Vec<zorai_protocol::PluginInfo>),
    PluginGet {
        plugin: Option<zorai_protocol::PluginInfo>,
        settings_schema: Option<String>,
    },
    PluginSettings {
        plugin_name: String,
        settings: Vec<(String, String, bool)>,
    },
    PluginTestConnection {
        plugin_name: String,
        success: bool,
        message: String,
    },
    PluginAction {
        success: bool,
        message: String,
    },
    PluginCommands(Vec<zorai_protocol::PluginCommandInfo>),
    PluginOAuthUrl {
        name: String,
        url: String,
    },
    PluginOAuthComplete {
        name: String,
        success: bool,
        error: Option<String>,
    },
    NotificationSnapshot(Vec<zorai_protocol::InboxNotification>),
    NotificationUpsert(zorai_protocol::InboxNotification),
    WorkspaceSettings(zorai_protocol::WorkspaceSettings),
    WorkspaceSettingsList(Vec<zorai_protocol::WorkspaceSettings>),
    WorkspaceTaskList {
        workspace_id: String,
        tasks: Vec<zorai_protocol::WorkspaceTask>,
    },
    WorkspaceTaskUpdated(zorai_protocol::WorkspaceTask),
    WorkspaceTaskDeleted {
        task_id: String,
        deleted_at: Option<u64>,
    },
    WorkspaceNotices {
        workspace_id: String,
        notices: Vec<zorai_protocol::WorkspaceNotice>,
    },
    WorkspaceNoticeUpdated(zorai_protocol::WorkspaceNotice),

    DatabaseSyncResult {
        ok: bool,
        message: String,
    },
    DatabaseBackendState {
        backend: Option<String>,
        sync_url: Option<String>,
        has_token: bool,
        seeded_at: Option<i64>,
    },

    Error(String),
    PromptQueue {
        thread_id: Option<String>,
        prompts: Vec<zorai_protocol::QueuedPromptRecord>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatusSnapshotVm {
    pub tier: String,
    pub activity: String,
    pub active_thread_id: Option<String>,
    pub active_goal_run_id: Option<String>,
    pub active_goal_run_title: Option<String>,
    pub provider_health_json: String,
    pub gateway_statuses_json: String,
    pub recent_actions_json: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AgentPromptInspectionSectionVm {
    pub id: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AgentPromptToolInspectionVm {
    pub name: String,
    pub serialized: String,
    pub serialized_chars: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AgentPromptInspectionVm {
    pub agent_id: String,
    pub agent_name: String,
    pub provider_id: String,
    pub model: String,
    pub sections: Vec<AgentPromptInspectionSectionVm>,
    pub final_prompt: String,
    #[serde(default)]
    pub tools: Vec<AgentPromptToolInspectionVm>,
}

pub struct DaemonClient {
    pub(crate) event_tx: mpsc::Sender<ClientEvent>,
    pub(crate) request_tx: mpsc::UnboundedSender<ClientMessage>,
    pub(crate) request_rx: Mutex<Option<mpsc::UnboundedReceiver<ClientMessage>>>,
}

#[cfg(test)]
impl DaemonClient {
    pub(crate) fn close_request_queue_for_test(&self) {
        self.request_rx
            .lock()
            .expect("request mutex poisoned")
            .take();
    }
}
