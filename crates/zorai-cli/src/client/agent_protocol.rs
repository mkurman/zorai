use anyhow::Result;
use serde::Deserialize;

fn default_requires_approval() -> bool {
    true
}

/// Gracefully deserialize context_messages. Malformed entries are dropped instead of failing the whole command.
fn deserialize_context_messages<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<zorai_protocol::AgentDbMessage>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let raw: Option<Vec<serde_json::Value>> = Option::deserialize(deserializer)?;
    match raw {
        None => Ok(None),
        Some(arr) => {
            let parsed: Vec<zorai_protocol::AgentDbMessage> = arr
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect();
            if parsed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(parsed))
            }
        }
    }
}

/// Commands for the agent bridge (JSON over stdin).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(super) enum AgentBridgeCommand {
    SendMessage {
        thread_id: Option<String>,
        content: String,
        session_id: Option<String>,
        #[serde(default, deserialize_with = "deserialize_context_messages")]
        context_messages: Option<Vec<zorai_protocol::AgentDbMessage>>,
        #[serde(default)]
        content_blocks_json: Option<String>,
        #[serde(default)]
        target_agent_id: Option<String>,
        #[serde(default)]
        workspace_context: Option<serde_json::Value>,
    },
    InternalDelegate {
        thread_id: Option<String>,
        target_agent_id: String,
        content: String,
        session_id: Option<String>,
    },
    ThreadParticipantCommand {
        thread_id: String,
        target_agent_id: String,
        action: String,
        instruction: Option<String>,
        session_id: Option<String>,
    },
    StopStream {
        thread_id: String,
    },
    RetryStreamNow {
        thread_id: String,
    },
    ListThreads {
        #[serde(default)]
        agent_filter: Option<String>,
        #[serde(default)]
        include_internal: bool,
    },
    GetThread {
        thread_id: String,
        #[serde(default)]
        message_limit: Option<usize>,
        #[serde(default)]
        message_offset: Option<usize>,
        #[serde(default)]
        collapse_tool_calls: bool,
    },
    DeleteThread {
        thread_id: String,
    },
    PinThreadMessageForCompaction {
        thread_id: String,
        message_id: String,
    },
    UnpinThreadMessageForCompaction {
        thread_id: String,
        message_id: String,
    },
    MessageFeedback {
        thread_id: String,
        message_id: String,
        #[serde(default)]
        reaction: Option<String>,
    },
    GetOperationStatus {
        operation_id: String,
    },
    AddTask {
        title: String,
        description: String,
        priority: Option<String>,
        command: Option<String>,
        session_id: Option<String>,
        scheduled_at: Option<u64>,
        #[serde(default)]
        dependencies: Vec<String>,
    },
    CancelTask {
        task_id: String,
    },
    ListTasks,
    ListRuns {
        #[serde(default)]
        parent_thread_id: Option<String>,
    },
    GetRun {
        run_id: String,
    },
    HandoffThread {
        thread_id: String,
        action: String,
        #[serde(default)]
        target_agent_id: Option<String>,
        reason: String,
        summary: String,
        requested_by: String,
        #[serde(default)]
        session_id: Option<String>,
    },
    StartGoalRun {
        goal: String,
        title: Option<String>,
        thread_id: Option<String>,
        session_id: Option<String>,
        priority: Option<String>,
        client_request_id: Option<String>,
        autonomy_level: Option<String>,
        #[serde(default = "default_requires_approval")]
        requires_approval: bool,
    },
    ListGoalRuns,
    GetGoalRun {
        goal_run_id: String,
    },
    ControlGoalRun {
        goal_run_id: String,
        action: String,
        step_index: Option<usize>,
        payload_json: Option<String>,
    },
    ListWorkspaceSettings,
    GetWorkspaceSettings {
        workspace_id: String,
    },
    SetWorkspaceOperator {
        workspace_id: String,
        operator: zorai_protocol::WorkspaceOperator,
    },
    SetWorkspaceRepoMonitor {
        workspace_id: String,
        repo_monitor_enabled: bool,
        #[serde(default)]
        repo_monitor_include_dirs: Vec<String>,
        #[serde(default)]
        repo_monitor_exclude_dirs: Vec<String>,
    },
    CreateWorkspaceTask {
        request: zorai_protocol::WorkspaceTaskCreate,
    },
    ListWorkspaceTasks {
        workspace_id: String,
        #[serde(default)]
        include_deleted: bool,
    },
    GetWorkspaceTask {
        task_id: String,
    },
    UpdateWorkspaceTask {
        task_id: String,
        update: zorai_protocol::WorkspaceTaskUpdate,
    },
    MoveWorkspaceTask {
        request: zorai_protocol::WorkspaceTaskMove,
    },
    RunWorkspaceTask {
        task_id: String,
    },
    PauseWorkspaceTask {
        task_id: String,
    },
    StopWorkspaceTask {
        task_id: String,
    },
    DeleteWorkspaceTask {
        task_id: String,
    },
    SubmitWorkspaceReview {
        review: zorai_protocol::WorkspaceReviewSubmission,
    },
    ListWorkspaceNotices {
        workspace_id: String,
        #[serde(default)]
        task_id: Option<String>,
    },
    ListTodos,
    GetTodos {
        thread_id: String,
    },
    GetWorkContext {
        thread_id: String,
    },
    GetFileOperationSnapshot {
        operation_id: String,
    },
    RevertFileOperation {
        operation_id: String,
    },
    GetThreadWorkspaceContext {
        thread_id: String,
    },
    SetThreadWorkspaceContext {
        thread_id: String,
        context: serde_json::Value,
    },
    SpawnSubagent {
        thread_id: String,
        args: serde_json::Value,
    },
    GetGitDiff {
        repo_path: String,
        file_path: Option<String>,
    },
    GetFilePreview {
        path: String,
        max_bytes: Option<usize>,
    },
    GetConfig,
    GetMlflowTracingStatus,
    TestMlflowTracingConnection,
    SendMlflowTracingTestTrace,
    ListMlflowTracingHeaders,
    SetMlflowTracingHeader {
        name: String,
        value: String,
    },
    DeleteMlflowTracingHeader {
        name: String,
    },
    GetGatewayConfig,
    ExternalRuntimeMigrationStatus,
    ExternalRuntimeMigrationPreview {
        runtime: String,
        config_path: Option<String>,
    },
    ExternalRuntimeMigrationApply {
        runtime: String,
        config_path: Option<String>,
        conflict_policy: String,
    },
    ExternalRuntimeMigrationReport {
        runtime: Option<String>,
        limit: Option<usize>,
    },
    ExternalRuntimeMigrationShadowRun {
        runtime: String,
    },
    SetConfigItem {
        key_path: String,
        value_json: String,
    },
    SetProviderModel {
        provider_id: String,
        model: String,
    },
    FetchModels {
        provider_id: String,
        base_url: String,
        api_key: String,
        output_modalities: Option<String>,
    },
    SetTargetAgentProviderModel {
        target_agent_id: String,
        provider_id: String,
        model: String,
    },
    SetTargetAgentReasoningEffort {
        target_agent_id: String,
        reasoning_effort: String,
    },
    SetTargetAgentContextWindow {
        target_agent_id: String,
        context_window_tokens: u32,
    },
    HeartbeatGetItems,
    HeartbeatSetItems {
        items_json: String,
    },
    ResolveTaskApproval {
        approval_id: String,
        decision: String,
    },
    ValidateProvider {
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    },
    GetProviderAuthStates,
    GetProviderCatalog,
    #[serde(rename = "openai-codex-auth-status")]
    GetOpenAICodexAuthStatus,
    #[serde(rename = "openai-codex-auth-login")]
    LoginOpenAICodex,
    #[serde(rename = "openai-codex-auth-logout")]
    LogoutOpenAICodex,
    LoginProvider {
        provider_id: String,
        api_key: String,
        #[serde(default)]
        base_url: String,
    },
    LogoutProvider {
        provider_id: String,
    },
    SetSubAgent {
        sub_agent_json: String,
    },
    RemoveSubAgent {
        sub_agent_id: String,
    },
    ListSubAgents,
    GetConciergeConfig,
    SetConciergeConfig {
        config_json: String,
    },
    DismissConciergeWelcome,
    RequestConciergeWelcome,
    AuditDismiss {
        entry_id: String,
    },
    QueryAudits {
        #[serde(default)]
        action_types: Option<Vec<String>>,
        #[serde(default)]
        since: Option<u64>,
        #[serde(default)]
        limit: Option<usize>,
    },
    GetProvenanceReport {
        #[serde(default)]
        limit: Option<u32>,
    },
    GetMemoryProvenanceReport {
        #[serde(default)]
        target: Option<String>,
        #[serde(default)]
        limit: Option<u32>,
    },
    ConfirmMemoryProvenanceEntry {
        entry_id: String,
    },
    RetractMemoryProvenanceEntry {
        entry_id: String,
    },
    GetCollaborationSessions {
        #[serde(default)]
        parent_task_id: Option<String>,
    },
    SendParticipantSuggestion {
        thread_id: String,
        suggestion_id: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        force_send: bool,
    },
    DismissParticipantSuggestion {
        thread_id: String,
        suggestion_id: String,
        #[serde(default)]
        session_id: Option<String>,
    },
    VoteOnCollaborationDisagreement {
        parent_task_id: String,
        disagreement_id: String,
        task_id: String,
        position: String,
        #[serde(default)]
        confidence: Option<f64>,
    },
    GetStatus,
    InspectPrompt {
        #[serde(default)]
        agent_id: Option<String>,
    },
    SetTierOverride {
        tier: Option<String>,
    },
    #[serde(rename = "plugin-list")]
    PluginList,
    #[serde(rename = "plugin-get")]
    PluginGetDetail {
        name: String,
    },
    #[serde(rename = "plugin-enable")]
    PluginEnableCmd {
        name: String,
    },
    #[serde(rename = "plugin-disable")]
    PluginDisableCmd {
        name: String,
    },
    #[serde(rename = "plugin-install")]
    PluginInstallCmd {
        dir_name: String,
        install_source: String,
    },
    #[serde(rename = "plugin-uninstall")]
    PluginUninstallCmd {
        name: String,
    },
    #[serde(rename = "plugin-get-settings")]
    PluginGetSettings {
        name: String,
    },
    #[serde(rename = "plugin-update-settings")]
    PluginUpdateSettings {
        plugin_name: String,
        key: String,
        value: String,
        is_secret: bool,
    },
    #[serde(rename = "plugin-test-connection")]
    PluginTestConnection {
        name: String,
    },
    #[serde(rename = "plugin-oauth-start")]
    PluginOAuthStart {
        name: String,
    },
    WhatsAppLinkStart,
    WhatsAppLinkStop,
    WhatsAppLinkStatus,
    WhatsAppLinkSubscribe,
    WhatsAppLinkUnsubscribe,
    StartOperatorProfileSession {
        kind: String,
    },
    NextOperatorProfileQuestion {
        session_id: String,
    },
    SubmitOperatorProfileAnswer {
        session_id: String,
        question_id: String,
        answer_json: String,
    },
    SkipOperatorProfileQuestion {
        session_id: String,
        question_id: String,
        #[serde(default)]
        reason: Option<String>,
    },
    DeferOperatorProfileQuestion {
        session_id: String,
        question_id: String,
        #[serde(default)]
        defer_until_unix_ms: Option<u64>,
    },
    #[serde(rename = "answer-question")]
    AnswerQuestion {
        question_id: String,
        answer: String,
    },
    GetOperatorProfileSummary,
    SetOperatorProfileConsent {
        consent_key: String,
        granted: bool,
    },
    ExplainAction {
        action_id: String,
        #[serde(default)]
        step_index: Option<usize>,
    },
    StartDivergentSession {
        problem_statement: String,
        thread_id: String,
        #[serde(default)]
        goal_run_id: Option<String>,
        #[serde(default)]
        custom_framings_json: Option<String>,
    },
    GetDivergentSession {
        session_id: String,
    },
    ListGeneratedTools,
    RunGeneratedTool {
        tool_name: String,
        args_json: String,
    },
    SpeechToText {
        args_json: String,
    },
    TextToSpeech {
        args_json: String,
    },
    PromoteGeneratedTool {
        tool_name: String,
    },
    ActivateGeneratedTool {
        tool_name: String,
    },
    RetireGeneratedTool {
        tool_name: String,
    },
    #[serde(rename = "agent-get-statistics")]
    GetStatistics {
        window: zorai_protocol::AgentStatisticsWindow,
        #[serde(default)]
        session_limit: Option<usize>,
        #[serde(default)]
        session_offset: Option<usize>,
        #[serde(default)]
        sessions_only: bool,
    },
    EnqueuePrompt {
        thread_id: String,
        content: String,
        #[serde(default)]
        content_blocks_json: Option<String>,
        #[serde(default)]
        prompt_id: Option<String>,
    },
    ListPromptQueue {
        #[serde(default)]
        thread_id: Option<String>,
    },
    UpdateQueuedPrompt {
        thread_id: String,
        prompt_id: String,
        content: String,
        #[serde(default)]
        content_blocks_json: Option<String>,
    },
    CancelQueuedPrompt {
        thread_id: String,
        prompt_id: String,
    },
    SendQueuedPromptNow {
        thread_id: String,
        prompt_id: String,
    },
    GetThreadExecutionProfile {
        thread_id: String,
    },
    SetThreadExecutionProfile {
        thread_id: String,
        profile_json: String,
    },
    ForceCompact {
        thread_id: String,
    },
    Shutdown,
}
