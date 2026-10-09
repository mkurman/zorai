#![allow(dead_code)]

//! AgentEngine struct definition and constructor.

use super::circuit_breaker::CircuitBreakerRegistry;
use super::concierge::ConciergeEngine;
use super::*;
use std::time::Duration;

pub(crate) mod helpers;
pub(in crate::agent) use helpers::aline_available;
use helpers::build_agent_http_client;
pub(in crate::agent) use helpers::{
    build_fresh_agent_http_client, collect_provider_health_snapshot,
    collect_provider_outage_metadata, default_agent_http_read_timeout,
    file_watch_event_is_relevant, format_provider_outage_message,
    provider_is_eligible_for_alternative,
};

pub(super) struct SendMessageOutcome {
    pub thread_id: String,
    pub stream_generation: u64,
    pub interrupted_for_approval: bool,
    pub terminated_for_budget: bool,
    pub subagent_report: Option<SubagentTurnReport>,
    pub upstream_message: Option<CompletionUpstreamMessage>,
    pub provider_final_result: Option<CompletionProviderFinalResult>,
    pub fresh_runner_retry: Option<FreshRunnerRetryRequest>,
    pub handoff_restart: Option<HandoffRestartRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubagentReportStatus {
    Done,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubagentTurnReport {
    pub status: SubagentReportStatus,
    pub summary: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FreshRunnerRetryRequest {
    pub scheduled_retry_cycles: u32,
}

#[derive(Debug, Clone)]
pub(super) struct HandoffRestartRequest {
    pub llm_user_content: String,
}

#[derive(Clone)]
pub struct StreamCancellationEntry {
    pub generation: u64,
    pub token: CancellationToken,
    pub retry_now: Arc<tokio::sync::Notify>,
    pub started_at: u64,
    pub last_progress_at: u64,
    pub last_progress_kind: StreamProgressKind,
    pub last_progress_excerpt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StreamProgressKind {
    Started,
    Reasoning,
    Content,
    ToolCalls,
}

pub struct ThreadRepoWatcher {
    pub repo_root: String,
    pub thread_ids: Arc<std::sync::Mutex<HashSet<String>>>,
    pub watcher: RecommendedWatcher,
}

#[derive(Clone, Debug)]
pub(super) struct SubagentRuntimeStats {
    pub task_id: String,
    pub parent_task_id: Option<String>,
    pub thread_id: Option<String>,
    pub started_at: u64,
    pub created_at: u64,
    pub max_duration_secs: Option<u64>,
    pub context_budget_tokens: Option<u32>,
    pub last_tool_call_at: Option<u64>,
    pub last_progress_at: Option<u64>,
    pub tool_calls_total: u32,
    pub tool_calls_succeeded: u32,
    pub tool_calls_failed: u32,
    pub consecutive_errors: u32,
    pub recent_tool_names: VecDeque<String>,
    pub tokens_consumed: u32,
    pub context_utilization_pct: u32,
    pub health_state: SubagentHealthState,
    pub updated_at: u64,
}

pub(super) const ONECONTEXT_BOOTSTRAP_QUERY_MAX_CHARS: usize = 180;
pub(super) const ONECONTEXT_BOOTSTRAP_OUTPUT_MAX_CHARS: usize = 5000;
pub(super) const ONECONTEXT_BOOTSTRAP_TIMEOUT_MS: u64 = 100;
pub(super) const MIN_CONTEXT_TARGET_TOKENS: usize = 1024;
pub(in crate::agent) const APPROX_CHARS_PER_TOKEN: usize = 4;
pub(super) const FILE_WATCH_DEBOUNCE_MS: u64 = 700;
pub(super) const FILE_WATCH_TICK_MS: u64 = 250;
const AGENT_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const AGENT_HTTP_READ_TIMEOUT: Duration = Duration::from_secs(125);

#[derive(Debug, Clone)]
pub struct CritiqueApprovalContinuation {
    pub tool_call: ToolCall,
    pub thread_id: String,
    pub agent_data_dir: PathBuf,
}

pub struct AgentEngine {
    pub started_at_ms: u64,
    pub config: Arc<RwLock<AgentConfig>>,
    pub http_client: reqwest::Client,
    pub concierge: Arc<ConciergeEngine>,
    pub session_manager: Arc<SessionManager>,
    pub history: HistoryStore,
    pub(crate) mlflow_tracing: Arc<super::mlflow_tracing::MlflowTracingRuntime>,
    pub threads: RwLock<HashMap<String, AgentThread>>,
    pub(super) thread_message_hydration_pending: RwLock<HashSet<String>>,
    pub(crate) pending_thread_workspace_contexts: RwLock<HashMap<String, ThreadWorkspaceContext>>,
    pub(super) thread_message_hydration_lock: Mutex<()>,
    pub(super) semantic_document_index_sync_lock: Mutex<()>,
    pub(super) semantic_vector_index_lock: Mutex<()>,
    #[cfg(test)]
    pub(super) thread_message_hydration_test_delay: Mutex<Option<Duration>>,
    pub thread_handoff_states: RwLock<HashMap<String, ThreadHandoffState>>,
    pub thread_participants: RwLock<HashMap<String, Vec<ThreadParticipantState>>>,
    pub thread_participant_suggestions: RwLock<HashMap<String, Vec<ThreadParticipantSuggestion>>>,
    pub(super) deferred_visible_thread_continuations:
        Mutex<HashMap<String, Vec<DeferredVisibleThreadContinuation>>>,
    pub(super) active_visible_thread_continuation_flushes: std::sync::Mutex<HashSet<String>>,
    pub(super) active_thread_participant_suggestion_drains: Mutex<HashSet<String>>,
    pub thread_client_surfaces: RwLock<HashMap<String, zorai_protocol::ClientSurface>>,
    pub thread_execution_profiles: RwLock<HashMap<String, ThreadExecutionProfile>>,
    pub(super) thread_identity_metadata: RwLock<HashMap<String, ThreadIdentityMetadata>>,
    pub thread_skill_discovery_states: RwLock<HashMap<String, LatestSkillDiscoveryState>>,
    pub thread_memory_injection_states: RwLock<HashMap<String, PromptMemoryInjectionState>>,
    pub thread_structural_memories:
        RwLock<HashMap<String, crate::agent::context::structural_memory::ThreadStructuralMemory>>,
    pub thread_todos: RwLock<HashMap<String, Vec<TodoItem>>>,
    pub thread_work_contexts: RwLock<HashMap<String, ThreadWorkContext>>,
    pub(super) resonance_context_cache:
        RwLock<HashMap<String, super::resonance::ResonanceContextSnapshot>>,
    pub tasks: Mutex<VecDeque<AgentTask>>,
    pub goal_runs: Mutex<VecDeque<GoalRun>>,
    pub goal_run_client_surfaces: RwLock<HashMap<String, zorai_protocol::ClientSurface>>,
    pub inflight_goal_runs: Mutex<HashSet<String>>,
    pub heartbeat_items: RwLock<Vec<HeartbeatItem>>,
    pub event_tx: broadcast::Sender<AgentEvent>,
    /// Daemon-internal coordination bus (never serialized to clients). Lets
    /// background workers react to lifecycle changes instead of polling.
    pub(super) internal_event_tx: broadcast::Sender<super::internal_event::InternalAgentEvent>,
    pub memory: RwLock<HashMap<String, AgentMemory>>,
    pub(super) recent_policy_decisions:
        RwLock<super::orchestrator_policy::ShortLivedRecentPolicyDecisions>,
    pub(super) retry_guards: RwLock<super::orchestrator_policy::ShortLivedRetryGuards>,
    pub(super) operator_model: RwLock<OperatorModel>,
    pub(super) meta_cognitive_self_model: RwLock<super::metacognitive::types::SelfModel>,
    pub(super) anticipatory: RwLock<AnticipatoryRuntime>,
    pub(super) collaboration: RwLock<HashMap<String, collaboration::CollaborationSession>>,
    pub(super) tool_synthesis_gap_notices: RwLock<HashSet<String>>,
    pub data_dir: PathBuf,
    pub workspace_root: Option<PathBuf>,
    pub gateway_process: Mutex<Option<tokio::process::Child>>,
    pub(super) gateway_init_lock: Mutex<()>,
    pub gateway_state: Mutex<Option<gateway::GatewayState>>,
    #[cfg(test)]
    pub(super) gateway_init_test_delay: Mutex<Option<Duration>>,
    pub gateway_ipc_sender: Mutex<Option<mpsc::UnboundedSender<zorai_protocol::DaemonMessage>>>,
    pub gateway_pending_send_results:
        Mutex<HashMap<String, tokio::sync::oneshot::Sender<zorai_protocol::GatewaySendResult>>>,
    pub gateway_restart_attempts: Mutex<u32>,
    pub gateway_restart_not_before_ms: Mutex<Option<u64>>,
    pub whatsapp_link: Arc<whatsapp_link::WhatsAppLinkRuntime>,
    /// Discord channel IDs to poll (parsed from config).
    pub gateway_discord_channels: RwLock<Vec<String>>,
    /// Slack channel IDs to poll (parsed from config).
    pub gateway_slack_channels: RwLock<Vec<String>>,
    /// Maps gateway channel IDs to daemon thread IDs for conversation continuity.
    pub gateway_threads: RwLock<HashMap<String, String>>,
    /// Sticky agent route per gateway channel.
    pub gateway_route_modes: RwLock<HashMap<String, gateway::GatewayRouteMode>>,
    /// Recently-seen gateway message IDs for deduplication (capped ring buffer).
    pub gateway_seen_ids: Mutex<Vec<String>>,
    /// Channels currently being processed — prevents concurrent dispatch to the same channel.
    pub gateway_inflight_channels: Mutex<HashSet<String>>,
    /// Messages that arrived while a channel's turn was in flight; drained as
    /// follow-up turns the moment the active turn finishes.
    pub gateway_pending_followups: Mutex<HashMap<String, VecDeque<gateway::IncomingMessage>>>,
    /// Queue of externally injected gateway messages (e.g. linked WhatsApp sidecar).
    pub gateway_injected_messages: Mutex<VecDeque<gateway::IncomingMessage>>,
    pub(super) webhook_listener_addr: RwLock<Option<String>>,
    /// External agent runners for openclaw/hermes backends.
    pub external_runners: RwLock<HashMap<String, external_runner::ExternalAgentRunner>>,
    pub(super) subagent_runtime: RwLock<HashMap<String, SubagentRuntimeStats>>,
    pub(super) agent_terminal_leases:
        Mutex<HashMap<uuid::Uuid, super::terminal_leases::AgentTerminalLease>>,
    pub(super) trusted_weles_tasks: RwLock<HashSet<String>>,
    pub(super) weles_health: RwLock<WelesHealthStatus>,
    /// Active cancellation tokens per thread for stop-stream behavior.
    pub stream_cancellations: Mutex<HashMap<String, StreamCancellationEntry>>,
    pub(super) stream_providers: Mutex<HashMap<String, (u64, String)>>,
    pub(crate) operator_stopped_streams: Mutex<HashSet<String>>,
    pub stream_generation: AtomicU64,
    pub(super) stalled_turn_candidates:
        Mutex<HashMap<String, crate::agent::stalled_turns::StalledTurnCandidate>>,
    pub(super) operation_wakeups:
        Mutex<HashMap<String, crate::agent::operation_wakeup::OperationWakeup>>,
    pub(super) timer_wakeups: Mutex<HashMap<String, crate::agent::agent_wakeup::AgentWakeup>>,
    pub(super) active_operator_sessions: RwLock<HashMap<String, u64>>,
    pub(super) pending_operator_approvals: RwLock<HashMap<String, PendingApprovalObservation>>,
    pub(super) pending_approval_commands: RwLock<HashMap<String, String>>,
    pub(super) critique_approval_continuations:
        Mutex<HashMap<String, CritiqueApprovalContinuation>>,
    pub(super) policy_escalation_session_grants: RwLock<HashSet<String>>,
    pub(super) task_approval_rules: RwLock<Vec<zorai_protocol::TaskApprovalRule>>,
    pub(super) pending_operator_questions: Mutex<HashMap<String, PendingOperatorQuestionState>>,
    pub(super) operator_profile_sessions: RwLock<HashMap<String, OperatorProfileSessionState>>,
    pub(super) honcho_sync: Mutex<HonchoSyncState>,
    pub repo_watchers: Mutex<HashMap<String, ThreadRepoWatcher>>,
    pub watcher_refresh_tx: mpsc::UnboundedSender<String>,
    pub watcher_refresh_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    pub(super) skill_discovery_result_tx:
        mpsc::UnboundedSender<super::skill_preflight::AsyncSkillDiscoveryCompletion>,
    pub(super) auto_thread_title_jobs:
        mpsc::UnboundedSender<super::thread_title::AutoThreadTitleJob>,
    pub(super) prompt_queue_wake_tx: mpsc::UnboundedSender<String>,
    pub(super) continuation_flush_tx: mpsc::UnboundedSender<String>,
    pub(super) internal_dm_jobs_tx: mpsc::UnboundedSender<super::messaging::InternalDmJob>,
    #[cfg(test)]
    pub(super) skill_discovery_test_runner:
        std::sync::OnceLock<Arc<dyn super::skill_preflight::SkillDiscoveryTestRunner>>,
    #[cfg(test)]
    pub(super) force_mesh_discovery_degraded_for_tests: std::sync::atomic::AtomicBool,
    pub(super) aline_startup_reconcile_started: std::sync::atomic::AtomicBool,
    pub(super) aline_startup_test_completion: std::sync::OnceLock<tokio::sync::watch::Sender<bool>>,
    #[cfg(test)]
    pub(super) aline_startup_test_runner:
        std::sync::OnceLock<Arc<dyn super::aline_startup::StartupCommandRunner>>,
    #[cfg(test)]
    pub(super) aline_startup_test_availability: std::sync::OnceLock<bool>,
    #[cfg(test)]
    pub(super) aline_startup_test_repo_roots: Mutex<Vec<PathBuf>>,
    pub(super) aline_startup_last_summary: Mutex<Option<super::aline_startup::AlineStartupSummary>>,
    /// Per-provider circuit breakers for LLM call path gating.
    pub circuit_breakers: Arc<CircuitBreakerRegistry>,
    /// Locally known provider failures. Populated only from authenticated
    /// session state or an observed upstream response; preflight never probes.
    pub(super) provider_availability_signals:
        RwLock<HashMap<String, super::provider_preflight::KnownProviderAvailabilitySignal>>,
    /// Notifies the run_loop when config changes so heartbeat schedule can be recomputed.
    pub config_notify: tokio::sync::Notify,
    /// Tracks desired-vs-effective runtime config reconciliation progress.
    pub(super) config_runtime_projection: Mutex<ConfigRuntimeProjection>,
    /// Learned priority weights per check type, updated from feedback signals (D-04).
    /// When present, these override the config `priority_weight` fields.
    /// Falls back to config weights when a check type has no learned weight.
    pub(crate) learned_check_weights: RwLock<HashMap<HeartbeatCheckType, f64>>,
    /// Aggregated learned heuristics persisted across restarts (Phase 5).
    pub(super) heuristic_store: RwLock<super::learning::heuristics::HeuristicStore>,
    /// Mined tool-usage patterns persisted across restarts (Phase 5).
    pub(super) pattern_store: RwLock<super::learning::patterns::PatternStore>,
    /// Feature disclosure queue for progressive tier-based feature revelation (D-13).
    pub(super) disclosure_queue: RwLock<super::capability_tier::DisclosureQueue>,
    /// Plugin manager for API proxy tool executor access (Phase 17).
    /// Set after both AgentEngine and PluginManager are constructed in server.rs.
    pub plugin_manager: std::sync::OnceLock<Arc<crate::plugin::PluginManager>>,
    /// Episodic memory subsystem state (Phase v3.0).
    pub(super) episodic_store: RwLock<HashMap<String, super::episodic::EpisodicStore>>,
    /// Situational awareness monitor (Phase v3.0: AWAR-01).
    pub(super) awareness: RwLock<super::awareness::AwarenessMonitor>,
    /// Calibration tracker for uncertainty quantification (Phase v3.0: UNCR-07).
    pub(super) calibration_tracker: RwLock<super::uncertainty::calibration::CalibrationTracker>,
    /// Handoff broker for multi-agent task delegation (Phase v3.0: HAND-01).
    pub(super) handoff_broker: RwLock<super::handoff::HandoffBroker>,
    /// Cached embeddings of the specialist catalog for semantic handoff routing.
    pub(super) specialist_embedding_cache:
        tokio::sync::Mutex<super::handoff::SpecialistEmbeddingCache>,
    /// Active divergent sessions for parallel framing mode (Phase v3.0: DIVR-01).
    pub(super) divergent_sessions:
        RwLock<HashMap<String, super::handoff::divergent::DivergentSession>>,
    /// Active debate sessions for structured multi-round debate mode (Spec-04).
    pub(super) debate_sessions: RwLock<HashMap<String, super::debate::types::DebateSession>>,
    /// Retry counters for required goal step completion marker reminders.
    pub(super) goal_step_completion_marker_retries: Mutex<HashMap<String, u32>>,
    /// Per-goal-run cost trackers, keyed by goal_run_id (Phase v3.0: COST-01).
    pub(super) cost_trackers: Mutex<HashMap<String, super::cost::CostTracker>>,
    /// Per-goal-run `HealthMonitor` instances for the heartbeat-driven
    /// aggregation layer. Lives on the engine (not reconstructed each tick)
    /// so the monitor's hysteresis counters can persist across heartbeat
    /// cycles — a transient error-rate spike across one tick must not flip
    /// the goal-run state on its own. Entries are evicted when their goal
    /// run leaves the `Running` status set.
    pub(super) goal_run_health_monitors:
        RwLock<HashMap<String, super::liveness::health_monitor::HealthMonitor>>,
}

impl AgentEngine {
    pub fn new_with_shared_history(
        session_manager: Arc<SessionManager>,
        config: AgentConfig,
        history: Arc<HistoryStore>,
    ) -> Arc<Self> {
        Self::new_with_storage(
            session_manager,
            config,
            (*history).clone(),
            agent_data_dir(),
        )
    }

    fn new_with_storage(
        session_manager: Arc<SessionManager>,
        config: AgentConfig,
        history: HistoryStore,
        data_dir: PathBuf,
    ) -> Arc<Self> {
        Self::new_with_storage_and_http_client(
            session_manager,
            config,
            history,
            data_dir,
            build_agent_http_client(AGENT_HTTP_READ_TIMEOUT),
        )
    }

    pub(crate) fn new_with_storage_and_http_client(
        session_manager: Arc<SessionManager>,
        config: AgentConfig,
        history: HistoryStore,
        data_dir: PathBuf,
        http_client: reqwest::Client,
    ) -> Arc<Self> {
        let workspace_root = std::env::current_dir().ok().filter(|path| path.is_dir());
        Self::new_with_storage_and_http_client_for_root(
            session_manager,
            config,
            history,
            data_dir,
            http_client,
            workspace_root,
        )
    }

    fn new_with_storage_and_http_client_for_root(
        session_manager: Arc<SessionManager>,
        config: AgentConfig,
        history: HistoryStore,
        data_dir: PathBuf,
        http_client: reqwest::Client,
        workspace_root: Option<PathBuf>,
    ) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(config.agent_event_channel_capacity);
        let internal_event_tx = super::internal_event::new_internal_event_channel();
        let (watcher_refresh_tx, watcher_refresh_rx) = mpsc::unbounded_channel();
        let (skill_discovery_result_tx, skill_discovery_result_rx) = mpsc::unbounded_channel();
        let (auto_thread_title_jobs, auto_thread_title_rx) = mpsc::unbounded_channel();
        let (prompt_queue_wake_tx, prompt_queue_wake_rx) = mpsc::unbounded_channel();
        let (continuation_flush_tx, continuation_flush_rx) = mpsc::unbounded_channel();
        let (internal_dm_jobs_tx, internal_dm_jobs_rx) = mpsc::unbounded_channel();

        let mut runners = HashMap::new();
        for agent_type in &["openclaw", "hermes"] {
            runners.insert(
                agent_type.to_string(),
                external_runner::ExternalAgentRunner::new(agent_type, event_tx.clone()),
            );
        }

        let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
            config
                .providers
                .keys()
                .cloned()
                .chain(std::iter::once(config.provider.clone())),
        ));

        let initial_config_runtime_projection =
            super::config::derive_startup_config_runtime_projection(&config);

        let initial_mlflow_config = config.mlflow_tracing.clone();
        let config = Arc::new(RwLock::new(config));
        let mlflow_tracing =
            super::mlflow_tracing::MlflowTracingRuntime::new(&data_dir, &initial_mlflow_config);
        let concierge = Arc::new(ConciergeEngine::new(
            config.clone(),
            event_tx.clone(),
            http_client.clone(),
            circuit_breakers.clone(),
        ));

        let engine = Arc::new(Self {
            started_at_ms: now_millis(),
            config,
            http_client,
            concierge,
            session_manager,
            history,
            mlflow_tracing: mlflow_tracing.clone(),
            threads: RwLock::new(HashMap::new()),
            thread_message_hydration_pending: RwLock::new(HashSet::new()),
            pending_thread_workspace_contexts: RwLock::new(HashMap::new()),
            thread_message_hydration_lock: Mutex::new(()),
            semantic_document_index_sync_lock: Mutex::new(()),
            semantic_vector_index_lock: Mutex::new(()),
            #[cfg(test)]
            thread_message_hydration_test_delay: Mutex::new(None),
            thread_handoff_states: RwLock::new(HashMap::new()),
            thread_participants: RwLock::new(HashMap::new()),
            thread_participant_suggestions: RwLock::new(HashMap::new()),
            deferred_visible_thread_continuations: Mutex::new(HashMap::new()),
            active_visible_thread_continuation_flushes: std::sync::Mutex::new(HashSet::new()),
            active_thread_participant_suggestion_drains: Mutex::new(HashSet::new()),
            thread_client_surfaces: RwLock::new(HashMap::new()),
            thread_execution_profiles: RwLock::new(HashMap::new()),
            thread_identity_metadata: RwLock::new(HashMap::new()),
            thread_skill_discovery_states: RwLock::new(HashMap::new()),
            thread_memory_injection_states: RwLock::new(HashMap::new()),
            thread_structural_memories: RwLock::new(HashMap::new()),
            thread_todos: RwLock::new(HashMap::new()),
            thread_work_contexts: RwLock::new(HashMap::new()),
            resonance_context_cache: RwLock::new(HashMap::new()),
            tasks: Mutex::new(VecDeque::new()),
            goal_runs: Mutex::new(VecDeque::new()),
            goal_run_client_surfaces: RwLock::new(HashMap::new()),
            inflight_goal_runs: Mutex::new(HashSet::new()),
            heartbeat_items: RwLock::new(Vec::new()),
            event_tx: event_tx.clone(),
            internal_event_tx,
            memory: RwLock::new(HashMap::new()),
            recent_policy_decisions: RwLock::new(HashMap::new()),
            retry_guards: RwLock::new(HashMap::new()),
            operator_model: RwLock::new(OperatorModel::default()),
            meta_cognitive_self_model: RwLock::new(
                super::metacognitive::types::SelfModel::default(),
            ),
            anticipatory: RwLock::new(AnticipatoryRuntime::default()),
            collaboration: RwLock::new(HashMap::new()),
            tool_synthesis_gap_notices: RwLock::new(HashSet::new()),
            data_dir,
            workspace_root,
            gateway_process: Mutex::new(None),
            gateway_init_lock: Mutex::new(()),
            gateway_state: Mutex::new(None),
            #[cfg(test)]
            gateway_init_test_delay: Mutex::new(None),
            gateway_ipc_sender: Mutex::new(None),
            gateway_pending_send_results: Mutex::new(HashMap::new()),
            gateway_restart_attempts: Mutex::new(0),
            gateway_restart_not_before_ms: Mutex::new(None),
            whatsapp_link: Arc::new(whatsapp_link::WhatsAppLinkRuntime::new()),
            gateway_discord_channels: RwLock::new(Vec::new()),
            gateway_slack_channels: RwLock::new(Vec::new()),
            gateway_threads: RwLock::new(HashMap::new()),
            gateway_route_modes: RwLock::new(HashMap::new()),
            gateway_seen_ids: Mutex::new(Vec::new()),
            gateway_inflight_channels: Mutex::new(HashSet::new()),
            gateway_pending_followups: Mutex::new(HashMap::new()),
            gateway_injected_messages: Mutex::new(VecDeque::new()),
            webhook_listener_addr: RwLock::new(None),
            external_runners: RwLock::new(runners),
            subagent_runtime: RwLock::new(HashMap::new()),
            agent_terminal_leases: Mutex::new(HashMap::new()),
            trusted_weles_tasks: RwLock::new(HashSet::new()),
            weles_health: RwLock::new(WelesHealthStatus {
                state: WelesHealthState::Healthy,
                reason: None,
                checked_at: 0,
            }),
            stream_cancellations: Mutex::new(HashMap::new()),
            stream_providers: Mutex::new(HashMap::new()),
            operator_stopped_streams: Mutex::new(HashSet::new()),
            stream_generation: AtomicU64::new(1),
            stalled_turn_candidates: Mutex::new(HashMap::new()),
            operation_wakeups: Mutex::new(HashMap::new()),
            timer_wakeups: Mutex::new(HashMap::new()),
            active_operator_sessions: RwLock::new(HashMap::new()),
            pending_operator_approvals: RwLock::new(HashMap::new()),
            pending_approval_commands: RwLock::new(HashMap::new()),
            critique_approval_continuations: Mutex::new(HashMap::new()),
            policy_escalation_session_grants: RwLock::new(HashSet::new()),
            task_approval_rules: RwLock::new(Vec::new()),
            pending_operator_questions: Mutex::new(HashMap::new()),
            operator_profile_sessions: RwLock::new(HashMap::new()),
            honcho_sync: Mutex::new(HonchoSyncState::default()),
            repo_watchers: Mutex::new(HashMap::new()),
            watcher_refresh_tx,
            watcher_refresh_rx: Mutex::new(Some(watcher_refresh_rx)),
            skill_discovery_result_tx,
            auto_thread_title_jobs,
            prompt_queue_wake_tx,
            continuation_flush_tx,
            internal_dm_jobs_tx,
            #[cfg(test)]
            skill_discovery_test_runner: std::sync::OnceLock::new(),
            #[cfg(test)]
            force_mesh_discovery_degraded_for_tests: std::sync::atomic::AtomicBool::new(false),
            aline_startup_reconcile_started: std::sync::atomic::AtomicBool::new(false),
            aline_startup_test_completion: std::sync::OnceLock::new(),
            #[cfg(test)]
            aline_startup_test_runner: std::sync::OnceLock::new(),
            #[cfg(test)]
            aline_startup_test_availability: std::sync::OnceLock::new(),
            #[cfg(test)]
            aline_startup_test_repo_roots: Mutex::new(Vec::new()),
            aline_startup_last_summary: Mutex::new(None),
            circuit_breakers,
            provider_availability_signals: RwLock::new(HashMap::new()),
            config_notify: tokio::sync::Notify::new(),
            config_runtime_projection: Mutex::new(initial_config_runtime_projection),
            learned_check_weights: RwLock::new(HashMap::new()),
            heuristic_store: RwLock::new(super::learning::heuristics::HeuristicStore::default()),
            pattern_store: RwLock::new(super::learning::patterns::PatternStore::default()),
            disclosure_queue: RwLock::new(super::capability_tier::DisclosureQueue::default()),
            plugin_manager: std::sync::OnceLock::new(),
            episodic_store: RwLock::new(HashMap::new()),
            awareness: RwLock::new(super::awareness::AwarenessMonitor::new()),
            calibration_tracker: RwLock::new(
                super::uncertainty::calibration::CalibrationTracker::default(),
            ),
            handoff_broker: RwLock::new(super::handoff::HandoffBroker::default()),
            specialist_embedding_cache: tokio::sync::Mutex::new(
                super::handoff::SpecialistEmbeddingCache::default(),
            ),
            divergent_sessions: RwLock::new(HashMap::new()),
            debate_sessions: RwLock::new(HashMap::new()),
            goal_step_completion_marker_retries: Mutex::new(HashMap::new()),
            cost_trackers: Mutex::new(HashMap::new()),
            goal_run_health_monitors: RwLock::new(HashMap::new()),
        });
        super::skill_preflight::spawn_skill_discovery_result_applier(
            engine.clone(),
            skill_discovery_result_rx,
        );
        super::thread_title::spawn_auto_thread_title_worker(engine.clone(), auto_thread_title_rx);
        super::prompt_queue::spawn_prompt_queue_worker(engine.clone(), prompt_queue_wake_rx);
        super::thread_participants::spawn_continuation_flush_worker(
            engine.clone(),
            continuation_flush_rx,
        );
        super::prompt_queue::spawn_prompt_queue_startup_flush(engine.clone());
        super::messaging::spawn_internal_dm_worker(engine.clone(), internal_dm_jobs_rx);
        mlflow_tracing.start(
            Arc::downgrade(&engine),
            event_tx.subscribe(),
            initial_mlflow_config,
        );
        Self::spawn_svarog_workspace_reconciliation(engine.clone());
        Self::spawn_agent_terminal_lease_worker(engine.clone());

        engine
    }

    pub(crate) fn thread_memory_injection_state_map(
        &self,
    ) -> &RwLock<HashMap<String, PromptMemoryInjectionState>> {
        &self.thread_memory_injection_states
    }

    /// Check the circuit breaker before an LLM call. Returns `Err` if the
    /// breaker is open (provider is unhealthy). Callers must invoke
    /// [`record_llm_outcome`] after the call completes.
    pub async fn check_circuit_breaker(&self, provider: &str) -> Result<()> {
        let breaker_arc = self.circuit_breakers.get(provider).await;
        let mut breaker = breaker_arc.lock().await;
        let now = now_millis();

        if !breaker.can_execute(now) {
            let trip_count = breaker.trip_count();
            drop(breaker);
            let outage = collect_provider_outage_metadata(
                &self.config,
                &self.circuit_breakers,
                provider,
                trip_count,
                "circuit breaker open",
            )
            .await;
            let _ = self.event_tx.send(AgentEvent::ProviderCircuitOpen {
                provider: outage.provider,
                failed_model: outage.failed_model,
                trip_count: outage.trip_count,
                reason: outage.reason,
                suggested_alternatives: outage.suggested_alternatives,
            });
            self.abort_streams_for_provider(provider).await;
            anyhow::bail!(
                "Circuit breaker open for provider '{}' — {} consecutive failures. \
                 Requests are blocked for ~30s to allow recovery.",
                provider,
                trip_count
            );
        }
        Ok(())
    }

    pub(super) async fn provider_circuit_is_closed(&self, provider: &str) -> bool {
        use super::circuit_breaker::CircuitState;

        let breaker_arc = self.circuit_breakers.get(provider).await;
        let breaker = breaker_arc.lock().await;
        breaker.state() == CircuitState::Closed
    }

    /// Record the outcome of an LLM call for circuit breaker tracking.
    /// Emits [`AgentEvent::ProviderCircuitOpen`] on trip and
    /// [`AgentEvent::ProviderCircuitRecovered`] on recovery.
    pub async fn record_llm_outcome(&self, provider: &str, success: bool) {
        use super::circuit_breaker::CircuitState;

        let breaker_arc = self.circuit_breakers.get(provider).await;
        let mut breaker = breaker_arc.lock().await;
        let now = now_millis();

        if success {
            let was_half_open = breaker.state() == CircuitState::HalfOpen;
            breaker.record_success(now);
            if was_half_open && breaker.state() == CircuitState::Closed {
                tracing::info!(
                    provider,
                    "circuit breaker recovered — provider is healthy again"
                );
                let _ = self.event_tx.send(AgentEvent::ProviderCircuitRecovered {
                    provider: provider.to_string(),
                });
            }
        } else {
            let was_closed_or_half = breaker.state() != CircuitState::Open;
            breaker.record_failure(now);
            if was_closed_or_half && breaker.state() == CircuitState::Open {
                let trip_count = breaker.trip_count();
                drop(breaker);
                let outage = collect_provider_outage_metadata(
                    &self.config,
                    &self.circuit_breakers,
                    provider,
                    trip_count,
                    "circuit breaker tripped",
                )
                .await;
                tracing::warn!(
                    provider,
                    trips = trip_count,
                    "circuit breaker tripped — provider marked unhealthy"
                );
                let _ = self.event_tx.send(AgentEvent::ProviderCircuitOpen {
                    provider: outage.provider,
                    failed_model: outage.failed_model,
                    trip_count: outage.trip_count,
                    reason: outage.reason,
                    suggested_alternatives: outage.suggested_alternatives,
                });
                self.abort_streams_for_provider(provider).await;
            }
        }
    }

    async fn provider_is_eligible_for_alternative(
        &self,
        failed_provider: &str,
        provider_id: &str,
    ) -> bool {
        provider_is_eligible_for_alternative(
            &self.config,
            &self.circuit_breakers,
            failed_provider,
            provider_id,
        )
        .await
    }

    /// Suggest an alternative healthy provider when the requested one is unavailable.
    pub(super) async fn suggest_alternative_provider(
        &self,
        failed_provider: &str,
    ) -> Option<String> {
        let outage = collect_provider_outage_metadata(
            &self.config,
            &self.circuit_breakers,
            failed_provider,
            0,
            "circuit breaker open",
        )
        .await;
        format_provider_outage_message(&outage)
    }

    #[cfg(test)]
    pub(crate) async fn new_test(
        session_manager: Arc<SessionManager>,
        config: AgentConfig,
        root: &std::path::Path,
    ) -> Arc<Self> {
        let history = HistoryStore::new_test_store(root)
            .await
            .expect("test history store initialization failed");
        let data_dir = root.join("agent");
        std::fs::create_dir_all(&data_dir).expect("failed to create test agent data dir");
        Self::new_with_storage_and_http_client_for_root(
            session_manager,
            config,
            history,
            data_dir,
            build_agent_http_client(AGENT_HTTP_READ_TIMEOUT),
            Some(root.to_path_buf()),
        )
    }

    #[cfg(test)]
    pub(crate) fn set_skill_discovery_test_runner(
        &self,
        runner: Arc<dyn super::skill_preflight::SkillDiscoveryTestRunner>,
    ) {
        let _ = self.skill_discovery_test_runner.set(runner);
    }

    /// Subscribe to agent events (for IPC forwarding to frontend).
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.event_tx.subscribe()
    }

