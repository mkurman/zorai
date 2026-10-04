use super::ApiTransport;
use serde::{Deserialize, Serialize};

/// Configuration for sub-agent supervision — how often to check, when to
/// consider a sub-agent stuck, and what intervention level to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorConfig {
    /// How often to check sub-agent health (seconds). Default: 30.
    #[serde(default = "default_supervisor_check_interval")]
    pub check_interval_secs: u64,
    /// Seconds of no progress before flagging as stuck. Default: 300 (5 min).
    #[serde(default = "default_stuck_timeout")]
    pub stuck_timeout_secs: u64,
    /// Maximum retries before escalating. Default: 2.
    #[serde(default = "default_supervisor_max_retries")]
    pub max_retries: u32,
    /// How aggressively to intervene. Default: Normal.
    #[serde(default)]
    pub intervention_level: InterventionLevel,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: default_supervisor_check_interval(),
            stuck_timeout_secs: default_stuck_timeout(),
            max_retries: default_supervisor_max_retries(),
            intervention_level: InterventionLevel::default(),
        }
    }
}

fn default_supervisor_check_interval() -> u64 {
    30
}
fn default_stuck_timeout() -> u64 {
    300
}
fn default_supervisor_max_retries() -> u32 {
    2
}

/// How aggressively the supervisor should intervene when issues are detected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum InterventionLevel {
    /// Only log, never intervene automatically.
    Passive,
    /// Self-correct where safe (compress context, inject reflection).
    #[default]
    Normal,
    /// Aggressively intervene (terminate stuck agents, retry from checkpoint).
    Aggressive,
}

/// Overall health state of a sub-agent as determined by the supervisor.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentHealthState {
    #[default]
    Healthy,
    Degraded,
    Stuck,
    Crashed,
}

/// Why a sub-agent is considered stuck.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StuckReason {
    /// No tool calls or progress for configured timeout.
    NoProgress,
    /// Same error repeated 3+ times in a row.
    ErrorLoop,
    /// Cycling tool calls (A→B→A→B pattern).
    ToolCallLoop,
    /// Context budget > 90% consumed.
    ResourceExhaustion,
    /// Exceeded max_duration_secs.
    Timeout,
}

/// What the supervisor should do when a problem is detected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InterventionAction {
    /// Inject a self-assessment prompt asking the agent to reflect.
    SelfAssess,
    /// Compress context to free up budget.
    CompressContext,
    /// Retry from the last successful checkpoint.
    RetryFromCheckpoint,
    /// Escalate to the parent task/agent.
    EscalateToParent,
    /// Escalate to the user for manual intervention.
    EscalateToUser,
}

/// What to do when a context budget is exceeded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextOverflowAction {
    /// Compress older context to free space.
    #[default]
    Compress,
    /// Truncate oldest messages.
    Truncate,
    /// Return an error and stop execution.
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    #[serde(alias = "running")]
    InProgress,
    AwaitingApproval,
    Blocked,
    FailedAnalyzing,
    BudgetExceeded,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    /// Whether this status is terminal: the task will make no further progress
    /// and blocked parents may resume. Single source of truth — coordination
    /// paths (dispatch wakeup, supervision, internal events) must all use this.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::Failed
                | Self::Cancelled
                | Self::BudgetExceeded
                | Self::FailedAnalyzing
        )
    }
}

pub const SUBAGENT_REPORT_REQUIREMENT_DESCRIPTION: &str = "usable subagent outcome report";
pub const CHILD_RESULT_INTEGRATION_REQUIREMENT_PREFIX: &str = "integrate child result: ";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChildReportState {
    #[default]
    Unavailable,
    Usable,
    Empty,
    Truncated,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParentNotificationState {
    #[default]
    NotRequired,
    Pending,
    Delivered,
}

/// Durable state at the spawned-child/parent integration boundary. This is
/// stored with the child task so a daemon restart can replay a pending parent
/// notification without guessing whether the child report was usable.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildResultContract {
    #[serde(default)]
    pub terminal_status: Option<TaskStatus>,
    #[serde(default)]
    pub terminal_version: u32,
    #[serde(default)]
    pub report_state: ChildReportState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub summary_chars: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_error: Option<String>,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default)]
    pub open_ask_ids: Vec<String>,
    #[serde(default)]
    pub asks_reconciled: bool,
    #[serde(default)]
    pub parent_notification: ParentNotificationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_notification_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_notified_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration_acknowledged_at: Option<u64>,
}

