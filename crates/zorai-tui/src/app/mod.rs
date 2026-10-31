//! TuiModel compositor -- delegates to decomposed state modules.
//!
//! This replaces the old monolithic 3,500-line app.rs with a clean
//! compositor that owns the 8 state sub-modules and bridges between
//! the daemon client events and the UI state.

use crate::client::ClientEvent;
use crate::providers;
use crate::state::*;
use crate::theme::ThemeTokens;
use crate::widgets;
#[cfg(test)]
use crossterm::event::ModifierKeyCode;
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear};
use std::process::Child;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

mod commands;
mod config_io;
pub(crate) mod conversion;
mod events;
mod gateway_agent_picker;
mod gateway_link;
mod input_ops;
mod keyboard;
mod modal_handlers;
mod mouse;
mod participant_management;
mod render_helpers;
mod rendering;
mod settings_handlers;
mod thread_handoff;
mod thread_search;
mod workspace_actor_picker;
mod workspace_create_modal;
#[cfg(test)]
mod workspace_create_modal_tests;
mod workspace_create_workspace_modal;
#[cfg(test)]
mod workspace_create_workspace_modal_tests;
mod workspace_detail_modal;
#[cfg(test)]
mod workspace_detail_modal_tests;
mod workspace_edit_modal;
#[cfg(test)]
mod workspace_edit_modal_tests;
mod workspace_history_modal;
mod workspace_review_modal;
#[cfg(test)]
mod workspace_review_modal_tests;
mod workspace_update;

#[path = "mod_parts/modal_body_to_step.rs"]
mod modal_body_to_step;
#[path = "mod_parts/pending_workspace_actor_picker.rs"]
mod pending_workspace_actor_picker;

const MESSAGE_DELETE_BACKFILL_THRESHOLD: usize = 5;
pub(crate) const THREAD_PICKER_REFRESH_DEBOUNCE: Duration = Duration::from_millis(750);

#[derive(Clone, Copy, Debug)]
struct PendingDeleteBackfillFetch {
    message_limit: usize,
    message_offset: usize,
    outstanding_rows: usize,
    requested_at_tick: u64,
}

#[derive(Debug, Clone)]
struct PendingThreadPickerRefresh {
    tab: modal::ThreadPickerTab,
    agent_filter: Option<String>,
    ready_at: Instant,
}

pub struct TuiModel {
    chat: chat::ChatState,
    input: input::InputState,
    modal: modal::ModalState,
    sidebar: sidebar::SidebarState,
    goal_sidebar: goal_sidebar::GoalSidebarState,
    goal_mission_control: goal_mission_control::GoalMissionControlState,
    goal_composer_saved_defaults: Option<Vec<task::GoalAgentAssignment>>,
    goal_workspace: goal_workspace::GoalWorkspaceState,
    mission_control_navigation: MissionControlNavigationState,
    goal_sidebar_selection_anchor: Option<GoalSidebarSelectionAnchor>,
    tasks: task::TaskState,
    config: config::ConfigState,
    approval: approval::ApprovalState,
    anticipatory: AnticipatoryState,
    pub audit: crate::state::audit::AuditState,
    notifications: notifications::NotificationsState,
    settings: settings::SettingsState,
    pub plugin_settings: settings::PluginSettingsState,
    pub auth: AuthState,
    pub subagents: SubAgentsState,
    pub collaboration: CollaborationState,
    pub concierge: ConciergeState,
    pub tier: TierState,
    pub workspace: crate::state::workspace::WorkspaceState,

    focus: FocusArea,
    theme: ThemeTokens,
    width: u16,
    height: u16,

    daemon_cmd_tx: UnboundedSender<DaemonCommand>,
    daemon_events_rx: Receiver<ClientEvent>,

