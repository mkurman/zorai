use super::*;
use crate::widgets;
impl TuiModel {
    pub(crate) fn dismiss_active_main_pane(&mut self, focus: FocusArea) -> bool {
        match &self.main_pane_view {
            MainPaneView::Task(target) => {
                if let sidebar::SidebarItemTarget::Task { task_id } = target {
                    if let Some(parent_target) = self.parent_goal_target_for_task(task_id) {
                        self.open_sidebar_target(parent_target);
                        self.focus = focus;
                        return true;
                    }
                }
                if let Some(thread_id) = self.target_thread_id(target) {
                    if self.tasks.selected_work_path(&thread_id).is_some() {
                        self.tasks.reduce(task::TaskAction::SelectWorkPath {
                            thread_id,
                            path: None,
                        });
                        self.focus = focus;
                        return true;
                    }
                }
                if matches!(target, sidebar::SidebarItemTarget::GoalRun { .. }) {
                    if self.sidebar_visible() {
                        self.focus = FocusArea::Sidebar;
                    }
                    return true;
                }
                self.set_main_pane_conversation(focus);
                true
            }
            MainPaneView::Collaboration
            | MainPaneView::Workspace
            | MainPaneView::WorkContext
            | MainPaneView::FilePreview(_) => {
                if let Some(thread_id) = self.mission_control_return_to_thread_id() {
                    self.set_mission_control_return_to_thread_id(None);
                    if matches!(self.main_pane_view, MainPaneView::FilePreview(_))
                        && self.chat.active_thread_id() == Some(thread_id.as_str())
                    {
                        self.restore_current_conversation_view(focus);
                        return true;
                    }
                    self.restore_conversation_thread(thread_id, focus);
                    return true;
                }
                if let Some(target) = self.mission_control_return_to_goal_target() {
                    self.set_mission_control_return_to_goal_target(None);
                    self.open_sidebar_target(target);
                    self.focus = focus;
                    return true;
                }
                self.set_main_pane_conversation(focus);
                true
            }
            MainPaneView::GoalComposer => {
                if self.sidebar_visible() {
                    self.focus = FocusArea::Sidebar;
                } else {
                    self.focus = focus;
                }
                true
            }
            MainPaneView::Conversation => false,
        }
    }

    pub(crate) fn should_toggle_work_context_from_sidebar(&self, thread_id: &str) -> bool {
        if !matches!(self.main_pane_view, MainPaneView::WorkContext) {
            return false;
        }

        match self.sidebar.active_tab() {
            SidebarTab::Files => self
                .selected_sidebar_file_path()
                .is_some_and(|path: String| {
                    self.tasks.selected_work_path(thread_id) == Some(path.as_str())
                }),
            SidebarTab::Todos => self
                .tasks
                .todos_for_thread(thread_id)
                .get(self.sidebar.selected_item())
                .is_some(),
            SidebarTab::Spawned => false,
            SidebarTab::Pinned => false,
        }
    }

    fn visible_concierge_action_count(&self) -> usize {
        let active_actions = self.chat.active_actions();
        if !active_actions.is_empty() {
            active_actions.len()
        } else {
            self.concierge.welcome_actions.len()
        }
    }

    pub(crate) fn select_visible_concierge_action(&mut self, action_index: usize) {
        let action_count = self.visible_concierge_action_count();
        self.concierge.selected_action = if action_count == 0 {
            0
        } else {
            action_index.min(action_count - 1)
        };
    }

    pub(crate) fn navigate_visible_concierge_action(&mut self, delta: i32) {
        let action_count = self.visible_concierge_action_count();
        if action_count == 0 {
            self.concierge.selected_action = 0;
        } else if delta > 0 {
            self.concierge.selected_action =
                (self.concierge.selected_action + delta as usize).min(action_count - 1);
        } else {
            self.concierge.selected_action = self
                .concierge
                .selected_action
                .saturating_sub((-delta) as usize);
        }
    }

    fn resolve_visible_concierge_action(
        &self,
        action_index: usize,
    ) -> Option<crate::state::ConciergeActionVm> {
        self.chat
            .active_actions()
            .get(action_index)
            .map(|action| crate::state::ConciergeActionVm {
                label: action.label.clone(),
                action_type: action.action_type.clone(),
                thread_id: action.thread_id.clone(),
            })
            .or_else(|| self.concierge.welcome_actions.get(action_index).cloned())
    }

    pub(crate) fn execute_concierge_action(&mut self, action_index: usize) {
        let Some(action) = self.resolve_visible_concierge_action(action_index) else {
            return;
        };
        self.run_concierge_action(action);
    }

