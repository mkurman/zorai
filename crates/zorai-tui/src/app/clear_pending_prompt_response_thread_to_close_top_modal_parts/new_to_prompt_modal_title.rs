use super::*;
use crate::client::ClientEvent;
use crate::theme::ThemeTokens;
use crate::widgets;
use crossterm::event::KeyModifiers;
use std::sync::mpsc::Receiver;
use tokio::sync::mpsc::UnboundedSender;
impl TuiModel {
    pub fn new(
        daemon_events_rx: Receiver<ClientEvent>,
        daemon_cmd_tx: UnboundedSender<DaemonCommand>,
    ) -> Self {
        let mut system_monitor_sampler = crate::system_monitor::SystemMonitorSampler::new();
        let system_monitor = system_monitor_sampler.sample();
        let ticks_per_second = (1_000 / TUI_TICK_RATE_MS).max(1);

        Self {
            chat: chat::ChatState::new(),
            input: input::InputState::new(),
            modal: modal::ModalState::new(),
            sidebar: sidebar::SidebarState::new(),
            goal_sidebar: goal_sidebar::GoalSidebarState::new(),
            goal_mission_control: goal_mission_control::GoalMissionControlState::new(),
            goal_composer_saved_defaults: Self::load_goal_composer_saved_defaults(),
            goal_workspace: goal_workspace::GoalWorkspaceState::new(),
            mission_control_navigation: MissionControlNavigationState::default(),
            goal_sidebar_selection_anchor: None,
            tasks: task::TaskState::new(),
            config: config::ConfigState::new(),
            approval: approval::ApprovalState::new(),
            anticipatory: AnticipatoryState::new(),
            audit: crate::state::audit::AuditState::new(),
            notifications: notifications::NotificationsState::new(),
            settings: settings::SettingsState::new(),
            plugin_settings: settings::PluginSettingsState::new(),
            auth: AuthState::new(),
            subagents: SubAgentsState::new(),
            collaboration: CollaborationState::new(),
            concierge: ConciergeState::new(),
            tier: TierState::default(),
            workspace: crate::state::workspace::WorkspaceState::new(),
            focus: FocusArea::Input,
            theme: ThemeTokens::default(),
            width: 120,
            height: 40,
            daemon_cmd_tx,
            daemon_events_rx,
            connected: false,
            agent_config_loaded: false,
            status_line: "Starting...".to_string(),
            default_session_id: None,
            tick_counter: 0,
            next_spawned_sidebar_task_refresh_tick: 0,
            auto_refresh_target: None,
            next_auto_refresh_tick: 0,
            system_monitor,
            system_monitor_sampler,
            next_system_monitor_tick: ticks_per_second * 3,
            image_preview_cache_revision: widgets::image_preview::preview_cache_revision(),
            agent_activity: None,
            thread_agent_activity: std::collections::HashMap::new(),
            bootstrap_pending_activity_threads: std::collections::HashSet::new(),
            pending_prompt_response_threads: std::collections::HashSet::new(),
            deleted_thread_ids: std::collections::HashSet::new(),
            gateway_chats: Vec::new(),
            gateway_link_pending: None,
            gateway_agent_picker_field: "gateway_default_agent".to_string(),
            pending_local_message_delete_reload_suppression: std::collections::HashMap::new(),
            pending_local_message_delete_backfills: std::collections::HashMap::new(),
            pending_local_message_delete_fetches: std::collections::HashMap::new(),
            participant_playground_activity: std::collections::HashMap::new(),
            last_error: None,
            error_active: false,
            error_tick: 0,
            openai_auth_url: None,
            openai_auth_status_text: None,
            settings_picker_target: None,
            model_picker_provider_id: None,
            last_attention_surface: None,
            show_sidebar_override: None,
            main_pane_view: MainPaneView::Conversation,
            task_view_scroll: 0,
            task_show_live_todos: true,
            task_show_timeline: true,
            task_show_files: true,
            pending_quit: false,
            pending_stop: false,
            pending_stop_tick: 0,
            input_notice: None,
            pending_chat_action_confirm: None,
            pending_pinned_budget_exceeded: None,
            pending_pinned_jump: None,
            pending_pinned_shortcut_leader: None,
            chat_action_confirm_accept_selected: true,
            retry_wait_start_selected: false,
            auto_response_selection: AutoResponseActionSelection::Yes,
            held_key_modifiers: KeyModifiers::NONE,
            attachments: Vec::new(),
            voice_recording: false,
            voice_capture_path: None,
            voice_capture_stderr_path: None,
            voice_capture_backend_label: None,
            voice_recorder: None,
            voice_player: None,
            voice_player_ipc: None,
            voice_paused: false,
            voice_player_seq: 0,
            tts_audio_cache: std::collections::HashMap::new(),
            pending_tts_cache_key: None,
            queued_prompts: Vec::new(),
            queued_prompt_action: QueuedPromptAction::SendNow,
            hidden_auto_response_suggestion_ids: std::collections::HashSet::new(),
            operator_profile: OperatorProfileOnboardingState::default(),
            cancelled_thread_id: None,
            pending_new_thread_target_agent: None,
            pending_new_thread_execution_profile: None,
            pending_thread_picker_refresh: None,
            thread_picker_loading_tab: None,
            pending_builtin_persona_setup: None,
            pending_target_agent_config: None,
            pending_svarog_reasoning_effort: None,
            pending_participant_instruction_edit: None,
            participant_actions_target: None,
            thread_loading_id: None,
            missing_runtime_thread_ids: std::collections::HashSet::new(),
            empty_hydrated_runtime_thread_ids: std::collections::HashSet::new(),
            pending_reconnect_restore: None,
            pending_goal_hydration_refreshes: std::collections::HashSet::new(),
            ignore_pending_concierge_welcome: false,
            operator_profile_auto_start_requested: false,
            operator_profile_auto_start_pending_summary: false,
            gateway_statuses: Vec::new(),
            weles_health: None,
            recent_actions: Vec::new(),
            status_modal_snapshot: None,
            status_modal_diagnostics_json: None,
            status_modal_loading: false,
            status_modal_error: None,
            status_modal_scroll: 0,
            statistics_modal_snapshot: None,
            statistics_modal_loading: false,
            statistics_modal_error: None,
            statistics_modal_tab: crate::state::statistics::StatisticsTab::Overview,
            statistics_modal_window: zorai_protocol::AgentStatisticsWindow::All,
            statistics_modal_scroll: 0,
            prompt_modal_snapshot: None,
            prompt_modal_loading: false,
            prompt_modal_error: None,
            prompt_modal_scroll: 0,
            prompt_modal_title_override: None,
            prompt_modal_body_override: None,
            settings_modal_scroll: 0,
            help_modal_scroll: 0,
            chat_drag_anchor: None,
            chat_drag_current: None,
            chat_drag_anchor_point: None,
            chat_drag_current_point: None,
            chat_selection_snapshot: None,
            sidebar_snapshot: None,
            sidebar_body_area: None,
            chat_scrollbar_drag_grab_offset: None,
            file_preview_scrollbar_drag_grab_offset: None,
            work_context_drag_anchor: None,
            work_context_drag_current: None,
            work_context_drag_anchor_point: None,
            work_context_drag_current_point: None,
            task_view_drag_anchor: None,
            task_view_drag_current: None,
            task_view_drag_anchor_point: None,
            task_view_drag_current_point: None,
            workspace_drag_task: None,
            workspace_drag_status: None,
            workspace_drag_start_target: None,
            workspace_board_selection: None,
            workspace_board_scroll: widgets::workspace_board::WorkspaceBoardScroll::default(),
            workspace_expanded_task_ids: std::collections::HashSet::new(),
            pending_workspace_create_workspace_form: None,
            pending_workspace_create_form: None,
            pending_workspace_review_form: None,
            pending_workspace_edit_form: None,
            workspace_edit_modal_scroll: 0,
            pending_workspace_detail_task_id: None,
            pending_workspace_history_task_id: None,
            pending_workspace_actor_picker: None,
        }
    }