    connected: bool,
    agent_config_loaded: bool,
    status_line: String,
    default_session_id: Option<String>,
    tick_counter: u64,
    next_spawned_sidebar_task_refresh_tick: u64,
    auto_refresh_target: Option<AutoRefreshTarget>,
    next_auto_refresh_tick: u64,
    system_monitor: Option<crate::system_monitor::SystemMonitorDisplay>,
    system_monitor_sampler: crate::system_monitor::SystemMonitorSampler,
    next_system_monitor_tick: u64,
    image_preview_cache_revision: u64,

    agent_activity: Option<String>,
    thread_agent_activity: std::collections::HashMap<String, String>,
    bootstrap_pending_activity_threads: std::collections::HashSet<String>,
    pending_prompt_response_threads: std::collections::HashSet<String>,
    deleted_thread_ids: std::collections::HashSet<String>,
    gateway_chats: Vec<crate::client::GatewayChatVm>,
    gateway_link_pending: Option<String>,
    gateway_agent_picker_field: String,
    pending_local_message_delete_reload_suppression: std::collections::HashMap<String, usize>,
    pending_local_message_delete_backfills: std::collections::HashMap<String, usize>,
    pending_local_message_delete_fetches:
        std::collections::HashMap<String, PendingDeleteBackfillFetch>,
    participant_playground_activity:
        std::collections::HashMap<String, ParticipantPlaygroundActivity>,

    last_error: Option<String>,
    error_active: bool,
    error_tick: u64,

    openai_auth_url: Option<String>,
    openai_auth_status_text: Option<String>,
    settings_picker_target: Option<SettingsPickerTarget>,
    model_picker_provider_id: Option<String>,
    last_attention_surface: Option<String>,

    show_sidebar_override: Option<bool>,
    main_pane_view: MainPaneView,
    task_view_scroll: usize,
    task_show_live_todos: bool,
    task_show_timeline: bool,
    task_show_files: bool,

    pending_quit: bool,

    pending_stop: bool,
    pending_stop_tick: u64,
    input_notice: Option<InputNotice>,
    pending_chat_action_confirm: Option<PendingConfirmAction>,
    pending_pinned_budget_exceeded: Option<PendingPinnedBudgetExceeded>,
    pending_pinned_jump: Option<PendingPinnedJump>,
    pending_pinned_shortcut_leader: Option<PendingPinnedShortcutLeader>,
    chat_action_confirm_accept_selected: bool,
    retry_wait_start_selected: bool,
    auto_response_selection: AutoResponseActionSelection,
    held_key_modifiers: KeyModifiers,

    attachments: Vec<Attachment>,

    voice_recording: bool,
    voice_capture_path: Option<String>,
    voice_capture_stderr_path: Option<String>,
    voice_capture_backend_label: Option<String>,
    voice_recorder: Option<Child>,
    voice_player: Option<Child>,
    voice_player_ipc: Option<std::path::PathBuf>,
    voice_paused: bool,
    voice_player_seq: u64,
    tts_audio_cache: std::collections::HashMap<String, std::path::PathBuf>,
    pending_tts_cache_key: Option<String>,

    queued_prompts: Vec<QueuedPrompt>,
    queued_prompt_action: QueuedPromptAction,
    hidden_auto_response_suggestion_ids: std::collections::HashSet<String>,

    operator_profile: OperatorProfileOnboardingState,

    cancelled_thread_id: Option<String>,

    pending_new_thread_target_agent: Option<String>,
    pending_new_thread_execution_profile: Option<PendingThreadExecutionProfile>,
    pending_thread_picker_refresh: Option<PendingThreadPickerRefresh>,
    thread_picker_loading_tab: Option<modal::ThreadPickerTab>,
    pending_thread_search: Option<thread_search::PendingThreadSearch>,

    pending_builtin_persona_setup: Option<PendingBuiltinPersonaSetup>,
    pending_target_agent_config: Option<PendingTargetAgentConfig>,
    pending_svarog_reasoning_effort: Option<String>,
    pending_participant_instruction_edit: Option<PendingParticipantInstructionEdit>,
    pending_operator_question_other: Option<String>,
    participant_actions_target: Option<ParticipantActionsTarget>,

