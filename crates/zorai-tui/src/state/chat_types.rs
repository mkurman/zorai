#[derive(Debug, Clone, Default)]
pub struct AgentThread {
    pub id: String,
    pub agent_name: Option<String>,
    pub profile_provider: Option<String>,
    pub profile_model: Option<String>,
    pub profile_reasoning_effort: Option<String>,
    pub profile_context_window_tokens: Option<u32>,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<AgentMessage>,
    pub total_message_count: usize,
    pub loaded_message_start: usize,
    pub loaded_message_end: usize,
    pub active_compaction_window_start: Option<usize>,
    pub active_context_window_start: Option<usize>,
    pub active_context_window_end: Option<usize>,
    pub active_context_window_tokens: Option<u64>,
    pub latest_turn_context_tokens: Option<u64>,
    pub pinned_messages: Vec<PinnedThreadMessage>,
    pub older_page_pending: bool,
    pub older_page_request_cooldown_until_tick: Option<u64>,
    pub history_window_expanded: bool,
    pub collapse_deadline_tick: Option<u64>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub thread_participants: Vec<ThreadParticipantState>,
    pub queued_participant_suggestions: Vec<ThreadParticipantSuggestionVm>,
    pub thread_handoff_state: Option<ThreadHandoffState>,
    pub runtime_provider: Option<String>,
    pub runtime_model: Option<String>,
    pub runtime_reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PinnedThreadMessage {
    pub message_id: String,
    pub absolute_index: usize,
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadParticipantState {
    pub agent_id: String,
    pub agent_name: String,
    pub instruction: String,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub deactivated_at: Option<u64>,
    pub last_contribution_at: Option<u64>,
    pub always_auto_response: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadParticipantSuggestionVm {
    pub id: String,
    pub target_agent_id: String,
    pub target_agent_name: String,
    pub instruction: String,
    pub suggestion_kind: String,
    pub force_send: bool,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub auto_send_at: Option<u64>,
    pub source_message_timestamp: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadHandoffFrame {
    pub agent_id: String,
    pub agent_name: String,
    pub entered_at: u64,
    pub linked_thread_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadHandoffState {
    pub origin_agent_id: String,
    pub active_agent_id: String,
    pub responder_stack: Vec<ThreadHandoffFrame>,
    pub pending_approval_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadRuntimeMetadata {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentMessage {
    pub id: Option<String>,
    pub role: MessageRole,
    pub content: String,
    pub content_blocks: Vec<AgentContentBlock>,
    pub reasoning: Option<String>,
    pub author_agent_id: Option<String>,
    pub author_agent_name: Option<String>,
    pub is_operator_question: bool,
    pub operator_question_id: Option<String>,
    pub operator_question_answer: Option<String>,
    pub provider_final_result_json: Option<String>,
    pub tool_name: Option<String>,
    pub tool_arguments: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_status: Option<String>,
    pub weles_review: Option<WelesReviewMetaVm>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tps: Option<f64>,
    pub generation_ms: Option<u64>,
    pub cost: Option<f64>,
    pub is_streaming: bool,
    pub pinned_for_compaction: bool,
    pub message_kind: String,
    pub compaction_strategy: Option<String>,
    pub compaction_payload: Option<String>,
    pub tool_output_preview_path: Option<String>,
    pub timestamp: u64,
    pub actions: Vec<MessageAction>,
    pub is_concierge_welcome: bool,
    pub feedback: Option<zorai_protocol::Reaction>,
}

#[derive(Debug, Clone)]
pub enum AgentContentBlock {
    Text {
        text: String,
    },
    Image {
        url: Option<String>,
        data_url: Option<String>,
        mime_type: Option<String>,
    },
    Audio {
        url: Option<String>,
        data_url: Option<String>,
        mime_type: Option<String>,
    },
    Video {
        url: Option<String>,
        data_url: Option<String>,
        mime_type: Option<String>,
    },
}

impl Default for AgentContentBlock {
    fn default() -> Self {
        Self::Text {
            text: String::new(),
        }
    }
}

pub type WelesReviewMetaVm = crate::client::WelesReviewMetaVm;

#[derive(Debug, Clone, Default)]
pub struct MessageAction {
    pub label: String,
    pub action_type: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
    #[default]
    Unknown,
}

#[derive(Debug, Clone)]
pub struct GatewayStatusVm {
    pub platform: String,
    pub status: String,
    pub last_error: Option<String>,
    pub consecutive_failures: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptMode {
    Compact,
    Tools,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatHitTarget {
    Message(usize),
    ReasoningToggle(usize),
    ToolToggle(usize),
    ToolFilePath {
        message_index: usize,
    },
    MessageImage {
        message_index: usize,
    },
    RetryStartNow,
    RetryStop,
    MessageAction {
        message_index: usize,
        action_index: usize,
    },
    CopyMessage(usize),
    ForkMessage(usize),
    ResendMessage(usize),
    RegenerateMessage(usize),
    PinMessage(usize),
    UnpinMessage(usize),
    DeleteMessage(usize),
    ThumbsUp(usize),
    ThumbsDown(usize),
    ExportMessage(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallStatus {
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone)]
pub struct ToolCallVm {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    pub status: ToolCallStatus,
    pub result: Option<String>,
    pub is_error: bool,
    pub weles_review: Option<WelesReviewMetaVm>,
    pub started_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryPhase {
    Retrying,
    Waiting,
}

#[derive(Debug, Clone)]
pub struct RetryStatusVm {
    pub phase: RetryPhase,
    pub attempt: u32,
    pub max_retries: u32,
    pub delay_ms: u64,
    pub failure_class: String,
    pub message: String,
    pub received_at_tick: u64,
}

pub fn retry_status_shows_prompt_actions(status: &RetryStatusVm) -> bool {
    matches!(status.phase, RetryPhase::Waiting)
        || (matches!(status.phase, RetryPhase::Retrying) && status.failure_class == "rate_limit")
}

#[derive(Debug, Clone)]
pub(super) struct StoredMessageRef {
    pub(super) thread_id: String,
    pub(super) message_id: Option<String>,
    pub(super) absolute_index: usize,
}

impl PartialEq for StoredMessageRef {
    fn eq(&self, other: &Self) -> bool {
        self.thread_id == other.thread_id
            && self.message_id == other.message_id
            && self.absolute_index == other.absolute_index
    }
}

impl Eq for StoredMessageRef {}

impl std::hash::Hash for StoredMessageRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.thread_id.hash(state);
        self.message_id.hash(state);
        self.absolute_index.hash(state);
    }
}

#[derive(Debug, Clone)]
pub(super) struct CopiedMessageFeedback {
    pub(super) message_ref: StoredMessageRef,
    pub(super) expires_at_tick: u64,
}

#[derive(Debug, Clone)]
pub enum ChatAction {
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
        args: String,
        weles_review: Option<WelesReviewMetaVm>,
        /// Persisted daemon `agent_messages.id` for the row that will hold this
        /// tool call, if the daemon has already assigned one. When `Some`, the
        /// chat reducer rewrites the latest matching tool-call message's id so
        /// downstream per-message actions (feedback, pin) target the real row.
        message_id: Option<String>,
    },
    ToolResult {
        thread_id: String,
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
        weles_review: Option<WelesReviewMetaVm>,
        /// Persisted daemon `agent_messages.id` for the tool-result row.
        message_id: Option<String>,
    },
    TurnDone {
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
        /// Persisted daemon `agent_messages.id` for the assistant message that
        /// just finished streaming.
        message_id: Option<String>,
    },
    SetRetryStatus {
        thread_id: String,
        phase: RetryPhase,
        attempt: u32,
        max_retries: u32,
        delay_ms: u64,
        failure_class: String,
        message: String,
        received_at_tick: u64,
    },
    ClearRetryStatus {
        thread_id: String,
    },
    ThreadListReceived(Vec<AgentThread>),
    ThreadDetailReceived(AgentThread),
    ContextWindowUpdated {
        thread_id: String,
        active_context_window_start: usize,
        active_context_window_end: usize,
        active_context_window_tokens: u64,
    },
    InvalidateContextWindow {
        thread_id: String,
    },
    CompactionApplied {
        thread_id: String,
        active_compaction_window_start: usize,
        total_message_count: usize,
    },
    ThreadCreated {
        thread_id: String,
        title: String,
    },
    ThreadTitleUpdated {
        thread_id: String,
        title: String,
    },
    ThreadDeleted {
        thread_id: String,
    },
    AppendMessage {
        thread_id: String,
        message: AgentMessage,
    },
    UnpinMessageForCompaction {
        thread_id: String,
        message_id: String,
        absolute_index: Option<usize>,
    },
    ClearThread {
        thread_id: String,
    },
    DismissConciergeWelcome,
    SelectThread(String),
    ScrollChat(i32),
    PinMessageTop(usize),
    NewThread,
    SetTranscriptMode(TranscriptMode),
    ResetStreaming,
    ForceStopStreaming,
}