    /// Get a reference to the event sender (for server.rs integration).
    pub fn event_sender(&self) -> broadcast::Sender<AgentEvent> {
        self.event_tx.clone()
    }

    pub(super) async fn run_aline_startup_reconciliation(
        &self,
        repo_root: PathBuf,
    ) -> Result<super::aline_startup::AlineStartupSummary> {
        #[cfg(test)]
        {
            self.aline_startup_test_repo_roots
                .lock()
                .await
                .push(repo_root.clone());
        }

        if !self.aline_startup_is_available() {
            let summary = super::aline_startup::AlineStartupSummary::unavailable();
            self.record_aline_startup_summary(summary.clone()).await;
            log_aline_startup_summary(&repo_root, &summary);
            return Ok(summary);
        }

        let runner = self.aline_startup_command_runner();
        let summary = super::aline_startup::run_aline_startup_reconciliation_for_data_dir(
            runner.as_ref(),
            &repo_root,
            &self.data_dir,
        )
        .await?;

        self.record_aline_startup_summary(summary.clone()).await;
        log_aline_startup_summary(&repo_root, &summary);
        Ok(summary)
    }

    pub(super) fn aline_startup_is_available(&self) -> bool {
        #[cfg(test)]
        if let Some(available) = self.aline_startup_test_availability.get() {
            return *available;
        }

        aline_available()
    }