    thread_loading_id: Option<String>,
    missing_runtime_thread_ids: std::collections::HashSet<String>,
    empty_hydrated_runtime_thread_ids: std::collections::HashSet<String>,
    pending_reconnect_restore: Option<PendingReconnectRestore>,
    pending_goal_hydration_refreshes: std::collections::HashSet<String>,

    ignore_pending_concierge_welcome: bool,
    operator_profile_auto_start_requested: bool,
    operator_profile_auto_start_pending_summary: bool,

    pub gateway_statuses: Vec<chat::GatewayStatusVm>,

    pub weles_health: Option<crate::client::WelesHealthVm>,

    pub recent_actions: Vec<RecentActionVm>,
    status_modal_snapshot: Option<crate::client::AgentStatusSnapshotVm>,
    status_modal_diagnostics_json: Option<String>,
    status_modal_loading: bool,
    status_modal_error: Option<String>,
    status_modal_scroll: usize,
    statistics_modal_snapshot: Option<zorai_protocol::AgentStatisticsSnapshot>,
    statistics_modal_loading: bool,
    statistics_modal_error: Option<String>,
    statistics_modal_tab: crate::state::statistics::StatisticsTab,
    statistics_modal_window: zorai_protocol::AgentStatisticsWindow,
    statistics_modal_scroll: usize,
    prompt_modal_snapshot: Option<crate::client::AgentPromptInspectionVm>,
    prompt_modal_loading: bool,
    prompt_modal_error: Option<String>,
    prompt_modal_scroll: usize,
    prompt_modal_title_override: Option<String>,
    prompt_modal_body_override: Option<String>,
    settings_modal_scroll: usize,
    help_modal_scroll: usize,

    chat_drag_anchor: Option<Position>,
    chat_drag_current: Option<Position>,
    chat_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    chat_drag_current_point: Option<widgets::chat::SelectionPoint>,
    chat_selection_snapshot: Option<widgets::chat::CachedSelectionSnapshot>,
    sidebar_snapshot: Option<widgets::sidebar::CachedSidebarSnapshot>,
    sidebar_body_area: Option<(Rect, Rect)>,
    chat_scrollbar_drag_grab_offset: Option<u16>,
    file_preview_scrollbar_drag_grab_offset: Option<u16>,

    work_context_drag_anchor: Option<Position>,
    work_context_drag_current: Option<Position>,
    work_context_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    work_context_drag_current_point: Option<widgets::chat::SelectionPoint>,

    task_view_drag_anchor: Option<Position>,
    task_view_drag_current: Option<Position>,
    task_view_drag_anchor_point: Option<widgets::chat::SelectionPoint>,
    task_view_drag_current_point: Option<widgets::chat::SelectionPoint>,

    workspace_drag_task: Option<String>,
    workspace_drag_status: Option<zorai_protocol::WorkspaceTaskStatus>,
    workspace_drag_start_target: Option<widgets::workspace_board::WorkspaceBoardHitTarget>,
    workspace_board_selection: Option<widgets::workspace_board::WorkspaceBoardHitTarget>,
    workspace_board_scroll: widgets::workspace_board::WorkspaceBoardScroll,
    workspace_expanded_task_ids: std::collections::HashSet<String>,
    pending_workspace_create_workspace_form:
        Option<workspace_create_workspace_modal::WorkspaceCreateForm>,
    pending_workspace_create_form: Option<workspace_create_modal::WorkspaceCreateTaskForm>,
    pending_workspace_review_form: Option<workspace_review_modal::WorkspaceReviewForm>,
    pending_workspace_edit_form: Option<workspace_edit_modal::WorkspaceEditForm>,
    workspace_edit_modal_scroll: usize,
    pending_workspace_detail_task_id: Option<String>,
    pending_workspace_history_task_id: Option<String>,
    pending_workspace_actor_picker: Option<PendingWorkspaceActorPicker>,
}

