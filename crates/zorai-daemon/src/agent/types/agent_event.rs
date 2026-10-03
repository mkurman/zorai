use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weles_review: Option<WelesReviewMeta>,
        /// Persisted `agent_messages.id` of the tool-call row, when known.
        /// Frontends use this to attach per-message actions (e.g. 👍/👎
        /// feedback) to the row the daemon actually stored, instead of a
        /// client-local placeholder id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_input_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_output_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_cost_usd: Option<f64>,
    },
    ToolResult {
        thread_id: String,
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weles_review: Option<WelesReviewMeta>,
        /// Persisted `agent_messages.id` of the tool-result row, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },
    ApprovalRequired {
        thread_id: String,
        approval_id: String,
        command: String,
        rationale: Option<String>,
        reasons: Vec<String>,
        risk_level: String,
        blast_radius: String,
    },
    Done {
        thread_id: String,
        input_tokens: u64,
        output_tokens: u64,
        cost: Option<f64>,
        provider: Option<String>,
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tps: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        generation_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        upstream_message: Option<CompletionUpstreamMessage>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_final_result: Option<CompletionProviderFinalResult>,
        /// Persisted `agent_messages.id` of the assistant message that just
        /// finished streaming. None when finalization happens before the
        /// daemon has assigned a row (e.g. fatal error mid-stream).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },
    Error {
        thread_id: String,
        message: String,
    },
    TurnInterrupted {
        thread_id: String,
    },
    ThreadCreated {
        thread_id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_name: Option<String>,
    },
    ThreadTitleUpdated {
        thread_id: String,
        title: String,
    },
    ThreadReloadRequired {
        thread_id: String,
    },
    ContextWindowUpdate {
        thread_id: String,
        active_context_window_start: usize,
        active_context_window_end: usize,
        active_context_window_tokens: u64,
    },
    ParticipantSuggestion {
        thread_id: String,
        suggestion: crate::agent::ThreadParticipantSuggestion,
    },
    TaskUpdate {
        task_id: String,
        status: TaskStatus,
        progress: u8,
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        task: Option<AgentTask>,
    },
    GoalRunUpdate {
        goal_run_id: String,
        status: GoalRunStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        current_step_index: Option<usize>,
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        goal_run: Option<GoalRun>,
    },
    TodoUpdate {
        thread_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        goal_run_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        step_index: Option<usize>,
        items: Vec<TodoItem>,
    },
    WorkContextUpdate {
        thread_id: String,
        context: ThreadWorkContext,
    },
    WorkflowNotice {
        thread_id: String,
        kind: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
    },
    WelesHealthUpdate {
        state: WelesHealthState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
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
    AnticipatoryUpdate {
        items: Vec<AnticipatoryItem>,
    },
    HeartbeatResult {
        item_id: String,
        result: HeartbeatOutcome,
        message: String,
    },
    /// Prioritized heartbeat digest from structured checks + LLM synthesis. Per D-11.
    HeartbeatDigest {
        cycle_id: String,
        actionable: bool,
        digest: String,
        items: Vec<HeartbeatDigestItem>,
        checked_at: u64,
        /// Inline explanation for the overall digest. Per D-01.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        explanation: Option<String>,
        /// Confidence of the overall assessment (0.0..1.0). Per D-01/D-09.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    Notification {
        title: String,
        body: String,
        severity: NotificationSeverity,
        channels: Vec<String>,
    },
    NotificationInboxUpsert {
        notification: zorai_protocol::InboxNotification,
    },
    WorkspaceSettingsUpdate {
        settings: zorai_protocol::WorkspaceSettings,
    },
    WorkspaceTaskUpdate {
        task: zorai_protocol::WorkspaceTask,
    },
    WorkspaceTaskDeleted {
        task_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deleted_at: Option<u64>,
    },
    WorkspaceNoticeUpdate {
        notice: zorai_protocol::WorkspaceNotice,
    },
    /// Request to send a message via a gateway platform (Slack/Discord/Telegram/WhatsApp).
    GatewaySend {
        platform: String,
        target: String,
        message: String,
    },
    /// Execute a workspace UI command on the frontend.
    WorkspaceCommand {
        command: String,
        args: serde_json::Value,
    },
    /// Incoming message from a gateway platform (for frontend display).
    GatewayIncoming {
        platform: String,
        sender: String,
        content: String,
        channel: String,
        /// Daemon thread already bound to this gateway channel, when known.
        /// Absent on the first message after `!new` / `!reset` until a new thread is created.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
    },
    /// Gateway platform connection status change (per D-05/GATE-05).
    GatewayStatus {
        platform: String,
        /// Serialized status: "connected", "disconnected", "error".
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        consecutive_failures: Option<u32>,
    },
    WhatsAppLinkStatus {
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phone: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_error: Option<String>,
    },
    WhatsAppLinkQr {
        ascii_qr: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at_ms: Option<u64>,
    },
    WhatsAppLinked {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phone: Option<String>,
    },
    WhatsAppLinkError {
        message: String,
        recoverable: bool,
    },
    WhatsAppLinkDisconnected {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Sub-agent health state change detected by the supervisor.
    SubagentHealthChange {
        task_id: String,
        previous_state: SubagentHealthState,
        new_state: SubagentHealthState,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<StuckReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        intervention: Option<InterventionAction>,
    },
    /// A checkpoint was created for a goal run.
    CheckpointCreated {
        checkpoint_id: String,
        goal_run_id: String,
        checkpoint_type: String,
        step_index: Option<usize>,
    },
    ConciergeWelcome {
        thread_id: String,
        content: String,
        detail_level: ConciergeDetailLevel,
        actions: Vec<ConciergeAction>,
    },
    /// A provider's circuit breaker has tripped to Open state (per D-06).
    ProviderCircuitOpen {
        provider: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failed_model: Option<String>,
        trip_count: u32,
        #[serde(default = "default_provider_circuit_reason")]
        reason: String,
        #[serde(default)]
        suggested_alternatives: Vec<ProviderAlternativeSuggestion>,
    },
    /// A provider's circuit breaker has recovered to Closed state (per D-07).
    ProviderCircuitRecovered {
        provider: String,
    },
    /// Broadcast audit entry to all connected clients. Per D-06/TRNS-03.
    AuditAction {
        id: String,
        timestamp: u64,
        action_type: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        explanation: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence_band: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        causal_trace_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
    },
    /// Escalation level change notification. Per D-11/TRNS-05.
    EscalationUpdate {
        thread_id: String,
        from_level: String,
        to_level: String,
        reason: String,
        attempts: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        audit_id: Option<String>,
    },
    /// Capability tier changed notification (Phase 10).
    TierChanged {
        previous_tier: String,
        new_tier: String,
        reason: String,
    },
    /// An episode was recorded in episodic memory (Phase v3.0).
    EpisodeRecorded {
        episode_id: String,
        episode_type: String,
        outcome: String,
        summary: String,
    },
    /// Counter-who detected a repeated correction pattern (Phase v3.0).
    CounterWhoAlert {
        thread_id: String,
        pattern: String,
        attempt_count: u32,
        suggestion: String,
    },
    /// Trajectory update for a goal run or entity (Phase v3.0: AWAR-04).
    TrajectoryUpdate {
        goal_run_id: String,
        /// "converging", "diverging", or "stalled"
        direction: String,
        progress_ratio: f64,
        message: String,
    },
    /// Mode shift triggered after diminishing returns + counter-who confirmation (Phase v3.0: AWAR-02).
    ModeShift {
        thread_id: String,
        reason: String,
        previous_strategy: String,
        new_strategy: String,
    },
    /// Confidence warning for a planned or executing action (Phase v3.0: AWAR-05).
    ConfidenceWarning {
        thread_id: String,
        /// "plan_step" or "tool_call"
        action_type: String,
        /// "high", "medium", or "low"
        band: String,
        evidence: String,
        domain: String,
        blocked: bool,
    },
    /// Budget alert: cumulative goal run cost crossed the operator-defined threshold (COST-03).
    BudgetAlert {
        goal_run_id: String,
        current_cost_usd: f64,
        threshold_usd: f64,
    },
    /// Blocking operator-facing multiple-choice question requested by a tool or workflow.
    OperatorQuestion {
        question_id: String,
        content: String,
        options: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
    },
    /// Operator resolved a previously broadcast multiple-choice question.
    OperatorQuestionResolved {
        question_id: String,
        answer: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
    },
    /// Operator's reaction (thumbs up/down) on a specific assistant or tool
    /// message was set, switched, or cleared. Frontends mirror this to update
    /// per-message UI; the daemon feeds the same signal into the
    /// metacognitive self-assessment loop.
    MessageFeedbackUpdated {
        thread_id: String,
        message_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reaction: Option<zorai_protocol::Reaction>,
    },
    PromptQueueUpdate {
        thread_id: String,
        prompts: Vec<zorai_protocol::QueuedPromptRecord>,
    },
}