    pub(super) fn aline_startup_command_runner(
        &self,
    ) -> Arc<dyn super::aline_startup::StartupCommandRunner> {
        #[cfg(test)]
        if let Some(runner) = self.aline_startup_test_runner.get() {
            return Arc::clone(runner);
        }

        Arc::new(super::aline_startup::TokioStartupCommandRunner)
    }

    pub(super) fn notify_aline_startup_reconciliation_finished_for_tests(&self) {
        if let Some(tx) = self.aline_startup_test_completion.get() {
            let _ = tx.send(true);
        }
    }

    pub(super) async fn maybe_run_aline_startup_reconciliation_for_repo(&self, repo_root: &str) {
        if !self.aline_startup_is_available() {
            return;
        }

        if self
            .aline_startup_reconcile_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        if let Err(error) = self
            .run_aline_startup_reconciliation(PathBuf::from(repo_root))
            .await
        {
            tracing::warn!(repo_root = %repo_root, %error, "Aline startup reconciliation failed");
        }
        self.notify_aline_startup_reconciliation_finished_for_tests();
    }

    #[cfg(test)]
    pub(in crate::agent) fn set_aline_startup_test_runner(
        &self,
        runner: Arc<dyn super::aline_startup::StartupCommandRunner>,
    ) {
        let _ = self.aline_startup_test_runner.set(runner);
    }