    pub(crate) fn send_daemon_command(&self, command: DaemonCommand) {
        let _ = self.daemon_cmd_tx.send(command);
    }

    pub(crate) fn active_always_auto_response_participant(
        &self,
    ) -> Option<&crate::state::chat::ThreadParticipantState> {
        self.chat
            .active_thread()?
            .thread_participants
            .iter()
            .find(|participant| {
                participant.status.eq_ignore_ascii_case("active")
                    && participant.always_auto_response
            })
    }

    pub(crate) fn queued_active_auto_response_suggestion(
        &self,
    ) -> Option<&crate::state::chat::ThreadParticipantSuggestionVm> {
        let thread = self.chat.active_thread()?;
        thread
            .queued_participant_suggestions
            .iter()
            .find(|suggestion| {
                suggestion
                    .suggestion_kind
                    .eq_ignore_ascii_case("auto_response")
                    && suggestion.status.eq_ignore_ascii_case("queued")
                    && !self
                        .hidden_auto_response_suggestion_ids
                        .contains(&suggestion.id)
            })
    }

    pub(crate) fn active_auto_response_suggestion(
        &self,
    ) -> Option<&crate::state::chat::ThreadParticipantSuggestionVm> {
        if self.assistant_busy() || self.active_always_auto_response_participant().is_some() {
            return None;
        }
        self.queued_active_auto_response_suggestion()
    }

