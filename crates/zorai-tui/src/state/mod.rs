pub mod anticipatory;
pub mod approval;
pub mod audit;
pub mod auth;
pub mod chat;
pub mod collaboration;
pub mod concierge;
pub mod config;
pub mod goal_mission_control;
pub mod goal_sidebar;
pub mod goal_workspace;
pub mod input;
pub mod input_refs;
pub mod modal;
pub mod notifications;
pub mod settings;
pub mod sidebar;
pub mod spawned_tree;
pub mod statistics;
pub mod subagents;
pub mod task;
pub mod tier;
pub mod workspace;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusArea {
    Chat,
    Sidebar,
    Input,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum DaemonCommand {
    Refresh,
    /// Request an unbounded thread list, optionally narrowed to a specific
    /// agent filter. The TUI caches the result client-side and refreshes on
    /// demand.
    RefreshThreadsForAgent {
        agent_filter: Option<String>,
    },
    GetConfig,
    GetMlflowTracingStatus,
    TestMlflowTracingConnection,
    SendMlflowTracingTestTrace,
    RefreshServices,
    ListTasks,
    RequestThread {
        thread_id: String,
        message_limit: Option<usize>,
        message_offset: Option<usize>,
    },
    PinThreadMessageForCompaction {
        thread_id: String,
        message_id: String,
    },
    UnpinThreadMessageForCompaction {
        thread_id: String,
        message_id: String,
    },
    SubmitMessageFeedback {
        thread_id: String,
        message_id: String,
        absolute_message_index: Option<usize>,
        reaction: Option<zorai_protocol::Reaction>,
    },
    RequestThreadTodos(String),
    RequestThreadWorkContext(String),
    RequestGoalRunDetail(String),
    RequestGoalRunDetailPage {
        goal_run_id: String,
        step_offset: Option<usize>,
        step_limit: Option<usize>,
        event_offset: Option<usize>,
        event_limit: Option<usize>,
    },
    RequestGoalRunCheckpoints(String),
    ScheduleGoalHydrationRefresh(String),
    StartGoalRun {
        goal: String,
        thread_id: Option<String>,
        session_id: Option<String>,
        launch_assignments: Vec<crate::state::task::GoalAgentAssignment>,
    },
    ExplainAction {
        action_id: String,
        step_index: Option<usize>,
    },
    StartDivergentSession {
        problem_statement: String,
        thread_id: String,
        goal_run_id: Option<String>,
    },
    GetDivergentSession {
        session_id: String,
    },
    RequestGitDiff {
        repo_path: String,
        file_path: Option<String>,
    },
    RequestFilePreview {
        path: String,
        max_bytes: Option<usize>,
    },
    RequestAgentStatus,
    CancelTask {
        task_id: String,
    },
    RequestAgentStatistics {
        window: zorai_protocol::AgentStatisticsWindow,
    },
    RequestPromptInspection {
        agent_id: Option<String>,
    },
    SendMessage {
        thread_id: Option<String>,
        content: String,
        content_blocks_json: Option<String>,
        session_id: Option<String>,
        target_agent_id: Option<String>,
    },
    ForkThread {
        thread_id: String,
        thread_json: String,
        messages_json: Vec<String>,
        refresh_message_limit: usize,
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
    ThreadHandoff {
        thread_id: String,
        action: String,
        target_agent_id: Option<String>,
        reason: String,
        summary: String,
    },
    SendParticipantSuggestion {
        thread_id: String,
        suggestion_id: String,
    },
    DismissParticipantSuggestion {
        thread_id: String,
        suggestion_id: String,
    },
    StopStream {
        thread_id: String,
    },
    ForceCompact {
        thread_id: String,
    },
    RetryStreamNow {
        thread_id: String,
    },
    FetchModels {
        provider_id: String,
        base_url: String,
        api_key: String,
        output_modalities: Option<String>,
    },
    SetConfigItem {
        key_path: String,
        value_json: String,
    },
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
    SetProviderModel {
        provider_id: String,
        model: String,
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
    SetThreadExecutionProfile {
        thread_id: String,
        profile_json: String,
    },
    ControlGoalRun {
        goal_run_id: String,
        action: String,
        step_index: Option<usize>,
        payload_json: Option<String>,
    },
    DeleteGoalRun {
        goal_run_id: String,
    },
    ListTaskApprovalRules,
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
        repo_monitor_include_dirs: Vec<String>,
        repo_monitor_exclude_dirs: Vec<String>,
    },
    CreateWorkspaceTask(zorai_protocol::WorkspaceTaskCreate),
    ListWorkspaceTasks {
        workspace_id: String,
        include_deleted: bool,
    },
    ListWorkspaceNotices {
        workspace_id: String,
        task_id: Option<String>,
    },
    UpdateWorkspaceTask {
        task_id: String,
        update: zorai_protocol::WorkspaceTaskUpdate,
    },
    RunWorkspaceTask(String),
    PauseWorkspaceTask(String),
    StopWorkspaceTask(String),
    MoveWorkspaceTask(zorai_protocol::WorkspaceTaskMove),
    SubmitWorkspaceReview(zorai_protocol::WorkspaceReviewSubmission),
    DeleteWorkspaceTask(String),
    CreateTaskApprovalRule {
        approval_id: String,
    },
    RevokeTaskApprovalRule {
        rule_id: String,
    },
    ResolveTaskApproval {
        approval_id: String,
        decision: String,
    },
    SpawnSession {
        shell: Option<String>,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
    },
    GetProviderAuthStates,
    GetOpenAICodexAuthStatus,
    LoginOpenAICodex,
    LogoutOpenAICodex,
    LoginProvider {
        provider_id: String,
        api_key: String,
        base_url: String,
    },
    LogoutProvider {
        provider_id: String,
    },
    ValidateProvider {
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    },
    SetSubAgent(String),
    RemoveSubAgent(String),
    ListSubAgents,
    GetConciergeConfig,
    SetConciergeConfig(String),
    RequestConciergeWelcome,
    RetryOperatorProfile,
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
        reason: Option<String>,
    },
    DeferOperatorProfileQuestion {
        session_id: String,
        question_id: String,
        defer_until_unix_ms: Option<u64>,
    },
    AnswerOperatorQuestion {
        question_id: String,
        answer: String,
    },
    GetOperatorProfileSummary,
    GetOperatorModel,
    ResetOperatorModel,
    GetCollaborationSessions,
    VoteOnCollaborationDisagreement {
        parent_task_id: String,
        disagreement_id: String,
        task_id: String,
        position: String,
        confidence: Option<f64>,
    },
    GetGeneratedTools,
    SpeechToText {
        args_json: String,
    },
    TextToSpeech {
        args_json: String,
    },
    GenerateImage {
        args_json: String,
    },
    SetOperatorProfileConsent {
        consent_key: String,
        granted: bool,
    },
    DismissConciergeWelcome,
    WhatsAppLinkStart,
    WhatsAppLinkStop,
    WhatsAppLinkStatus,
    WhatsAppLinkSubscribe,
    WhatsAppLinkUnsubscribe,
    WhatsAppLinkReset,
    DatabaseSyncNow,
    DatabaseGetBackend,
    DeleteMessages {
        thread_id: String,
        message_ids: Vec<String>,
    },
    ExportThread {
        thread_id: String,
        message_id: String,
    },
    DeleteThread {
        thread_id: String,
    },
    RecordAttention {
        surface: String,
        thread_id: Option<String>,
        goal_run_id: Option<String>,
    },
    AuditDismiss {
        entry_id: String,
    },
    PluginList,
    PluginGet(String),
    PluginInstallSource(String),
    PluginEnable(String),
    PluginDisable(String),
    PluginGetSettings(String),
    PluginUpdateSetting {
        plugin_name: String,
        key: String,
        value: String,
        is_secret: bool,
    },
    PluginTestConnection(String),
    PluginListCommands,
    PluginOAuthStart(String),
    ListNotifications,
    UpsertNotification(zorai_protocol::InboxNotification),
    MarkAllNotificationsRead,
    ArchiveReadNotifications,
    EnqueuePrompt {
        thread_id: String,
        prompt_id: String,
        content: String,
        content_blocks_json: Option<String>,
    },
    ListPromptQueue {
        thread_id: Option<String>,
    },
    UpdateQueuedPrompt {
        thread_id: String,
        prompt_id: String,
        content: String,
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
}

#[allow(unused_imports)]
pub use anticipatory::{AnticipatoryAction, AnticipatoryState};
#[allow(unused_imports)]
pub use approval::{ApprovalAction, ApprovalFilter, ApprovalState, PendingApproval, RiskLevel};
#[allow(unused_imports)]
pub use audit::{AuditAction, AuditEntryVm, AuditState, EscalationVm, TimeRange};
#[allow(unused_imports)]
pub use auth::{AuthAction, AuthState, ProviderAuthEntry};
#[allow(unused_imports)]
pub use chat::{ChatAction, ChatState, ToolCallStatus, ToolCallVm, TranscriptMode};
#[allow(unused_imports)]
pub use collaboration::{
    CollaborationAction, CollaborationDisagreementVm, CollaborationEscalationVm,
    CollaborationPaneFocus, CollaborationRowVm, CollaborationSessionVm, CollaborationState,
};
#[allow(unused_imports)]
pub use concierge::{ConciergeAction, ConciergeActionVm, ConciergeState};
#[allow(unused_imports)]
pub use config::{ConfigAction, ConfigState};
#[allow(unused_imports)]
pub use goal_sidebar::{GoalSidebarState, GoalSidebarTab};
#[allow(unused_imports)]
pub use input::{InputAction, InputMode, InputState};
#[allow(unused_imports)]
pub use modal::{CommandItem, ModalAction, ModalKind, ModalState};
#[allow(unused_imports)]
pub use notifications::{NotificationsAction, NotificationsHeaderAction, NotificationsState};
#[allow(unused_imports)]
pub use settings::{SettingsAction, SettingsState, SettingsTab};
#[allow(unused_imports)]
pub use sidebar::{SidebarAction, SidebarItemTarget, SidebarState, SidebarTab};
#[allow(unused_imports)]
pub use subagents::{SubAgentEntry, SubAgentsAction, SubAgentsState};
#[allow(unused_imports)]
pub use task::{TaskAction, TaskState};
#[allow(unused_imports)]
pub use tier::TierState;

#[allow(dead_code)]
#[derive(Debug)]
pub enum AppAction {
    Chat(ChatAction),
    Task(TaskAction),
    Audit(AuditAction),
    Sidebar(SidebarAction),
    Input(InputAction),
    Modal(ModalAction),
    Config(ConfigAction),
    Approval(ApprovalAction),
    Settings(SettingsAction),
    Status(String),
    Focus(FocusArea),
    Resize { width: u16, height: u16 },
    Tick,
    Connected,
    Disconnected,
    Quit,
}

#[allow(unused_imports)]
pub use AppAction as Action;

#[cfg(test)]
mod tests {
    use super::DaemonCommand;

    #[test]
    fn daemon_command_openai_codex_auth_variants_are_constructible() {
        let commands = [
            DaemonCommand::GetOpenAICodexAuthStatus,
            DaemonCommand::LoginOpenAICodex,
            DaemonCommand::LogoutOpenAICodex,
        ];

        assert_eq!(commands.len(), 3);
    }
}
