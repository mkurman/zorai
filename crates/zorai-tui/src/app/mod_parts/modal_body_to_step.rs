use super::*;

pub(crate) const TUI_TICK_RATE_MS: u64 = 50;
/// Debounce on `maybe_refresh_spawned_sidebar_tasks`. Tick-based, so the
/// real cadence depends on whether the loop is in fast (50 ms) or idle
/// (250 ms) mode. At 200 ticks that's:
///   * fast tick: 200 × 50 ms = 10 s
///   * idle tick: 200 × 250 ms = 50 s
/// Was 20 ticks (1 Hz at fast tick — i.e., 1 full ListTasks/sec while
/// any goal was running). Daemon already broadcasts TaskUpdate events,
/// so this poll is a safety net rather than the primary source of
/// truth — a far longer interval is fine, and the previous setting was
/// the dominant lag source on the goal view.
pub(crate) const SPAWNED_SIDEBAR_TASK_REFRESH_TICKS: u64 = 200;

/// A file attached to the next outgoing message.
#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    pub size_bytes: usize,
    pub payload: AttachmentPayload,
}

#[derive(Debug, Clone)]
pub enum AttachmentPayload {
    Text(String),
    ContentBlock(serde_json::Value),
}

/// A recent autonomous action displayed in the sidebar.
#[derive(Debug, Clone)]
pub struct RecentActionVm {
    pub action_type: String,
    pub summary: String,
    pub timestamp: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PaneLayout {
    pub(crate) chat: Rect,
    pub(crate) sidebar: Option<Rect>,
    pub(crate) concierge: Rect,
    pub(crate) input: Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct ChatFilePreviewTarget {
    pub(crate) path: String,
    pub(crate) repo_root: Option<String>,
    pub(crate) repo_relative_path: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum MainPaneView {
    Conversation,
    Collaboration,
    Workspace,
    Task(sidebar::SidebarItemTarget),
    WorkContext,
    FilePreview(ChatFilePreviewTarget),
    GoalComposer,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MissionControlNavigationState {
    pub(crate) source_goal_target: Option<sidebar::SidebarItemTarget>,
    pub(crate) return_to_goal_target: Option<sidebar::SidebarItemTarget>,
    pub(crate) return_to_thread_id: Option<String>,
    pub(crate) return_to_workspace: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GoalSidebarSelectionAnchor {
    Step(String),
    Checkpoint(String),
    Task(String),
    File { thread_id: String, path: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AutoRefreshTarget {
    Goal(String),
    Workspace(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsPickerTarget {
    Provider,
    Model,
    AudioSttProvider,
    AudioSttModel,
    AudioTtsProvider,
    AudioTtsModel,
    ImageGenerationProvider,
    ImageGenerationModel,
    EmbeddingProvider,
    EmbeddingModel,
    BuiltinPersonaProvider,
    BuiltinPersonaModel,
    CompactionWelesProvider,
    CompactionWelesModel,
    CompactionCustomProvider,
    CompactionCustomModel,
    SubAgentProvider,
    SubAgentModel,
    SubAgentRole,
    SubAgentReasoningEffort,
    TargetAgentProvider,
    TargetAgentModel,
    TargetAgentReasoningEffort,
    ConciergeProvider,
    ConciergeModel,
    ConciergeReasoningEffort,
    SvarogReasoningEffort,
    CompactionWelesReasoningEffort,
    CompactionCustomReasoningEffort,
    OpenRouterPreferredProviders,
    OpenRouterExcludedProviders,
    SubAgentOpenRouterPreferredProviders,
    SubAgentOpenRouterExcludedProviders,
    ConciergeOpenRouterPreferredProviders,
    ConciergeOpenRouterExcludedProviders,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputNoticeKind {
    Warning,
    Success,
    Error,
}

#[derive(Clone, Debug)]
pub(crate) struct InputNotice {
    pub(crate) text: String,
    pub(crate) kind: InputNoticeKind,
    pub(crate) expires_at_tick: u64,
    pub(crate) dismiss_on_interaction: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum PendingConfirmAction {
    RegenerateMessage {
        message_index: usize,
    },
    DeleteMessage {
        message_index: usize,
    },
    DeleteThread {
        thread_id: String,
        title: String,
    },
    StopThread {
        thread_id: String,
        title: String,
    },
    ResumeThread {
        thread_id: String,
        title: String,
    },
    DeleteGoalRun {
        goal_run_id: String,
        title: String,
    },
    PauseGoalRun {
        goal_run_id: String,
        title: String,
    },
    StopGoalRun {
        goal_run_id: String,
        title: String,
    },
    ResumeGoalRun {
        goal_run_id: String,
        title: String,
    },
    GoalReview {
        goal_run_id: String,
        goal_title: String,
        verdict: String,
    },
    ReuseModelAsStt {
        model_id: String,
    },
}

impl PendingConfirmAction {
    pub(crate) fn modal_body(&self) -> String {
        match self {
            PendingConfirmAction::RegenerateMessage { message_index } => {
                format!("Proceed with regenerate for message {}?", message_index + 1)
            }
            PendingConfirmAction::DeleteMessage { message_index } => {
                format!("Proceed with delete for message {}?", message_index + 1)
            }
            PendingConfirmAction::DeleteThread { title, .. } => {
                format!("Delete thread \"{title}\"?")
            }
            PendingConfirmAction::StopThread { title, .. } => {
                format!("Stop thread \"{title}\"?")
            }
            PendingConfirmAction::ResumeThread { title, .. } => {
                format!("Resume thread \"{title}\"?")
            }
            PendingConfirmAction::DeleteGoalRun { title, .. } => {
                format!("Delete goal run \"{title}\"?")
            }
            PendingConfirmAction::PauseGoalRun { title, .. } => {
                format!("Pause goal run \"{title}\"?")
            }
            PendingConfirmAction::StopGoalRun { title, .. } => {
                format!("Stop goal run \"{title}\"?")
            }
            PendingConfirmAction::ResumeGoalRun { title, .. } => {
                format!("Resume goal run \"{title}\"?")
            }
            PendingConfirmAction::GoalReview {
                goal_title,
                verdict,
                ..
            } => match verdict.as_str() {
                "accept" => format!("Accept goal \"{goal_title}\"?"),
                "soft_reject" => {
                    format!("Soft reject goal \"{goal_title}\" and keep the worker going?")
                }
                "hard_reject" => format!("Hard reject and fail goal \"{goal_title}\"?"),
                _ => format!("Review goal \"{goal_title}\"?"),
            },
            PendingConfirmAction::ReuseModelAsStt { model_id } => {
                if model_id == "__mission_control__:next_turn" {
                    "Apply the pending Mission Control roster change on the next turn?".to_string()
                } else if model_id == "__mission_control__:reassign_active_step" {
                    "Reassign the active step with the pending Mission Control roster change?"
                        .to_string()
                } else if model_id == "__mission_control__:restart_active_step" {
                    "Restart the active step with the pending Mission Control roster change?"
                        .to_string()
                } else {
                    "Selected model supports audio. Use it as the STT model too?".to_string()
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BuiltinPersonaSetupConfigSnapshot {
    pub(crate) provider: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) custom_model_name: String,
    pub(crate) api_key: String,
    pub(crate) assistant_id: String,
    pub(crate) auth_source: String,
    pub(crate) api_transport: String,
    pub(crate) custom_context_window_tokens: Option<u32>,
    pub(crate) context_window_tokens: u32,
    pub(crate) fetched_models: Vec<config::FetchedModel>,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingBuiltinPersonaSetup {
    pub(crate) target_agent_id: String,
    pub(crate) target_agent_name: String,
    pub(crate) continuation: PendingBuiltinPersonaSetupContinuation,
    pub(crate) config_snapshot: BuiltinPersonaSetupConfigSnapshot,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingTargetAgentConfig {
    pub(crate) target_agent_id: String,
    pub(crate) target_agent_name: String,
    pub(crate) provider_id: String,
    pub(crate) model: String,
    pub(crate) reasoning_effort: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingThreadExecutionProfile {
    pub(crate) provider_id: String,
    pub(crate) model: String,
    pub(crate) reasoning_effort: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum PendingBuiltinPersonaSetupContinuation {
    SubmitPrompt(String),
    SelectWorkspaceActor {
        pending: PendingWorkspaceActorPicker,
        actor: zorai_protocol::WorkspaceActor,
    },
    AddThreadParticipant {
        thread_id: String,
        agent_id: String,
        agent_name: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct PendingParticipantInstructionEdit {
    pub(crate) thread_id: String,
    pub(crate) agent_id: String,
    pub(crate) agent_name: String,
    pub(crate) is_new: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ParticipantActionsTarget {
    pub(crate) agent_id: String,
    pub(crate) agent_name: String,
    pub(crate) active: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ParticipantPlaygroundActivity {
    pub(crate) visible_thread_id: String,
    pub(crate) participant_agent_id: String,
    pub(crate) participant_agent_name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingPinnedBudgetExceeded {
    pub(crate) current_pinned_chars: usize,
    pub(crate) pinned_budget_chars: usize,
    pub(crate) candidate_pinned_chars: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingPinnedJump {
    pub(crate) thread_id: String,
    pub(crate) message_id: String,
    pub(crate) absolute_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingPinnedShortcutLeader {
    Active,
}

#[derive(Clone, Debug)]
pub(crate) struct QueuedPrompt {
    pub(crate) text: String,
    pub(crate) thread_id: Option<String>,
    pub(crate) prompt_id: Option<String>,
    pub(crate) suggestion_id: Option<String>,
    pub(crate) participant_agent_id: Option<String>,
    pub(crate) participant_agent_name: Option<String>,
    pub(crate) force_send: bool,
    pub(crate) copied_until_tick: Option<u64>,
}

impl QueuedPrompt {
    #[cfg(test)]
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            thread_id: None,
            prompt_id: None,
            suggestion_id: None,
            participant_agent_id: None,
            participant_agent_name: None,
            force_send: false,
            copied_until_tick: None,
        }
    }

    pub(crate) fn new_with_agent(
        text: impl Into<String>,
        thread_id: impl Into<String>,
        suggestion_id: impl Into<String>,
        participant_agent_id: impl Into<String>,
        participant_agent_name: impl Into<String>,
        force_send: bool,
    ) -> Self {
        Self {
            text: text.into(),
            thread_id: Some(thread_id.into()),
            prompt_id: None,
            suggestion_id: Some(suggestion_id.into()),
            participant_agent_id: Some(participant_agent_id.into()),
            participant_agent_name: Some(participant_agent_name.into()),
            force_send,
            copied_until_tick: None,
        }
    }

    pub(crate) fn from_daemon(record: zorai_protocol::QueuedPromptRecord) -> Self {
        Self {
            text: record.content,
            thread_id: Some(record.thread_id),
            prompt_id: Some(record.id),
            suggestion_id: None,
            participant_agent_id: None,
            participant_agent_name: None,
            force_send: false,
            copied_until_tick: None,
        }
    }

    pub(crate) fn is_copied(&self, current_tick: u64) -> bool {
        self.copied_until_tick
            .is_some_and(|expires_at| current_tick < expires_at)
    }

    pub(crate) fn mark_copied(&mut self, expires_at_tick: u64) {
        self.copied_until_tick = Some(expires_at_tick);
    }

    pub(crate) fn clear_expired_copy_feedback(&mut self, current_tick: u64) -> bool {
        if self
            .copied_until_tick
            .is_some_and(|expires_at| current_tick >= expires_at)
        {
            self.copied_until_tick = None;
            return true;
        }
        false
    }

    pub(crate) fn display_text(&self) -> String {
        match self.participant_agent_name.as_deref() {
            Some(agent_name) => format!("{agent_name}: {}", self.text),
            None => self.text.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueuedPromptAction {
    Expand,
    SendNow,
    Copy,
    Delete,
}

impl QueuedPromptAction {
    const ALL: [QueuedPromptAction; 4] = [
        QueuedPromptAction::Expand,
        QueuedPromptAction::SendNow,
        QueuedPromptAction::Copy,
        QueuedPromptAction::Delete,
    ];

    pub(crate) fn step(self, delta: i32) -> Self {
        let current = Self::ALL
            .iter()
            .position(|action| *action == self)
            .unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, (Self::ALL.len() - 1) as i32) as usize;
        Self::ALL[next]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoResponseActionSelection {
    Yes,
    No,
    Always,
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorProfileQuestionVm {
    pub(crate) session_id: String,
    pub(crate) question_id: String,
    pub(crate) field_key: String,
    pub(crate) prompt: String,
    pub(crate) input_kind: String,
    pub(crate) optional: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorProfileProgressVm {
    pub(crate) answered: u32,
    pub(crate) remaining: u32,
    pub(crate) completion_ratio: f64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct OperatorProfileOnboardingState {
    pub(crate) visible: bool,
    pub(crate) loading: bool,
    pub(crate) session_id: Option<String>,
    pub(crate) session_kind: Option<String>,
    pub(crate) question: Option<OperatorProfileQuestionVm>,
    pub(crate) progress: Option<OperatorProfileProgressVm>,
    pub(crate) summary_json: Option<String>,
    pub(crate) warning: Option<String>,
    pub(crate) bool_answer: Option<bool>,
    pub(crate) deferred_session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingReconnectRestore {
    pub(crate) thread_id: String,
    pub(crate) should_resume: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoalApprovalContext {
    pub(crate) approval_id: String,
    pub(crate) goal_run_id: String,
    pub(crate) thread_id: Option<String>,
    pub(crate) goal_title: String,
    pub(crate) step_title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PendingWorkspaceActorPickerTarget {
    Task { task_id: String },
    CreateForm,
    EditForm,
}