impl ChildResultContract {
    pub fn is_usable(&self) -> bool {
        self.report_state == ChildReportState::Usable
            && self
                .summary
                .as_deref()
                .is_some_and(|summary| !summary.trim().is_empty())
            && self.asks_reconciled
            && self.open_ask_ids.is_empty()
    }
}

/// One daemon-owned deliverable or verification obligation in a task's
/// durable completion contract.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCompletionRequirement {
    pub description: String,
    #[serde(default)]
    pub completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Durable authority for deciding whether a task may become `completed`.
/// Absence on `AgentTask` denotes a legacy implicit contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCompletionContract {
    #[serde(default = "default_completion_contract_version")]
    pub version: u32,
    #[serde(default)]
    pub objective: String,
    #[serde(default)]
    pub required_deliverables: Vec<TaskCompletionRequirement>,
    #[serde(default)]
    pub completed_actions: Vec<String>,
    #[serde(default)]
    pub outstanding_promised_actions: Vec<String>,
    #[serde(default)]
    pub pending_operations: Vec<String>,
    #[serde(default)]
    pub verification_requirements: Vec<TaskCompletionRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    /// The last terminal transition recorded for this contract, including
    /// legitimate failed, cancelled, and budget-exhausted outcomes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_status: Option<TaskStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_result: Option<ChildResultContract>,
    #[serde(default)]
    pub open_completion_attempts: u32,
    #[serde(default)]
    pub dispatch_finish_attempts: u32,
}

pub const OPEN_COMPLETION_CONTRACT_ATTEMPT_LIMIT: u32 = 3;
pub const GOAL_STEP_DISPATCH_FINISH_LIMIT: u32 = 12;

fn default_completion_contract_version() -> u32 {
    1
}

impl Default for TaskCompletionContract {
    fn default() -> Self {
        Self {
            version: default_completion_contract_version(),
            objective: String::new(),
            required_deliverables: Vec::new(),
            completed_actions: Vec::new(),
            outstanding_promised_actions: Vec::new(),
            pending_operations: Vec::new(),
            verification_requirements: Vec::new(),
            blocked_reason: None,
            terminal_status: None,
            child_result: None,
            open_completion_attempts: 0,
            dispatch_finish_attempts: 0,
        }
    }
}

impl TaskCompletionContract {
    /// Build the explicit contract attached to every newly enqueued task.
    /// Legacy rows remain distinguishable because only deserialization of an
    /// absent field yields `None`.
    pub fn for_new_task(objective: impl Into<String>, source: &str) -> Self {
        let mut contract = Self {
            objective: objective.into(),
            ..Self::default()
        };
        if source == "subagent" {
            contract
                .required_deliverables
                .push(TaskCompletionRequirement {
                    description: SUBAGENT_REPORT_REQUIREMENT_DESCRIPTION.into(),
                    completed: false,
                    evidence: None,
                });
        }
        contract
    }

    pub fn satisfy_requirement(&mut self, description: &str, evidence: impl Into<String>) {
        let evidence = evidence.into();
        for requirement in self
            .required_deliverables
            .iter_mut()
            .chain(self.verification_requirements.iter_mut())
            .filter(|requirement| requirement.description == description)
        {
            requirement.completed = true;
            requirement.evidence = Some(evidence.clone());
        }
    }

    pub fn require_child_result_integration(&mut self, child_task_id: &str) {
        let description = format!("{CHILD_RESULT_INTEGRATION_REQUIREMENT_PREFIX}{child_task_id}");
        if self
            .required_deliverables
            .iter()
            .all(|requirement| requirement.description != description)
        {
            self.required_deliverables.push(TaskCompletionRequirement {
                description,
                completed: false,
                evidence: None,
            });
        }
    }