    #[cfg(test)]
    pub(crate) fn set_aline_startup_test_availability(&self, available: bool) {
        let _ = self.aline_startup_test_availability.set(available);
    }

    #[cfg(test)]
    pub(crate) fn install_aline_startup_test_completion(
        &self,
    ) -> tokio::sync::watch::Receiver<bool> {
        self.aline_startup_test_completion
            .get_or_init(|| {
                let (tx, _rx) = tokio::sync::watch::channel(false);
                tx
            })
            .subscribe()
    }

    #[cfg(test)]
    pub(crate) fn aline_startup_reconciliation_started_for_tests(&self) -> bool {
        self.aline_startup_reconcile_started.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) async fn aline_startup_repo_roots_for_tests(&self) -> Vec<PathBuf> {
        self.aline_startup_test_repo_roots.lock().await.clone()
    }

    pub(super) async fn aline_startup_last_summary(
        &self,
    ) -> Option<super::aline_startup::AlineStartupSummary> {
        self.aline_startup_last_summary.lock().await.clone()
    }

    pub(super) async fn record_aline_startup_summary(
        &self,
        summary: super::aline_startup::AlineStartupSummary,
    ) {
        *self.aline_startup_last_summary.lock().await = Some(summary);
    }

    #[cfg(test)]
    pub(super) async fn aline_startup_last_summary_for_tests(
        &self,
    ) -> Option<super::aline_startup::AlineStartupSummary> {
        self.aline_startup_last_summary().await
    }