#[path = "chat_scrollbar_layout_to_publish_attention_surface_if_changed.rs"]
mod chat_scrollbar_layout_to_publish_attention_surface_if_changed;
#[path = "new_to_prompt_modal_title_to_clear_pending_prompt_response_thread.rs"]
mod new_to_prompt_modal_title_to_clear_pending_prompt_response_thread;
#[path = "send_continue_message_to_set_main_pane_conversation_to_mark_all.rs"]
mod send_continue_message_to_set_main_pane_conversation_to_mark_all;

#[path = "mod_parts/settings_tab_label_to_target_goal_run_id.rs"]
mod settings_tab_label_to_target_goal_run_id;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::conversion;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::mpsc;
    use tokio::sync::mpsc::unbounded_channel;

    fn build_model() -> TuiModel {
        let (_daemon_tx, daemon_rx) = mpsc::channel();
        let (cmd_tx, _cmd_rx) = unbounded_channel();
        TuiModel::new(daemon_rx, cmd_tx)
    }

    fn unauthenticated_entry() -> ProviderAuthEntry {
        ProviderAuthEntry {
            provider_id: zorai_shared::providers::PROVIDER_ID_OPENAI.to_string(),
            provider_name: "OpenAI".to_string(),
            authenticated: false,
            auth_source: "api_key".to_string(),
            model: "gpt-5.4".to_string(),
        }
    }

    fn rendered_chat_area(model: &TuiModel) -> Rect {
        let area = Rect::new(0, 0, model.width, model.height);
        let input_height = model.input_height();
        let anticipatory_height = model.anticipatory_banner_height();
        let concierge_height = model.concierge_banner_height();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(anticipatory_height),
                Constraint::Length(concierge_height),
                Constraint::Length(input_height),
                Constraint::Length(1),
            ])
            .split(area);

        if model.sidebar_visible() {
            let sidebar_pct = if model.width >= 120 { 33 } else { 28 };
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(100 - sidebar_pct),
                    Constraint::Percentage(sidebar_pct),
                ])
                .split(chunks[1])[0]
        } else {
            chunks[1]
        }
    }

    #[path = "background_delta_isolated_to_origin_thread_until_switch_to_background.rs"]
    mod background_delta_isolated_to_origin_thread_until_switch_to_background;
    #[path = "clicking_selected_message_copy_action_copies_that_message_to_click.rs"]
    mod clicking_selected_message_copy_action_copies_that_message_to_click;
    #[path = "drag_selection_copies_expected_text_after_autoscroll_to_status_modal.rs"]
    mod drag_selection_copies_expected_text_after_autoscroll_to_status_modal;
    #[path = "drag_selection_does_not_rebuild_full_transcript_for_every_mouse_event.rs"]
    mod drag_selection_does_not_rebuild_full_transcript_for_every_mouse_event;
    #[path = "goal_composer_add_agent_hotkey_creates_another_role_assignment.rs"]
    mod goal_composer_add_agent_hotkey_creates_another_role_assignment;
    #[path = "goal_sidebar_tab_cycling_stays_to_collaboration_mouse_clicks_select_rows.rs"]
    pub(super) mod goal_sidebar_tab_cycling_stays_to_collaboration_mouse_clicks_select_rows;
    #[path = "input_history_keyboard.rs"]
    mod input_history_keyboard;
    #[path = "migrate_preview_slash_command_sends_daemon_migration_preview_to_start.rs"]
    mod migrate_preview_slash_command_sends_daemon_migration_preview_to_start;
    #[path = "provider_onboarding_requires_loaded_auth_state_to_copy_message_shows.rs"]
    mod provider_onboarding_requires_loaded_auth_state_to_copy_message_shows;
}

pub(crate) use modal_body_to_step::*;
pub(crate) use pending_workspace_actor_picker::*;
pub(crate) use settings_tab_label_to_target_goal_run_id::*;
