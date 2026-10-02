use super::*;

#[path = "keyboard_enter.rs"]
mod enter;
#[path = "keyboard_actions_global.rs"]
mod keyboard_actions_global;
#[path = "keyboard_actions_goal_task.rs"]
mod keyboard_actions_goal_task;
#[path = "keyboard_actions_input.rs"]
mod keyboard_actions_input;
#[path = "keyboard_actions_navigation.rs"]
mod keyboard_actions_navigation;

#[path = "keyboard_shortcuts.rs"]
mod keyboard_shortcuts;

impl TuiModel {
    pub(super) fn paste_from_clipboard(&mut self) {
        if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
            if !text.is_empty() {
                self.handle_paste(text);
            }
        }
    }

    pub fn handle_key_release(&mut self, code: KeyCode, _modifiers: KeyModifiers) {
        self.update_held_modifier(code, false);
        self.clear_dismissable_input_notice();
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let event_modifiers = modifiers;
        self.update_held_modifier(code, true);
        let modifiers = modifiers | self.held_key_modifiers;

        if matches!(code, KeyCode::Modifier(_)) {
            return false;
        }

        if self.modal.top() == Some(modal::ModalKind::OperatorProfileOnboarding) {
            return self.handle_key_modal(
                code,
                event_modifiers,
                modal::ModalKind::OperatorProfileOnboarding,
            );
        }

        let ctrl = modifiers.contains(KeyModifiers::CONTROL);

        if code == KeyCode::Char('e') && ctrl {
            if self.modal.top() == Some(modal::ModalKind::ErrorViewer) {
                self.last_error = None;
                self.error_active = false;
                self.close_top_modal();
            } else if self.last_error.is_some() {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
                self.error_active = false;
            }
            return false;
        }
        if code == KeyCode::Char('c') && ctrl {
            if self.assistant_busy() {
                self.cancelled_thread_id = self.chat.active_thread_id().map(String::from);
                self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                self.clear_active_thread_activity();
                self.show_input_notice("Stopped stream", InputNoticeKind::Success, 100, false);
            } else if self.focus == FocusArea::Chat {
                if !self.copy_active_drag_selection_to_clipboard() {
                    if matches!(self.main_pane_view, MainPaneView::WorkContext) {
                        self.copy_work_context_content();
                    } else if let Some(sel) = self.chat.selected_message() {
                        self.copy_message(sel);
                    }
                }
            }
            return false;
        }
        if code == KeyCode::Char('q') && ctrl {
            self.open_queued_prompts_modal();
            return false;
        }
        if code == KeyCode::Char('s') && ctrl {
            if self.modal.top() == Some(modal::ModalKind::GoalPicker) {
                return self.handle_key_modal(code, modifiers, modal::ModalKind::GoalPicker);
            }
            if self.focus == FocusArea::Chat
                && matches!(
                    self.main_pane_view,
                    MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                )
                && self.request_selected_goal_run_toggle_confirmation()
            {
                return false;
            }
            if self.voice_player.is_some() {
                self.stop_voice_playback();
                return false;
            }
        }
        if code == KeyCode::Char('a') && ctrl {
            match self.modal.top() {
                Some(modal::ModalKind::ApprovalOverlay) => {}
                Some(modal::ModalKind::OperatorQuestionOverlay) => {}
                Some(modal::ModalKind::ApprovalCenter) => self.close_top_modal(),
                None => self.toggle_approval_center(),
                _ => {}
            }
            return false;
        }
        if let Some(modal_kind) = self.modal.top() {
            if modal_kind == modal::ModalKind::OperatorProfileOnboarding {
                return self.handle_key_modal(code, event_modifiers, modal_kind);
            }
            return self.handle_key_modal(code, modifiers, modal_kind);
        }

        if self.handle_pending_pinned_shortcut_leader(code, modifiers) {
            return false;
        }

        if self.focus == FocusArea::Chat
            && self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::CommandBar
            && matches!(
                self.main_pane_view,
                MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
            )
            && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            let target_mode = match code {
                KeyCode::Char('1') => Some(crate::state::goal_workspace::GoalWorkspaceMode::Work),
                KeyCode::Char('2') => Some(crate::state::goal_workspace::GoalWorkspaceMode::Review),
                KeyCode::Char('3') => {
                    Some(crate::state::goal_workspace::GoalWorkspaceMode::Activity)
                }
                KeyCode::Char('4') => {
                    Some(crate::state::goal_workspace::GoalWorkspaceMode::Threads)
                }
                KeyCode::Char('5') => Some(crate::state::goal_workspace::GoalWorkspaceMode::Files),
                _ => None,
            };
            if let Some(mode) = target_mode {
                self.set_goal_workspace_mode(mode);
                return false;
            }
        }

        if self.focus == FocusArea::Sidebar
            && !self.sidebar_uses_goal_sidebar()
            && self.sidebar.active_tab() == sidebar::SidebarTab::Files
            && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            match code {
                KeyCode::Esc if !self.sidebar.files_filter().is_empty() => {
                    self.sidebar.clear_files_filter();
                    return false;
                }
                KeyCode::Backspace => {
                    if self.sidebar.pop_files_filter() {
                        return false;
                    }
                }
                KeyCode::Char(c) if !c.is_control() && c != '[' && c != ']' => {
                    self.sidebar.push_files_filter(c);
                    return false;
                }
                _ => {}
            }
        }

        if code == KeyCode::Backspace
            && self.focus == FocusArea::Sidebar
            && self.chat.can_go_back_thread()
        {
            self.go_back_thread();
            return false;
        }

        if self.focus == FocusArea::Chat
            && matches!(self.main_pane_view, MainPaneView::Workspace)
            && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            match code {
                KeyCode::Left | KeyCode::Up => {
                    self.step_workspace_board_selection(-1);
                    return false;
                }
                KeyCode::Right | KeyCode::Down | KeyCode::Tab => {
                    self.step_workspace_board_selection(1);
                    return false;
                }
                KeyCode::BackTab => {
                    self.step_workspace_board_selection(-1);
                    return false;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.activate_workspace_board_selection();
                    return false;
                }
                KeyCode::Char('n') => {
                    self.open_workspace_create_modal(zorai_protocol::WorkspaceTaskType::Thread);
                    return false;
                }
                KeyCode::Char('a') => {
                    self.switch_workspace_operator_from_ui(
                        zorai_protocol::WorkspaceOperator::Svarog,
                    );
                    self.status_line = "Switching workspace operator to svarog...".to_string();
                    return false;
                }
                KeyCode::Char('u') => {
                    self.switch_workspace_operator_from_ui(zorai_protocol::WorkspaceOperator::User);
                    self.status_line = "Switching workspace operator to user...".to_string();
                    return false;
                }
                KeyCode::Char('r') => {
                    self.refresh_workspace_board();
                    self.status_line = "Refreshing workspace...".to_string();
                    return false;
                }
                _ => {}
            }
        }

        if matches!(self.main_pane_view, MainPaneView::Conversation)
            && !self.chat.active_actions().is_empty()
            && self.focus == FocusArea::Chat
        {
            match code {
                KeyCode::Left | KeyCode::Up => {
                    self.navigate_visible_concierge_action(-1);
                    return false;
                }
                KeyCode::Right | KeyCode::Down => {
                    self.navigate_visible_concierge_action(1);
                    return false;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.execute_concierge_action(self.concierge.selected_action);
                    return false;
                }
                _ => {}
            }
        }

        let retry_waiting = matches!(self.main_pane_view, MainPaneView::Conversation)
            && self
                .chat
                .retry_status()
                .is_some_and(crate::state::chat::retry_status_shows_prompt_actions);
        let auto_response_waiting = matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.active_auto_response_suggestion().is_some()
            && self.focus == FocusArea::Chat;
        if auto_response_waiting {
            let remaining_secs = self.active_auto_response_countdown_secs().unwrap_or(0);
            match code {
                KeyCode::Left | KeyCode::Char('h') => {
                    self.auto_response_selection = match self.auto_response_selection {
                        AutoResponseActionSelection::Yes => AutoResponseActionSelection::Yes,
                        AutoResponseActionSelection::No => AutoResponseActionSelection::Yes,
                        AutoResponseActionSelection::Always => AutoResponseActionSelection::No,
                    };
                    self.status_line = match self.auto_response_selection {
                        AutoResponseActionSelection::Yes => {
                            format!("Auto response: Yes in {}s", remaining_secs)
                        }
                        AutoResponseActionSelection::No => "Auto response: No".to_string(),
                        AutoResponseActionSelection::Always => {
                            "Auto response: Always for this thread".to_string()
                        }
                    };
                    return false;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.auto_response_selection = match self.auto_response_selection {
                        AutoResponseActionSelection::Yes => AutoResponseActionSelection::No,
                        AutoResponseActionSelection::No => AutoResponseActionSelection::Always,
                        AutoResponseActionSelection::Always => AutoResponseActionSelection::Always,
                    };
                    self.status_line = match self.auto_response_selection {
                        AutoResponseActionSelection::Yes => {
                            format!("Auto response: Yes in {}s", remaining_secs)
                        }
                        AutoResponseActionSelection::No => "Auto response: No".to_string(),
                        AutoResponseActionSelection::Always => {
                            "Auto response: Always for this thread".to_string()
                        }
                    };
                    return false;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    let _ = self.execute_active_auto_response_action(self.auto_response_selection);
                    return false;
                }
                _ => {}
            }
        }
        let retry_wait_accepts_keyboard = retry_waiting && self.focus == FocusArea::Chat;
        if retry_wait_accepts_keyboard {
            match code {
                KeyCode::Left | KeyCode::Char('h') if matches!(self.focus, FocusArea::Chat) => {
                    self.retry_wait_start_selected = true;
                    self.status_line = "Retry prompt: Yes now".to_string();
                    return false;
                }
                KeyCode::Right | KeyCode::Char('l') if matches!(self.focus, FocusArea::Chat) => {
                    self.retry_wait_start_selected = false;
                    self.status_line = "Retry prompt: No".to_string();
                    return false;
                }
                KeyCode::Left => {
                    self.retry_wait_start_selected = true;
                    self.status_line = "Retry prompt: Yes now".to_string();
                    return false;
                }
                KeyCode::Right => {
                    self.retry_wait_start_selected = false;
                    self.status_line = "Retry prompt: No".to_string();
                    return false;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                        if self.retry_wait_start_selected {
                            self.chat.reduce(chat::ChatAction::ClearRetryStatus {
                                thread_id: thread_id.clone(),
                            });
                            self.send_daemon_command(DaemonCommand::RetryStreamNow { thread_id });
                            self.status_line = "Retrying now...".to_string();
                            self.set_active_thread_activity("retrying");
                        } else {
                            self.cancelled_thread_id = Some(thread_id.clone());
                            self.chat.reduce(chat::ChatAction::ClearRetryStatus {
                                thread_id: thread_id.clone(),
                            });
                            self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                            self.clear_active_thread_activity();
                            self.send_daemon_command(DaemonCommand::StopStream { thread_id });
                            self.status_line = "Stopped retry loop".to_string();
                        }
                    }
                    return false;
                }
                _ => {}
            }
        }

        if self.focus == FocusArea::Chat
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.selected_message().is_some()
        {
            let action_count = self.selected_inline_message_action_count();
            if action_count > 0 {
                match code {
                    KeyCode::Left => {
                        self.chat.navigate_selected_message_action(-1, action_count);
                        return false;
                    }
                    KeyCode::Right => {
                        self.chat.navigate_selected_message_action(1, action_count);
                        return false;
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if self.execute_selected_inline_message_action() {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
        }

        if code == KeyCode::Enter
            && !modifiers
                .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL)
            && self.should_show_provider_onboarding()
        {
            let pending_input = self.input.buffer().trim();
            if !pending_input.starts_with('/') {
                self.open_provider_setup();
                return false;
            }
        }

        if code != KeyCode::Esc {
            self.clear_pending_stop();
        }

        if code == KeyCode::Esc
            && self.modal.top().is_none()
            && self.cancel_participant_instruction_edit()
        {
            self.clear_pending_stop();
            return false;
        }

        if code == KeyCode::Esc
            && self.modal.top().is_none()
            && matches!(self.main_pane_view, MainPaneView::GoalComposer)
            && self.cancel_goal_mission_control()
        {
            self.clear_pending_stop();
            return false;
        }

        if let Some(result) = self.handle_global_key_action(code, modifiers, ctrl) {
            self.sync_goal_mission_control_prompt_from_input();
            return result;
        }
        if Self::unmodified_printable_char(code, modifiers).is_some()
            && self.should_prioritize_composer_input()
        {
            if let Some(result) = self.handle_input_key_action(code, modifiers, ctrl) {
                self.sync_goal_mission_control_prompt_from_input();
                return result;
            }
        }
        if let Some(result) = self.handle_navigation_key_action(code, modifiers, ctrl) {
            self.sync_goal_mission_control_prompt_from_input();
            return result;
        }
        if let Some(result) = self.handle_goal_task_key_action(code, modifiers, ctrl) {
            self.sync_goal_mission_control_prompt_from_input();
            return result;
        }
        if let Some(result) = self.handle_input_key_action(code, modifiers, ctrl) {
            self.sync_goal_mission_control_prompt_from_input();
            return result;
        }

        self.sync_goal_mission_control_prompt_from_input();

        false
    }
}
