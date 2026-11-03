use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use super::goal_proof_check_record_to_output_line::{GoalRunDossier, GoalRunEvent, GoalRunStep};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WelesReviewMetaVm {
    #[serde(default)]
    pub weles_reviewed: bool,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub audit_id: Option<String>,
    #[serde(default)]
    pub security_override_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentMessage {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub role: MessageRole,
    #[serde(default)]
    pub content: String,

    #[serde(default)]
    pub content_blocks: Vec<AgentContentBlock>,

    #[serde(default)]
    pub reasoning: Option<String>,

    #[serde(default)]
    pub is_operator_question: bool,
    #[serde(default)]
    pub operator_question_id: Option<String>,
    #[serde(default)]
    pub operator_question_answer: Option<String>,

    #[serde(default)]
    pub provider_final_result_json: Option<String>,

    #[serde(default)]
    pub author_agent_id: Option<String>,
    #[serde(default)]
    pub author_agent_name: Option<String>,

    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_arguments: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_status: Option<String>,
    #[serde(default)]
    pub weles_review: Option<WelesReviewMetaVm>,

    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub tps: Option<f64>,
    #[serde(default)]
    pub generation_ms: Option<u64>,
    #[serde(default)]
    pub cost: Option<f64>,

    #[serde(default)]
    pub is_streaming: bool,
    #[serde(default)]
    pub pinned_for_compaction: bool,
    #[serde(default)]
    pub message_kind: String,
    #[serde(default)]
    pub compaction_strategy: Option<String>,
    #[serde(default)]
    pub compaction_payload: Option<String>,
    #[serde(default)]
    pub tool_output_preview_path: Option<String>,
    #[serde(default)]
    pub timestamp: u64,
    #[serde(default)]
    pub feedback: Option<zorai_protocol::Reaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentContentBlock {
    Text {
        #[serde(default)]
        text: String,
    },
    Image {
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        data_url: Option<String>,
        #[serde(default)]
        mime_type: Option<String>,
    },
    Audio {
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        data_url: Option<String>,
        #[serde(default)]
        mime_type: Option<String>,
    },
    Video {
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        data_url: Option<String>,
        #[serde(default)]
        mime_type: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentThread {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub profile_provider: Option<String>,
    #[serde(default)]
    pub profile_model: Option<String>,
    #[serde(default)]
    pub profile_reasoning_effort: Option<String>,
    #[serde(default)]
    pub profile_context_window_tokens: Option<u32>,
    #[serde(default)]
    pub title: String,

    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,

    #[serde(default)]
    pub messages: Vec<AgentMessage>,

    #[serde(default)]
    pub total_message_count: usize,
    #[serde(default)]
    pub loaded_message_start: usize,
    #[serde(default)]
    pub loaded_message_end: usize,
    #[serde(default)]
    pub active_context_window_start: Option<usize>,
    #[serde(default)]
    pub active_context_window_end: Option<usize>,
    #[serde(default)]
    pub active_context_window_tokens: Option<u64>,
    #[serde(default)]
    pub pinned_messages: Vec<PinnedThreadMessage>,

    #[serde(default)]
    pub total_input_tokens: u64,
    #[serde(default)]
    pub total_output_tokens: u64,

    #[serde(default)]
    pub thread_participants: Vec<ThreadParticipantState>,

    #[serde(default)]
    pub queued_participant_suggestions: Vec<ThreadParticipantSuggestion>,

    #[serde(default)]
    pub thread_handoff_state: Option<ThreadHandoffState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ThreadHandoffFrame {
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub agent_name: String,
    #[serde(default)]
    pub entered_at: u64,
    #[serde(default)]
    pub linked_thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ThreadHandoffState {
    #[serde(default)]
    pub origin_agent_id: String,
    #[serde(default)]
    pub active_agent_id: String,
    #[serde(default)]
    pub responder_stack: Vec<ThreadHandoffFrame>,
    #[serde(default)]
    pub pending_approval_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PinnedThreadMessage {
    #[serde(default)]
    pub message_id: String,
    #[serde(default)]
    pub absolute_index: usize,
    #[serde(default)]
    pub role: MessageRole,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThreadParticipantState {
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub agent_name: String,
    #[serde(default)]
    pub instruction: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub deactivated_at: Option<u64>,
    #[serde(default)]
    pub last_contribution_at: Option<u64>,
    #[serde(default)]
    pub always_auto_response: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThreadParticipantSuggestion {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub target_agent_id: String,
    #[serde(default)]
    pub target_agent_name: String,
    #[serde(default)]
    pub instruction: String,
    #[serde(default)]
    pub suggestion_kind: String,
    #[serde(default)]
    pub force_send: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub auto_send_at: Option<u64>,
    #[serde(default)]
    pub source_message_timestamp: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

const PERSONA_ID_MARKER: &str = "Agent persona id:";
const WELES_AGENT_ID: &str = "weles";
const WELES_GOVERNANCE_SCOPE: &str = "governance";
const WELES_VITALITY_SCOPE: &str = "vitality";

fn extract_persona_id(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let (marker, value) = line.split_once(':')?;
        if marker.trim() != PERSONA_ID_MARKER.trim_end_matches(':') {
            return None;
        }
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    })
}

pub fn is_weles_thread(thread: &AgentThread) -> bool {
    thread.messages.iter().any(|message| {
        extract_persona_id(&message.content).is_some_and(|persona_id| {
            matches!(
                persona_id.as_str(),
                WELES_AGENT_ID | WELES_GOVERNANCE_SCOPE | WELES_VITALITY_SCOPE
            )
        })
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    InProgress,
    AwaitingApproval,
    Blocked,
    FailedAnalyzing,
    BudgetExceeded,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTask {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub parent_thread_id: Option<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub progress: u8,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub goal_run_id: Option<String>,
    #[serde(default)]
    pub goal_step_title: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub awaiting_approval_id: Option<String>,
    #[serde(default)]
    pub blocked_reason: Option<String>,
    #[serde(default)]
    pub sub_agent_def_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalRunStatus {
    Queued,
    Planning,
    Running,
    AwaitingApproval,
    AwaitingReview,
    Paused,
    Blocked,
    Completed,
    Failed,
    Cancelled,
    Contained,
    Compensated,
    PartiallyCompensated,
    BreakGlass,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GoalRuntimeOwnerProfile {
    pub agent_label: String,
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GoalAgentAssignment {
    #[serde(default)]
    pub role_id: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub inherit_from_main: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalRunModelUsage {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub request_count: u64,
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub estimated_cost_usd: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

pub(super) fn deserialize_goal_binding<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(value) => Ok(value),
        serde_json::Value::Object(map) if map.len() == 1 => {
            let (kind, payload) = map.into_iter().next().expect("validated length");
            let payload = payload
                .as_str()
                .ok_or_else(|| D::Error::custom("goal binding payload must be a string"))?;
            Ok(format!("{kind}:{payload}"))
        }
        other => Err(D::Error::custom(format!(
            "unsupported goal binding payload: {other}"
        ))),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalRun {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub root_thread_id: Option<String>,
    #[serde(default)]
    pub active_thread_id: Option<String>,
    #[serde(default)]
    pub execution_thread_ids: Vec<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub status: Option<GoalRunStatus>,
    #[serde(default)]
    pub current_step_title: Option<String>,
    #[serde(default)]
    pub launch_assignment_snapshot: Vec<GoalAgentAssignment>,
    #[serde(default)]
    pub runtime_assignment_list: Vec<GoalAgentAssignment>,
    #[serde(default)]
    pub planner_owner_profile: Option<GoalRuntimeOwnerProfile>,
    #[serde(default)]
    pub current_step_owner_profile: Option<GoalRuntimeOwnerProfile>,
    #[serde(default)]
    pub total_prompt_tokens: u64,
    #[serde(default)]
    pub total_completion_tokens: u64,
    #[serde(default)]
    pub estimated_cost_usd: Option<f64>,
    #[serde(default)]
    pub model_usage: Vec<GoalRunModelUsage>,
    #[serde(default)]
    pub child_task_count: u32,
    #[serde(default)]
    pub approval_count: u32,
    #[serde(default)]
    pub awaiting_approval_id: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub pending_review_report: Option<String>,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub current_step_index: usize,
    #[serde(default)]
    pub reflection_summary: Option<String>,
    #[serde(default)]
    pub memory_updates: Vec<String>,
    #[serde(default)]
    pub generated_skill_path: Option<String>,
    #[serde(default)]
    pub child_task_ids: Vec<String>,
    #[serde(default)]
    pub loaded_step_start: usize,
    #[serde(default)]
    pub loaded_step_end: usize,
    #[serde(default)]
    pub total_step_count: usize,
    #[serde(default)]
    pub loaded_event_start: usize,
    #[serde(default)]
    pub loaded_event_end: usize,
    #[serde(default)]
    pub total_event_count: usize,
    #[serde(default)]
    pub steps: Vec<GoalRunStep>,
    #[serde(default)]
    pub events: Vec<GoalRunEvent>,
    #[serde(default)]
    pub dossier: Option<GoalRunDossier>,
    #[serde(skip)]
    pub sparse_update: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalEvidenceRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub captured_at: Option<u64>,
}