    #[cfg(test)]
    pub(super) async fn record_aline_startup_summary_for_tests(
        &self,
        summary: super::aline_startup::AlineStartupSummary,
    ) {
        self.record_aline_startup_summary(summary).await;
    }
}

pub(super) fn log_aline_startup_summary(
    repo_root: &std::path::Path,
    summary: &super::aline_startup::AlineStartupSummary,
) {
    let fields = || {
        (
            repo_root.display().to_string(),
            summary.aline_available,
            summary.watcher_initial_state.clone(),
            summary.watcher_started,
            summary.discovered_count,
            summary.selected_count,
            summary.imported_count,
            summary.generated_count,
            summary.skipped_recently_imported_count,
            summary.budget_exhausted,
            summary.failure_stage.clone(),
            summary.failure_message.clone(),
            summary
                .short_circuit_reason
                .map(super::aline_startup::AlineStartupShortCircuitReason::as_str),
        )
    };

    let (
        repo_root_display,
        aline_available,
        watcher_initial_state,
        watcher_started,
        discovered_count,
        selected_count,
        imported_count,
        generated_count,
        skipped_recently_imported_count,
        budget_exhausted,
        failure_stage,
        failure_message,
        short_circuit_reason,
    ) = fields();

    if failure_stage.is_some() {
        tracing::warn!(
            repo_root = %repo_root_display,
            aline_available,
            watcher_initial_state = ?watcher_initial_state,
            watcher_started,
            discovered_count,
            selected_count,
            imported_count,
            generated_count,
            skipped_recently_imported_count,
            budget_exhausted,
            failure_stage = failure_stage.as_deref(),
            failure_message = failure_message.as_deref(),
            short_circuit_reason,
            "Aline startup reconciliation summary"
        );
    } else {
        tracing::info!(
            repo_root = %repo_root_display,
            aline_available,
            watcher_initial_state = ?watcher_initial_state,
            watcher_started,
            discovered_count,
            selected_count,
            imported_count,
            generated_count,
            skipped_recently_imported_count,
            budget_exhausted,
            failure_stage = failure_stage.as_deref(),
            failure_message = failure_message.as_deref(),
            short_circuit_reason,
            "Aline startup reconciliation summary"
        );
    }
}

#[cfg(test)]
#[path = "tests/engine.rs"]
mod tests;