    pub fn acknowledge_child_result_integration(
        &mut self,
        child_task_id: &str,
        evidence: impl Into<String>,
    ) {
        self.satisfy_requirement(
            &format!("{CHILD_RESULT_INTEGRATION_REQUIREMENT_PREFIX}{child_task_id}"),
            evidence,
        );
    }

    pub fn open_completion_reasons(&self) -> Vec<String> {
        let mut reasons = Vec::new();
        reasons.extend(
            self.required_deliverables
                .iter()
                .filter(|item| !item.completed)
                .map(|item| format!("required deliverable: {}", item.description)),
        );
        reasons.extend(
            self.outstanding_promised_actions
                .iter()
                .map(|item| format!("promised action: {item}")),
        );
        reasons.extend(
            self.pending_operations
                .iter()
                .map(|item| format!("pending operation: {item}")),
        );
        reasons.extend(
            self.verification_requirements
                .iter()
                .filter(|item| !item.completed)
                .map(|item| format!("required verification: {}", item.description)),
        );
        if let Some(reason) = self
            .blocked_reason
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
        {
            reasons.push(format!("blocked: {reason}"));
        }
        reasons
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskLogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskLogEntry {
    pub id: String,
    pub timestamp: u64,
    pub level: TaskLogLevel,
    pub phase: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default)]
    pub attempt: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GoalStepReviewVerdict {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GoalVerdictEvidence {
    #[serde(default)]
    pub verifier: String,
    #[serde(default)]
    pub coverage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gaps: Option<String>,
    /// Named metric scores from the verifier (e.g. benchmark results).
    /// Correctness failure ⇒ empty/absent map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scores: Option<std::collections::BTreeMap<String, f64>>,
    /// Set by daemon at persist time; not trusted from the tool caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_new_best: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalFinalReviewRecord {
    pub task_id: String,
    pub goal_run_id: String,
    pub verdict: GoalStepReviewVerdict,
    pub explanation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<GoalVerdictEvidence>,
    pub submitted_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalStepReviewRecord {
    pub task_id: String,
    pub goal_run_id: String,
    pub goal_step_id: String,
    pub verdict: GoalStepReviewVerdict,
    pub explanation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<GoalVerdictEvidence>,
    pub submitted_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub priority: TaskPriority,
    #[serde(default)]
    pub progress: u8,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub notify_on_complete: bool,
    #[serde(default)]
    pub notify_channels: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_step_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_step_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(default = "default_task_runtime")]
    pub runtime: String,
    #[serde(default)]
    pub retry_count: u32,
    #[serde(default = "default_max_task_retries")]
    pub max_retries: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awaiting_approval_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_expires_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub containment_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compensation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compensation_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default)]
    pub logs: Vec<AgentTaskLogEntry>,

    /// Explicit completion authority. `None` preserves legacy behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_contract: Option<TaskCompletionContract>,

    /// Restrict which tools this sub-agent may call. `None` = all tools allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_whitelist: Option<Vec<String>>,
    /// Tools this sub-agent must NOT call. Applied after whitelist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_blacklist: Option<Vec<String>>,
    /// Maximum tokens this sub-agent may consume for its context window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_budget_tokens: Option<u32>,
    /// What to do when the context budget is exceeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_overflow_action: Option<ContextOverflowAction>,
    /// DSL expression for automatic termination (e.g. "timeout(300) OR error_count(3)").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub termination_conditions: Option<String>,
    /// Criteria the sub-agent must satisfy for the step to be considered successful.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_criteria: Option<String>,
    /// Hard time limit in seconds (fallback: 1800 = 30 min).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_secs: Option<u64>,
    /// Supervision configuration for this sub-agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supervisor_config: Option<SupervisorConfig>,

