#![allow(dead_code)]

use super::*;

pub(super) struct SendMessageRunner<'a> {
    pub(super) engine: &'a AgentEngine,
    pub(super) task_id: Option<&'a str>,
    pub(super) stored_user_content: &'a str,
    pub(super) stored_user_content_blocks: Vec<AgentContentBlock>,
    pub(super) llm_user_content: &'a str,
    pub(super) stream_chunk_timeout_override: Option<std::time::Duration>,
    pub(super) tid: String,
    pub(super) reuse_existing_user_message: bool,
    pub(super) config: AgentConfig,
    pub(super) provider_config: ProviderConfig,
    pub(super) preferred_session_id: Option<zorai_protocol::SessionId>,
    pub(super) onecontext_bootstrap: Option<String>,
    pub(super) skill_preflight: Option<crate::agent::skill_preflight::SkillPreflightContext>,
    pub(super) agent_scope_id: String,
    pub(super) runtime_agent_name: String,
    pub(super) active_provider_id: String,
    pub(super) memory_paths: MemoryPaths,
    pub(super) base_prompt: String,
    pub(super) operator_model_summary: Option<String>,
    pub(super) operational_context: Option<String>,
    pub(super) learned_patterns: Option<String>,
    pub(super) continuity_summary: Option<String>,
    pub(super) negative_constraints_context: Option<String>,
    pub(super) system_prompt: String,
    pub(super) current_task_snapshot: Option<AgentTask>,
    pub(super) is_durable_goal_task: bool,
    pub(super) task_tool_filter: Option<crate::agent::subagent::tool_filter::ToolFilter>,
    pub(super) task_context_budget: Option<crate::agent::subagent::context_budget::ContextBudget>,
    pub(super) task_termination_eval:
        Option<crate::agent::subagent::termination::TerminationEvaluator>,
    pub(super) task_type_for_trace: String,
    pub(super) initial_copilot_initiator: crate::agent::llm_client::CopilotInitiator,
    pub(super) tools: Vec<ToolDefinition>,
    pub(super) deferred_tool_pool: Vec<ToolDefinition>,
    pub(super) retry_strategy: RetryStrategy,
    pub(super) max_loops: u32,
    pub(super) stream_generation: u64,
    pub(super) stream_cancel_token: CancellationToken,
    pub(super) stream_retry_now: Arc<tokio::sync::Notify>,
    pub(super) loop_count: u32,
    pub(super) was_cancelled: bool,
    pub(super) interrupted_for_approval: bool,
    pub(super) terminated_for_budget: bool,
    pub(super) report_back_phase: Option<super::report_back::ReportBackReason>,
    pub(super) tools_before_report_back: Option<Vec<ToolDefinition>>,
    pub(super) subagent_report: Option<SubagentTurnReport>,
    pub(super) needs_turn_done: bool,
    pub(super) policy_aborted_retry: bool,
    pub(super) previous_tool_signature: Option<String>,
    pub(super) previous_tool_outcome: Option<(String, bool)>,
    pub(super) last_tool_error: Option<(String, String)>,
    pub(super) consecutive_same_tool_calls: u32,
    pub(super) last_pre_compaction_flush_signature: Option<u64>,
    pub(super) recorded_compaction_provenance: bool,
    pub(super) delivered_queued_message_ids: Vec<String>,
    pub(super) reused_prompt_preceding_message_id: Option<String>,
    pub(super) trace_collector: crate::agent::learning::traces::TraceCollector,
    pub(super) termination_tool_calls: u32,
    pub(super) termination_tool_successes: u32,
    pub(super) termination_consecutive_errors: u32,
    pub(super) termination_total_errors: u32,
    pub(super) loop_started_at: u64,
    pub(super) stream_timeout_count: u32,
    pub(super) tool_ack_emitted: bool,
    pub(super) tool_sequence_repaired: bool,
    pub(super) retry_status_visible: bool,
    pub(super) scheduled_retry_cycles: u32,
    pub(super) assistant_output_visible: bool,
    pub(super) tool_side_effect_committed: bool,
    pub(super) attempted_recovery_signatures: std::collections::HashSet<String>,
    pub(super) recent_policy_tool_outcomes:
        VecDeque<super::orchestrator_policy::PolicyToolOutcomeSummary>,
    pub(super) provider_final_result: Option<CompletionProviderFinalResult>,
    pub(super) fresh_runner_retry: Option<FreshRunnerRetryRequest>,
    pub(super) handoff_restart: Option<HandoffRestartRequest>,
    pub(super) turn_done_emitted: bool,
}

pub(super) struct StreamIteration {
    pub(super) prepared_request: PreparedLlmRequest,
    pub(super) llm_started_at: Instant,
    pub(super) first_token_at: Option<Instant>,
    pub(super) effective_transport_for_turn: ApiTransport,
    pub(super) accumulated_content: String,
    pub(super) accumulated_reasoning: String,
    pub(super) final_chunk: Option<CompletionChunk>,
    pub(super) stream_timed_out: bool,
    pub(super) retry_loop: bool,
}

pub(super) enum LoopDisposition {
    Continue,
    Break,
}

pub(super) enum ToolCallDisposition {
    ContinueTools,
    RestartLoop,
    BreakLoop,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FreshRunnerRetrySignal {
    pub(super) scheduled_retry_cycles: u32,
}

impl std::fmt::Display for FreshRunnerRetrySignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "restart send with a fresh runner after retry cycle {}",
            self.scheduled_retry_cycles
        )
    }
}

impl std::error::Error for FreshRunnerRetrySignal {}