    pub(crate) fn active_auto_response_countdown_secs(&self) -> Option<u64> {
        let suggestion = self.active_auto_response_suggestion()?;
        let due_at = suggestion.auto_send_at?;
        let now = Self::current_unix_ms().max(0) as u64;
        Some(due_at.saturating_sub(now).div_ceil(1000))
    }

    pub(crate) fn execute_active_auto_response_action(
        &mut self,
        selection: AutoResponseActionSelection,
    ) -> bool {
        let Some(suggestion) = self.queued_active_auto_response_suggestion().cloned() else {
            return false;
        };
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return false;
        };
        self.hidden_auto_response_suggestion_ids
            .insert(suggestion.id.clone());
        self.auto_response_selection = AutoResponseActionSelection::Yes;
        match selection {
            AutoResponseActionSelection::Yes => {
                self.send_daemon_command(DaemonCommand::SendParticipantSuggestion {
                    thread_id,
                    suggestion_id: suggestion.id,
                });
                self.status_line = format!(
                    "Auto response requested from {}",
                    suggestion.target_agent_name
                );
            }
            AutoResponseActionSelection::No => {
                self.send_daemon_command(DaemonCommand::DismissParticipantSuggestion {
                    thread_id,
                    suggestion_id: suggestion.id,
                });
                self.status_line = "Auto response dismissed".to_string();
            }
            AutoResponseActionSelection::Always => {
                self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
                    thread_id: thread_id.clone(),
                    target_agent_id: suggestion.target_agent_id.clone(),
                    action: "auto_response_always".to_string(),
                    instruction: None,
                    session_id: None,
                });
                self.send_daemon_command(DaemonCommand::SendParticipantSuggestion {
                    thread_id,
                    suggestion_id: suggestion.id,
                });
                self.status_line = format!(
                    "Auto response always enabled for {}",
                    suggestion.target_agent_name
                );
            }
        }
        true
    }

    fn latest_visible_main_agent_message_timestamp_for_auto_response(
        &self,
    ) -> Option<(u64, String)> {
        let thread = self.chat.active_thread()?;
        let latest_message = thread.messages.last()?;
        if latest_message.role != chat::MessageRole::Assistant {
            return None;
        }
        if latest_message.content.trim().is_empty() {
            return None;
        }
        let authored_by_participant =
            latest_message
                .author_agent_id
                .as_ref()
                .is_some_and(|author_id| {
                    thread
                        .thread_participants
                        .iter()
                        .any(|participant| participant.agent_id.eq_ignore_ascii_case(author_id))
                });
        (!authored_by_participant).then(|| {
            (
                latest_message.timestamp,
                latest_message.content.trim().to_string(),
            )
        })
    }

    fn active_auto_response_request_target(
        &self,
    ) -> Option<&crate::state::chat::ThreadParticipantState> {
        let thread = self.chat.active_thread()?;
        if let Some(always_participant) = self.active_always_auto_response_participant() {
            return Some(always_participant);
        }
        thread
            .thread_participants
            .iter()
            .filter(|participant| participant.status.eq_ignore_ascii_case("active"))
            .max_by(|left, right| {
                left.last_contribution_at
                    .unwrap_or(0)
                    .cmp(&right.last_contribution_at.unwrap_or(0))
                    .then_with(|| left.updated_at.cmp(&right.updated_at))
                    .then_with(|| left.created_at.cmp(&right.created_at))
                    .then_with(|| left.agent_id.cmp(&right.agent_id))
            })
    }

    pub(crate) fn maybe_request_auto_response_for_open_thread(&mut self, thread_id: &str) -> bool {
        if self.assistant_busy() || self.chat.active_thread_id() != Some(thread_id) {
            return false;
        }
        let Some((source_message_timestamp, _)) =
            self.latest_visible_main_agent_message_timestamp_for_auto_response()
        else {
            return false;
        };
        let Some(target_participant) = self.active_auto_response_request_target() else {
            return false;
        };
        let Some(thread) = self.chat.active_thread() else {
            return false;
        };
        let has_matching_suggestion =
            thread
                .queued_participant_suggestions
                .iter()
                .any(|suggestion| {
                    suggestion
                        .suggestion_kind
                        .eq_ignore_ascii_case("auto_response")
                        && suggestion.status.eq_ignore_ascii_case("queued")
                        && suggestion
                            .target_agent_id
                            .eq_ignore_ascii_case(&target_participant.agent_id)
                        && suggestion.source_message_timestamp == Some(source_message_timestamp)
                });
        if has_matching_suggestion {
            return false;
        }
        self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
            thread_id: thread_id.to_string(),
            target_agent_id: target_participant.agent_id.clone(),
            action: "auto_response".to_string(),
            instruction: None,
            session_id: None,
        });
        true
    }

    pub(crate) fn maybe_auto_send_always_auto_response(&mut self) -> bool {
        if self.assistant_busy() || self.active_always_auto_response_participant().is_none() {
            return false;
        }
        self.execute_active_auto_response_action(AutoResponseActionSelection::Yes)
    }

    pub(crate) fn open_status_modal_loading(&mut self) {
        self.status_modal_loading = true;
        self.status_modal_snapshot = None;
        self.status_modal_diagnostics_json = None;
        self.status_modal_error = None;
        self.status_modal_scroll = 0;
        if self.modal.top() != Some(modal::ModalKind::Status) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Status));
        }
    }

    pub(crate) fn open_prompt_modal_loading(&mut self) {
        self.prompt_modal_loading = true;
        self.prompt_modal_snapshot = None;
        self.prompt_modal_error = None;
        self.prompt_modal_scroll = 0;
        self.prompt_modal_title_override = None;
        self.prompt_modal_body_override = None;
        if self.modal.top() != Some(modal::ModalKind::PromptViewer) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::PromptViewer));
        }
    }

    pub(crate) fn open_statistics_modal_loading(&mut self) {
        self.statistics_modal_loading = true;
        self.statistics_modal_snapshot = None;
        self.statistics_modal_error = None;
        self.statistics_modal_scroll = 0;
        if self.modal.top() != Some(modal::ModalKind::Statistics) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Statistics));
        }
    }

    pub(crate) fn open_pinned_budget_exceeded_modal(
        &mut self,
        payload: PendingPinnedBudgetExceeded,
    ) {
        self.pending_pinned_budget_exceeded = Some(payload);
        if self.modal.top() != Some(modal::ModalKind::PinnedBudgetExceeded) {
            self.modal.reduce(modal::ModalAction::Push(
                modal::ModalKind::PinnedBudgetExceeded,
            ));
        }
    }

    pub(crate) fn close_pinned_budget_exceeded_modal(&mut self) {
        self.pending_pinned_budget_exceeded = None;
        if self.modal.top() == Some(modal::ModalKind::PinnedBudgetExceeded) {
            self.close_top_modal();
        }
    }

    pub(crate) fn status_modal_body(&self) -> String {
        if self.status_modal_loading {
            return "Loading zorai status...".to_string();
        }
        if let Some(error) = &self.status_modal_error {
            return format!("Status request failed\n====================\n{error}");
        }
        if let Some(snapshot) = &self.status_modal_snapshot {
            return render_helpers::format_status_modal_text(
                snapshot,
                self.status_modal_diagnostics_json.as_deref(),
            );
        }
        "No status available.".to_string()
    }

    pub(crate) fn statistics_modal_body(&self) -> String {
        if self.statistics_modal_loading {
            return "Loading historical statistics...".to_string();
        }
        if let Some(error) = &self.statistics_modal_error {
            return format!("Statistics request failed\n=========================\n{error}");
        }
        if let Some(snapshot) = &self.statistics_modal_snapshot {
            return widgets::statistics::format_statistics_body(
                snapshot,
                self.statistics_modal_tab,
            );
        }
        "No statistics available.".to_string()
    }

    pub(crate) fn prompt_modal_body(&self) -> String {
        if let Some(body) = &self.prompt_modal_body_override {
            return body.clone();
        }
        if self.prompt_modal_loading {
            return "Loading agent prompt...".to_string();
        }
        if let Some(error) = &self.prompt_modal_error {
            return format!("Prompt request failed\n=====================\n{error}");
        }
        if let Some(prompt) = &self.prompt_modal_snapshot {
            return render_helpers::format_prompt_modal_text(prompt);
        }
        "No prompt available.".to_string()
    }

    pub(crate) fn prompt_modal_title(&self) -> &str {
        self.prompt_modal_title_override
            .as_deref()
            .unwrap_or("PROMPT")
    }
}