    /// Override provider for this task (from SubAgentDefinition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_provider: Option<String>,
    /// Override model for this task (from SubAgentDefinition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_model: Option<String>,
    /// Override API transport for this task (from SubAgentDefinition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_api_transport: Option<ApiTransport>,
    /// Override system prompt for this task (from SubAgentDefinition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_system_prompt: Option<String>,
    /// The SubAgentDefinition ID this task was spawned from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_agent_def_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunKind {
    Task,
    Subagent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: String,
    pub task_id: String,
    pub kind: AgentRunKind,
    pub classification: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub priority: TaskPriority,
    #[serde(default)]
    pub progress: u8,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_task_runtime")]
    pub runtime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_step_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_step_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl AgentTask {
    pub fn completion_blockers(&self) -> Vec<String> {
        let mut blockers = self
            .completion_contract
            .as_ref()
            .map(TaskCompletionContract::open_completion_reasons)
            .unwrap_or_default();
        if self.is_spawned_subagent() {
            if let Some(result) = self
                .completion_contract
                .as_ref()
                .and_then(|contract| contract.child_result.as_ref())
            {
                if !result.open_ask_ids.is_empty() || !result.asks_reconciled {
                    blockers.push("unreconciled child asks".to_string());
                }
                if result.report_state != ChildReportState::Usable {
                    blockers.push(format!(
                        "child report is {:?}, not usable",
                        result.report_state
                    ));
                }
            }
        }
        blockers
    }

    /// Central guard for terminal task transitions. Open obligations reject
    /// only successful completion; failed, cancelled, and other legitimate
    /// terminal outcomes remain available for incomplete work.
    pub fn transition_to_terminal(
        &mut self,
        requested: TaskStatus,
        now: u64,
    ) -> Result<(), Vec<String>> {
        debug_assert!(requested.is_terminal());
        if requested == TaskStatus::Completed {
            let reasons = self.completion_blockers();
            if !reasons.is_empty() {
                return Err(reasons);
            }
        }

        self.status = requested;
        self.progress = 100;
        self.completed_at = Some(now);
        if let Some(contract) = self.completion_contract.as_mut() {
            contract.terminal_status = Some(requested);
        }
        Ok(())
    }

    pub(crate) fn is_internal_weles_review(&self) -> bool {
        self.sub_agent_def_id.as_deref()
            == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
            && self.source == "subagent"
            && self
                .override_system_prompt
                .as_deref()
                .and_then(crate::agent::weles_governance::parse_weles_internal_override_payload)
                .is_some()
    }

    pub(crate) fn is_spawned_subagent(&self) -> bool {
        self.source == "subagent" || self.parent_task_id.is_some()
    }
}

fn default_source() -> String {
    "user".into()
}

fn default_max_task_retries() -> u32 {
    3
}

fn default_task_runtime() -> String {
    "daemon".into()
}

#[cfg(test)]
mod completion_contract_tests {
    use super::*;

    fn explicit_task() -> AgentTask {
        serde_json::from_value(serde_json::json!({
            "id": "contract-task",
            "title": "Contract task",
            "description": "exercise completion contract",
            "status": "in_progress",
            "created_at": 1,
            "completion_contract": { "objective": "ship verified output" }
        }))
        .expect("minimal task fixture should deserialize with defaults")
    }

    #[test]
    fn completion_rejects_open_deliverables_promises_operations_and_verification() {
        let mut task = explicit_task();
        let contract = task.completion_contract.as_mut().unwrap();
        contract
            .required_deliverables
            .push(TaskCompletionRequirement {
                description: "artifact".into(),
                completed: false,
                evidence: None,
            });
        contract
            .outstanding_promised_actions
            .push("run formatter".into());
        contract.pending_operations.push("operation-1".into());
        contract
            .verification_requirements
            .push(TaskCompletionRequirement {
                description: "focused test".into(),
                completed: false,
                evidence: None,
            });

        let reasons = task
            .transition_to_terminal(TaskStatus::Completed, 10)
            .expect_err("premature completion must be rejected");
        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.completed_at, None);
        assert_eq!(reasons.len(), 4);
    }

