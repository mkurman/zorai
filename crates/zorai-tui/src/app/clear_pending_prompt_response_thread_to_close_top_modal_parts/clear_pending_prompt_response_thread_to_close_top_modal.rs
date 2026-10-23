use super::*;
use crate::widgets;
impl TuiModel {
    pub(crate) fn clear_pending_prompt_response_thread(&mut self, thread_id: &str) {
        self.pending_prompt_response_threads.remove(thread_id);
    }

    pub(crate) fn should_preserve_pending_activity_on_reload(&self, thread_id: &str) -> bool {
        if !self.thread_agent_activity.contains_key(thread_id) || self.chat.is_streaming() {
            return false;
        }

        if self.pending_prompt_response_threads.contains(thread_id) {
            return true;
        }

        if self.bootstrap_pending_activity_threads.contains(thread_id) {
            return true;
        }

        if self.chat.active_thread_id() == Some(thread_id) && self.chat.has_running_tool_calls() {
            return true;
        }

        self.chat.active_thread_id() == Some(thread_id)
            && self
                .chat
                .active_thread()
                .and_then(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .find(|message| {
                            !message.content.trim().is_empty()
                                && !matches!(message.role, chat::MessageRole::System)
                        })
                        .map(|message| message.role == chat::MessageRole::User)
                })
                .unwrap_or(false)
    }

    pub(crate) fn done_arrived_before_pending_prompt_output(&self, thread_id: &str) -> bool {
        self.pending_prompt_response_threads.contains(thread_id)
            && self
                .thread_agent_activity
                .get(thread_id)
                .map(String::as_str)
                == Some("thinking")
            && !self.chat.is_thread_streaming(thread_id)
            && self
                .chat
                .threads()
                .iter()
                .find(|thread| thread.id == thread_id)
                .and_then(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .find(|message| {
                            !message.content.trim().is_empty()
                                && !matches!(message.role, chat::MessageRole::System)
                        })
                        .map(|message| message.role == chat::MessageRole::User)
                })
                .unwrap_or(false)
    }

    pub(crate) fn clear_agent_activity_for(&mut self, thread_id: Option<&str>) {
        if let Some(thread_id) = thread_id {
            self.thread_agent_activity.remove(thread_id);
        } else {
            self.agent_activity = None;
        }
    }

    pub(crate) fn clear_active_thread_activity(&mut self) {
        let thread_id = self.chat.active_thread_id().map(str::to_string);
        if let Some(thread_id) = thread_id.as_deref() {
            self.clear_pending_prompt_response_thread(thread_id);
        }
        self.clear_agent_activity_for(thread_id.as_deref());
    }

    pub(crate) fn clear_all_agent_activity(&mut self) {
        self.agent_activity = None;
        self.thread_agent_activity.clear();
        self.pending_prompt_response_threads.clear();
    }

    pub(crate) fn clear_matching_agent_activity(&mut self, target: &str) {
        if self.agent_activity.as_deref() == Some(target) {
            self.agent_activity = None;
        }
        self.thread_agent_activity
            .retain(|_, activity| activity != target);
    }

    pub(crate) fn assistant_busy(&self) -> bool {
        self.chat.is_streaming() || self.current_thread_agent_activity().is_some()
    }

    pub(crate) fn should_queue_submitted_prompt(&self) -> bool {
        self.chat.is_streaming()
    }

    pub(crate) fn clear_expired_queued_prompt_copy_feedback(&mut self) -> bool {
        let mut changed = false;
        for prompt in &mut self.queued_prompts {
            changed |= prompt.clear_expired_copy_feedback(self.tick_counter);
        }
        changed
    }

    pub(crate) fn sync_queued_prompt_modal_state(&mut self) {
        if self.modal.top() != Some(modal::ModalKind::QueuedPrompts) {
            return;
        }

        if self.queued_prompts.is_empty() {
            self.close_top_modal();
            return;
        }

        self.modal.set_picker_item_count(self.queued_prompts.len());
    }

    pub(crate) fn actions_bar_visible(&self) -> bool {
        if !matches!(self.main_pane_view, MainPaneView::Conversation) {
            return false;
        }
        if self.should_show_local_landing() {
            return false;
        }
        if self.should_show_operator_profile_onboarding() {
            return false;
        }

        self.concierge.loading
            || !self.chat.active_actions().is_empty()
            || self.footer_activity_text().is_some()
    }

    pub(crate) fn should_show_daemon_connection_loading(&self) -> bool {
        (!self.connected || !self.agent_config_loaded)
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && !self.should_show_provider_onboarding()
    }

    pub(crate) fn should_show_local_landing(&self) -> bool {
        self.connected
            && self.agent_config_loaded
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && !self.has_mission_control_return_target()
            && !self.chat.is_streaming()
            && !self.concierge.loading
            && !self.should_show_provider_onboarding()
    }

    pub(crate) fn should_show_concierge_hero_loading(&self) -> bool {
        self.concierge.loading
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && self.chat.streaming_content().is_empty()
            && !self.concierge.has_active_welcome()
    }

    pub(crate) fn concierge_banner_height(&self) -> u16 {
        if self.should_show_concierge_hero_loading() {
            0
        } else if self.actions_bar_visible() {
            2
        } else {
            0
        }
    }

    pub(crate) fn anticipatory_banner_height(&self) -> u16 {
        0
    }

    pub(crate) fn pane_layout_for_area(&self, area: Rect) -> PaneLayout {
        let input_height = self.input_height().min(area.height.saturating_sub(1));
        let remaining_after_input = area.height.saturating_sub(input_height + 1);
        let anticipatory_height = self
            .anticipatory_banner_height()
            .min(remaining_after_input.saturating_sub(1));
        let remaining_after_anticipatory =
            remaining_after_input.saturating_sub(anticipatory_height);
        let concierge_height = self
            .concierge_banner_height()
            .min(remaining_after_anticipatory.saturating_sub(1));
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

        let body = chunks[1];
        let chat = if self.sidebar_visible() {
            let sidebar_pct = if self.width >= 120 { 33 } else { 28 };
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(100 - sidebar_pct),
                    Constraint::Percentage(sidebar_pct),
                ])
                .split(body)[0]
        } else {
            body
        };
        let sidebar = if self.sidebar_visible() {
            let sidebar_pct = if self.width >= 120 { 33 } else { 28 };
            Some(
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(100 - sidebar_pct),
                        Constraint::Percentage(sidebar_pct),
                    ])
                    .split(body)[1],
            )
        } else {
            None
        };

        PaneLayout {
            chat,
            sidebar,
            concierge: chunks[3],
            input: chunks[4],
        }
    }

    pub(crate) fn pane_layout(&self) -> PaneLayout {
        self.pane_layout_for_area(Rect::new(0, 0, self.width, self.height))
    }

    fn has_configured_provider(&self) -> bool {
        self.auth.entries.iter().any(|entry| entry.authenticated)
    }

    pub(crate) fn should_show_provider_onboarding(&self) -> bool {
        self.connected
            && self.auth.loaded
            && !self.has_configured_provider()
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && self.chat.streaming_content().is_empty()
    }

    pub(crate) fn should_show_operator_profile_onboarding(&self) -> bool {
        self.operator_profile.visible
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.streaming_content().is_empty()
    }

    fn operator_profile_select_options(field_key: &str) -> Option<&'static [&'static str]> {
        match field_key {
            "notification_preference" => Some(&["minimal", "balanced", "proactive"]),
            _ => None,
        }
    }

    pub(crate) fn normalize_operator_profile_input_kind(input_kind: &str) -> &str {
        match input_kind {
            "boolean" | "bool" => "bool",
            "select" => "select",
            _ => "text",
        }
    }

    pub(crate) fn current_operator_profile_select_options(
        &self,
    ) -> Option<&'static [&'static str]> {
        self.operator_profile
            .question
            .as_ref()
            .and_then(|question| {
                if Self::normalize_operator_profile_input_kind(&question.input_kind) == "bool" {
                    Some(&["yes", "no"][..])
                } else {
                    Self::operator_profile_select_options(&question.field_key)
                }
            })
    }

    pub(crate) fn operator_profile_onboarding_view(
        &self,
    ) -> widgets::operator_profile_onboarding::OperatorProfileOnboardingView<'_> {
        let question = self.operator_profile.question.as_ref().map(|question| {
            widgets::operator_profile_onboarding::OperatorProfileQuestionView {
                prompt: question.prompt.as_str(),
                input_kind: question.input_kind.as_str(),
            }
        });
        let progress = self.operator_profile.progress.as_ref().map(|progress| {
            widgets::operator_profile_onboarding::OperatorProfileProgressView {
                answered: progress.answered,
                remaining: progress.remaining,
                completion_ratio: progress.completion_ratio,
            }
        });
        widgets::operator_profile_onboarding::OperatorProfileOnboardingView { question, progress }
    }

    pub(crate) fn is_current_operator_profile_bool_question(&self) -> bool {
        self.operator_profile
            .question
            .as_ref()
            .is_some_and(|question| {
                Self::normalize_operator_profile_input_kind(&question.input_kind) == "bool"
            })
    }

    fn set_operator_profile_bool_answer(&mut self, value: bool) -> bool {
        if !self.is_current_operator_profile_bool_question() {
            return false;
        }
        self.operator_profile.bool_answer = Some(value);
        let target = if value { 0 } else { 1 };
        let current = self.modal.picker_cursor();
        self.modal
            .reduce(modal::ModalAction::Navigate(target as i32 - current as i32));
        self.focus = FocusArea::Chat;
        true
    }

    pub(crate) fn open_operator_profile_onboarding_modal(&mut self) {
        if self.is_current_operator_profile_bool_question() {
            self.input.set_text("");
        }
        if self.modal.top() == Some(modal::ModalKind::OperatorProfileOnboarding) {
            self.sync_operator_profile_onboarding_item_count();
            return;
        }
        self.modal.reduce(modal::ModalAction::RemoveAll(
            modal::ModalKind::OperatorProfileOnboarding,
        ));
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::OperatorProfileOnboarding,
        ));
        self.sync_operator_profile_onboarding_item_count();
    }

    pub(crate) fn close_operator_profile_onboarding_modal(&mut self) {
        self.modal.reduce(modal::ModalAction::RemoveAll(
            modal::ModalKind::OperatorProfileOnboarding,
        ));
    }

    pub(crate) fn sync_operator_profile_onboarding_item_count(&mut self) {
        let view = self.operator_profile_onboarding_view();
        let count = widgets::operator_profile_onboarding::item_count(&view);
        self.modal.set_picker_item_count(count);
    }

    pub(crate) fn execute_operator_profile_onboarding_target(
        &mut self,
        target: widgets::operator_profile_onboarding::OperatorProfileOnboardingHitTarget,
    ) -> bool {
        match target {
            widgets::operator_profile_onboarding::OperatorProfileOnboardingHitTarget::BoolChoice(
                value,
            ) => {
                self.set_operator_profile_bool_answer(value);
                self.submit_operator_profile_answer()
            }
            widgets::operator_profile_onboarding::OperatorProfileOnboardingHitTarget::Submit => {
                self.submit_operator_profile_answer()
            }
            widgets::operator_profile_onboarding::OperatorProfileOnboardingHitTarget::Skip => {
                self.skip_operator_profile_question()
            }
            widgets::operator_profile_onboarding::OperatorProfileOnboardingHitTarget::Defer => {
                self.defer_operator_profile_question()
            }
        }
    }

    pub(crate) fn submit_operator_profile_answer(&mut self) -> bool {
        let Some(question) = self.operator_profile.question.clone() else {
            return false;
        };
        let answer = self.input.buffer().trim();
        let input_kind = Self::normalize_operator_profile_input_kind(&question.input_kind);
        if answer.is_empty() && !question.optional && input_kind != "bool" {
            self.show_input_notice(
                "Answer required (Ctrl+S to skip, Ctrl+D to defer)",
                InputNoticeKind::Warning,
                80,
                true,
            );
            return true;
        }

        let answer_json = if answer.is_empty() && question.optional {
            "null".to_string()
        } else {
            match input_kind {
                "bool" => self
                    .operator_profile
                    .bool_answer
                    .unwrap_or(true)
                    .to_string(),
                "select" => {
                    let normalized = answer.to_ascii_lowercase();
                    if let Some(options) =
                        Self::operator_profile_select_options(&question.field_key)
                    {
                        if !options.iter().any(|option| *option == normalized) {
                            self.show_input_notice(
                                format!(
                                    "Pick one: {}",
                                    options
                                        .iter()
                                        .map(|option| option.to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                                InputNoticeKind::Warning,
                                100,
                                true,
                            );
                            return true;
                        }
                    }
                    match serde_json::to_string(&normalized) {
                        Ok(json) => json,
                        Err(_) => return false,
                    }
                }
                _ => match serde_json::to_string(answer) {
                    Ok(json) => json,
                    Err(_) => return false,
                },
            }
        };

        self.operator_profile.loading = true;
        self.operator_profile.question = None;
        self.operator_profile.bool_answer = None;
        self.operator_profile.warning = None;
        self.send_daemon_command(DaemonCommand::SubmitOperatorProfileAnswer {
            session_id: question.session_id,
            question_id: question.question_id,
            answer_json,
        });
        self.input.reduce(input::InputAction::Clear);
        self.status_line = "Submitting operator profile answer…".to_string();
        true
    }

    pub(crate) fn skip_operator_profile_question(&mut self) -> bool {
        let Some(question) = self.operator_profile.question.clone() else {
            return false;
        };
        self.operator_profile.loading = true;
        self.operator_profile.question = None;
        self.operator_profile.bool_answer = None;
        self.operator_profile.warning = None;
        self.send_daemon_command(DaemonCommand::SkipOperatorProfileQuestion {
            session_id: question.session_id,
            question_id: question.question_id,
            reason: Some("tui_skip_shortcut".to_string()),
        });
        self.input.reduce(input::InputAction::Clear);
        self.status_line = "Skipping operator profile question…".to_string();
        true
    }

    pub(crate) fn defer_operator_profile_question(&mut self) -> bool {
        let Some(question) = self.operator_profile.question.clone() else {
            return false;
        };
        let defer_until_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_millis() as u64 + 24 * 60 * 60 * 1_000);
        self.operator_profile.visible = false;
        self.operator_profile.loading = false;
        self.operator_profile.deferred_session_id = Some(question.session_id.clone());
        self.operator_profile.session_id = None;
        self.operator_profile.question = None;
        self.operator_profile.bool_answer = None;
        self.operator_profile.warning = None;
        self.close_operator_profile_onboarding_modal();
        self.send_daemon_command(DaemonCommand::DeferOperatorProfileQuestion {
            session_id: question.session_id,
            question_id: question.question_id,
            defer_until_unix_ms,
        });
        self.input.reduce(input::InputAction::Clear);
        self.status_line = "Deferring operator profile onboarding for 24h…".to_string();
        true
    }
    pub(crate) fn open_settings_tab(&mut self, tab: SettingsTab) {
        if self.modal.top() != Some(modal::ModalKind::Settings) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        }
        self.settings.reduce(SettingsAction::SwitchTab(tab));
        self.settings_modal_scroll = 0;
        self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
        self.send_daemon_command(DaemonCommand::GetOpenAICodexAuthStatus);
        self.send_daemon_command(DaemonCommand::ListSubAgents);
        self.send_daemon_command(DaemonCommand::GetConciergeConfig);
        if matches!(tab, SettingsTab::Gateway) {
            self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
        } else if matches!(tab, SettingsTab::Plugins) {
            self.plugin_settings.list_mode = true;
            self.send_daemon_command(DaemonCommand::PluginList);
        } else if matches!(tab, SettingsTab::Database) {
            self.send_daemon_command(DaemonCommand::DatabaseGetBackend);
        } else if matches!(tab, SettingsTab::Mlflow) {
            self.send_daemon_command(DaemonCommand::GetMlflowTracingStatus);
        }
    }

    pub(crate) fn open_provider_setup(&mut self) {
        self.open_settings_tab(SettingsTab::Agent);
        self.status_line = "Configure provider credentials to start chatting".to_string();
    }

    pub(crate) fn set_input_text(&mut self, text: &str) {
        self.input.reduce(input::InputAction::Clear);
        for ch in text.chars() {
            self.input.reduce(input::InputAction::InsertChar(ch));
        }
        self.input.set_mode(input::InputMode::Insert);
        self.sync_goal_mission_control_prompt_from_input();
    }

    pub(crate) fn close_top_modal(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::OpenAIAuth) {
            self.openai_auth_url = None;
            self.openai_auth_status_text = None;
        }
        if self.modal.top() == Some(modal::ModalKind::PromptViewer) {
            self.prompt_modal_title_override = None;
            self.prompt_modal_body_override = None;
            self.prompt_modal_scroll = 0;
        }
        if self.modal.top() == Some(modal::ModalKind::WhatsAppLink) {
            if self.modal.whatsapp_link().phase() != modal::WhatsAppLinkPhase::Connected {
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStop);
            }
            self.send_daemon_command(DaemonCommand::WhatsAppLinkUnsubscribe);
            self.modal.reset_whatsapp_link();
        }
        if self.modal.top() == Some(modal::ModalKind::PinnedBudgetExceeded) {
            self.pending_pinned_budget_exceeded = None;
        }
        if self.modal.top() == Some(modal::ModalKind::ThreadPicker) {
            self.pending_thread_picker_refresh = None;
            self.thread_picker_loading_tab = None;
            self.pending_thread_search = None;
        }
        if self.modal.top() == Some(modal::ModalKind::Settings) {
            self.settings_modal_scroll = 0;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceActorPicker) {
            self.pending_workspace_actor_picker = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceCreate) {
            self.pending_workspace_create_workspace_form = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceCreateTask) {
            self.pending_workspace_create_form = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceReviewTask) {
            self.pending_workspace_review_form = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceEditTask) {
            self.pending_workspace_edit_form = None;
            self.workspace_edit_modal_scroll = 0;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceTaskDetail) {
            self.pending_workspace_detail_task_id = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceTaskHistory) {
            self.pending_workspace_history_task_id = None;
        }
        self.modal.reduce(modal::ModalAction::Pop);
    }
}