    pub(crate) fn selected_inline_message_action_count(&self) -> usize {
        let Some(selected_message) = self.chat.selected_message() else {
            return 0;
        };
        let Some(message) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(selected_message))
        else {
            return 0;
        };
        let is_last_actionable = !message.actions.is_empty()
            && self
                .chat
                .active_actions()
                .first()
                .map(|action| &action.label)
                == message.actions.first().map(|action| &action.label);
        if is_last_actionable {
            0
        } else {
            widgets::chat::message_action_targets(
                &self.chat,
                selected_message,
                message,
                self.tick_counter,
            )
            .len()
        }
    }

    pub(crate) fn execute_concierge_message_action(
        &mut self,
        message_index: usize,
        action_index: usize,
    ) {
        let Some(action) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
            .and_then(|message| message.actions.get(action_index))
            .cloned()
        else {
            return;
        };
        self.run_concierge_action(crate::state::ConciergeActionVm {
            label: action.label,
            action_type: action.action_type,
            thread_id: action.thread_id,
        });
    }

    pub(crate) fn run_concierge_action(&mut self, action: crate::state::ConciergeActionVm) {
        if let Some((question_id, answer)) = action
            .action_type
            .strip_prefix("operator_question_answer:")
            .and_then(|rest| {
                let (question_id, answer) = rest.split_once(':')?;
                Some((question_id.to_string(), answer.to_string()))
            })
        {
            if self.chat.operator_question_answered(&question_id) {
                return;
            }
            self.chat
                .resolve_operator_question_answer(&question_id, answer.clone());
            self.send_daemon_command(DaemonCommand::AnswerOperatorQuestion {
                question_id,
                answer,
            });
            return;
        }

        match action.action_type.as_str() {
            "continue_session" => {
                if let Some(thread_id) = action.thread_id {
                    self.open_thread_conversation(thread_id);
                }
            }
            "start_new" => {
                self.start_new_thread_view();
            }
            "search" => {
                self.ignore_pending_concierge_welcome = true;
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeDismissed);
                self.send_daemon_command(DaemonCommand::DismissConciergeWelcome);
                self.chat
                    .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                self.request_latest_thread_page("concierge".to_string(), false);
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Input;
                self.set_input_text("Search history for: ");
                self.status_line = "Describe what you want to search and press Enter".to_string();
            }
            "dismiss" | "dismiss_welcome" => {
                self.cleanup_concierge_on_navigate();
                if self.chat.active_thread_id() == Some("concierge") {
                    self.chat.reduce(chat::ChatAction::NewThread);
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Input;
                }
            }
            zorai_protocol::tool_names::START_GOAL_RUN => {
                self.cleanup_concierge_on_navigate();
                self.chat
                    .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                self.request_latest_thread_page("concierge".to_string(), false);
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Input;
                self.set_input_text("/new-goal ");
                self.status_line = "Describe your goal and press Enter".to_string();
            }
            "focus_chat" => {
                self.cleanup_concierge_on_navigate();
                self.chat
                    .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                self.request_latest_thread_page("concierge".to_string(), false);
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Input;
            }
            "open_settings" => {
                self.cleanup_concierge_on_navigate();
                self.open_settings_tab(SettingsTab::Auth);
            }
            "operator_profile_skip" => {
                let _ = self.skip_operator_profile_question();
            }
            "operator_profile_defer" => {
                let _ = self.defer_operator_profile_question();
            }
            "operator_profile_retry" => {
                self.retry_operator_profile_request();
                self.status_line = "Retrying operator profile operation…".to_string();
                self.show_input_notice(
                    "Retrying operator profile operation…",
                    InputNoticeKind::Success,
                    40,
                    true,
                );
            }
            _ => {}
        }
    }

    pub(crate) fn open_pending_action_confirm(&mut self, action: PendingConfirmAction) {
        self.pending_chat_action_confirm = Some(action);
        self.chat_action_confirm_accept_selected = true;
        if self.modal.top() != Some(modal::ModalKind::ChatActionConfirm) {
            self.modal.reduce(modal::ModalAction::Push(
                modal::ModalKind::ChatActionConfirm,
            ));
        }
    }

    pub(crate) fn close_chat_action_confirm(&mut self) {
        self.pending_chat_action_confirm = None;
        self.chat_action_confirm_accept_selected = true;
        if self.modal.top() == Some(modal::ModalKind::ChatActionConfirm) {
            self.close_top_modal();
        }
    }

    pub(crate) fn cancel_chat_action_confirm(&mut self) {
        let clears_runtime_confirmation =
            self.pending_chat_action_confirm
                .as_ref()
                .is_some_and(|pending| {
                    matches!(
                        pending,
                        PendingConfirmAction::ReuseModelAsStt { model_id }
                            if model_id.starts_with("__mission_control__:")
                    )
                });
        self.close_chat_action_confirm();
        if clears_runtime_confirmation {
            self.goal_mission_control.clear_runtime_change();
        }
    }

    pub(crate) fn request_regenerate_message(&mut self, index: usize) {
        self.open_pending_action_confirm(PendingConfirmAction::RegenerateMessage {
            message_index: index,
        });
    }

    pub(crate) fn request_delete_message(&mut self, index: usize) {
        self.open_pending_action_confirm(PendingConfirmAction::DeleteMessage {
            message_index: index,
        });
    }

    pub(crate) fn confirm_pending_chat_action(&mut self) {
        let Some(pending) = self.pending_chat_action_confirm.take() else {
            return;
        };
        if self.modal.top() == Some(modal::ModalKind::ChatActionConfirm) {
            self.close_top_modal();
        }
        self.chat_action_confirm_accept_selected = true;
        match pending {
            PendingConfirmAction::RegenerateMessage { message_index } => {
                self.regenerate_from_message(message_index)
            }
            PendingConfirmAction::DeleteMessage { message_index } => {
                self.delete_message(message_index)
            }
            PendingConfirmAction::DeleteThread { thread_id, .. } => {
                let was_streaming = self.chat.is_thread_streaming(&thread_id);
                if was_streaming {
                    if self.chat.active_thread_id() == Some(thread_id.as_str()) {
                        self.cancelled_thread_id = Some(thread_id.clone());
                        self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                        self.clear_active_thread_activity();
                    }
                    self.send_daemon_command(DaemonCommand::StopStream {
                        thread_id: thread_id.clone(),
                    });
                }
                self.send_daemon_command(DaemonCommand::DeleteThread {
                    thread_id: thread_id.clone(),
                });
                self.status_line = "Deleting thread...".to_string();
            }
            PendingConfirmAction::StopThread { thread_id, .. } => {
                if self.chat.active_thread_id() == Some(thread_id.as_str()) {
                    self.cancelled_thread_id = Some(thread_id.clone());
                    self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                    self.clear_active_thread_activity();
                }
                self.send_daemon_command(DaemonCommand::StopStream { thread_id });
                self.status_line = "Stopping thread...".to_string();
            }
            PendingConfirmAction::ResumeThread { thread_id, .. } => {
                self.send_continue_message(thread_id);
                self.status_line = "Resuming thread...".to_string();
            }
            PendingConfirmAction::DeleteGoalRun { goal_run_id, .. } => {
                self.send_daemon_command(DaemonCommand::DeleteGoalRun { goal_run_id });
                self.status_line = "Deleting goal run...".to_string();
            }
            PendingConfirmAction::PauseGoalRun { goal_run_id, .. } => {
                self.send_daemon_command(DaemonCommand::ControlGoalRun {
                    goal_run_id,
                    action: "pause".to_string(),
                    step_index: None,
                    payload_json: None,
                });
                self.status_line = "Pausing goal run...".to_string();
            }
            PendingConfirmAction::StopGoalRun { goal_run_id, .. } => {
                self.send_daemon_command(DaemonCommand::ControlGoalRun {
                    goal_run_id,
                    action: "stop".to_string(),
                    step_index: None,
                    payload_json: None,
                });
                self.status_line = "Stopping goal run...".to_string();
            }
            PendingConfirmAction::ResumeGoalRun { goal_run_id, .. } => {
                self.send_daemon_command(DaemonCommand::ControlGoalRun {
                    goal_run_id,
                    action: "resume".to_string(),
                    step_index: None,
                    payload_json: None,
                });
                self.status_line = "Resuming goal run...".to_string();
            }
            PendingConfirmAction::GoalReview {
                goal_run_id,
                verdict,
                ..
            } => {
                let payload_json = match verdict.as_str() {
                    "soft_reject" => {
                        Some("{\"explanation\":\"Supervisor requested more work.\"}".to_string())
                    }
                    "hard_reject" => {
                        Some("{\"explanation\":\"Supervisor rejected the goal.\"}".to_string())
                    }
                    _ => None,
                };
                self.send_daemon_command(DaemonCommand::ControlGoalRun {
                    goal_run_id,
                    action: verdict,
                    step_index: None,
                    payload_json,
                });
                self.status_line = "Submitting goal review...".to_string();
            }
            PendingConfirmAction::ReuseModelAsStt { model_id } => {
                self.set_audio_config_string("stt", "model", model_id.clone());
                self.status_line = format!("STT model: {}", model_id);
            }
        }
    }
}