    #[test]
    fn completion_succeeds_after_deliverables_and_verification_are_satisfied() {
        let mut task = explicit_task();
        let contract = task.completion_contract.as_mut().unwrap();
        contract
            .required_deliverables
            .push(TaskCompletionRequirement {
                description: "artifact".into(),
                completed: true,
                evidence: Some("artifact.txt".into()),
            });
        contract.completed_actions.push("implemented change".into());
        contract
            .verification_requirements
            .push(TaskCompletionRequirement {
                description: "focused test".into(),
                completed: true,
                evidence: Some("cargo test: pass".into()),
            });

        task.transition_to_terminal(TaskStatus::Completed, 10)
            .expect("satisfied contract should complete");
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.completed_at, Some(10));
        assert_eq!(
            task.completion_contract.unwrap().terminal_status,
            Some(TaskStatus::Completed)
        );
    }

    #[test]
    fn open_contract_preserves_non_success_terminal_paths() {
        for status in [
            TaskStatus::Failed,
            TaskStatus::Cancelled,
            TaskStatus::BudgetExceeded,
        ] {
            let mut task = explicit_task();
            task.completion_contract
                .as_mut()
                .unwrap()
                .pending_operations
                .push("still-running".into());
            task.transition_to_terminal(status, 10)
                .expect("non-success terminal transition must remain legal");
            assert_eq!(task.status, status);
        }
    }

    #[test]
    fn blocked_transition_preserves_contract_and_can_later_fail_or_cancel() {
        let mut task = explicit_task();
        task.completion_contract
            .as_mut()
            .unwrap()
            .pending_operations
            .push("background-build".into());
        task.status = TaskStatus::Blocked;
        task.blocked_reason = Some("waiting for background-build".into());
        assert_eq!(task.completion_blockers().len(), 1);
        assert_eq!(task.completed_at, None);

        task.transition_to_terminal(TaskStatus::Failed, 10)
            .expect("blocked tasks with open work must retain legitimate failure paths");
        assert_eq!(task.status, TaskStatus::Failed);
        assert_eq!(
            task.completion_contract.as_ref().unwrap().terminal_status,
            Some(TaskStatus::Failed)
        );
    }

    #[test]
    fn child_failed_unavailable_and_budget_exhausted_states_do_not_allow_success() {
        for (status, report_state) in [
            (TaskStatus::Failed, ChildReportState::Failed),
            (TaskStatus::Cancelled, ChildReportState::Unavailable),
            (TaskStatus::BudgetExceeded, ChildReportState::Truncated),
        ] {
            let mut task = explicit_task();
            task.source = "subagent".into();
            task.status = TaskStatus::InProgress;
            let mut contract = TaskCompletionContract::for_new_task("child work", "subagent");
            contract.child_result = Some(ChildResultContract {
                terminal_status: Some(status),
                terminal_version: 1,
                report_state,
                asks_reconciled: true,
                parent_notification: ParentNotificationState::Pending,
                ..ChildResultContract::default()
            });
            task.completion_contract = Some(contract);

            let reasons = task
                .transition_to_terminal(TaskStatus::Completed, 10)
                .expect_err("non-usable child terminal states must not become successful");
            assert!(reasons.iter().any(|reason| reason.contains("not usable")));
            task.transition_to_terminal(status, 11)
                .expect("explicit non-success terminal outcome must remain representable");
            assert_eq!(task.status, status);
        }
    }

    #[test]
    fn legacy_task_without_contract_keeps_backward_compatible_completion() {
        let mut task: AgentTask = serde_json::from_value(serde_json::json!({
            "id": "legacy-task",
            "title": "Legacy task",
            "description": "created before contracts",
            "status": "in_progress",
            "created_at": 1
        }))
        .expect("legacy task should deserialize");
        assert!(task.completion_contract.is_none());
        task.transition_to_terminal(TaskStatus::Completed, 10)
            .expect("legacy completion should remain permissive");
        assert_eq!(task.status, TaskStatus::Completed);
    }
}
