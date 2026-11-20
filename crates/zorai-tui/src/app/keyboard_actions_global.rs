use super::*;
use crossterm::event::{KeyCode, KeyModifiers};
impl TuiModel {
    pub(super) fn handle_global_key_action(
        &mut self,
        code: KeyCode,
        _modifiers: KeyModifiers,
        ctrl: bool,
    ) -> Option<bool> {
        match code {
            KeyCode::Char('p') if ctrl && self.focus != FocusArea::Chat => {
                {
                    self.open_command_palette(None)
                };
                Some(false)
            }
            KeyCode::Char('t') if ctrl => {
                {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
                    self.sync_thread_picker_item_count();
                };
                Some(false)
            }
            KeyCode::Char('g') if ctrl => {
                {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
                    self.sync_goal_picker_item_count();
                    self.focus = FocusArea::Chat;
                };
                Some(false)
            }
            KeyCode::Char('o')
                if ctrl
                    && matches!(self.main_pane_view, MainPaneView::GoalComposer)
                    && self.mission_control_has_thread_target() =>
            {
                {
                    let _ = self.open_mission_control_goal_thread();
                };
                Some(false)
            }
            KeyCode::Char('n') if ctrl => {
                {
                    self.toggle_notifications_modal();
                };
                Some(false)
            }
            KeyCode::Char('b') if ctrl => {
                {
                    let current = self.show_sidebar_override.unwrap_or(self.width >= 80);
                    self.show_sidebar_override = Some(!current);
                };
                Some(false)
            }
            KeyCode::Char('k') if ctrl && self.pinned_shortcut_scope_active() => {
                {
                    self.arm_pinned_shortcut_leader();
                };
                Some(false)
            }
            KeyCode::Char('d') if ctrl => {
                {
                    if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.step_detail_view_scroll((self.height / 2) as i32);
                    } else {
                        let half_page = (self.height / 2) as i32;
                        self.chat.reduce(chat::ChatAction::ScrollChat(-half_page));
                    }
                };
                Some(false)
            }
            KeyCode::Char('u') if ctrl => {
                {
                    if self.focus == FocusArea::Input {
                        self.input.reduce(input::InputAction::ClearLine);
                    } else if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.step_detail_view_scroll(-((self.height / 2) as i32));
                    } else {
                        let half_page = (self.height / 2) as i32;
                        self.chat.reduce(chat::ChatAction::ScrollChat(half_page));
                    }
                };
                Some(false)
            }
            KeyCode::Char('r') if ctrl => {
                {
                    if self.focus == FocusArea::Chat
                        && matches!(
                            self.main_pane_view,
                            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                        )
                    {
                        if self.request_goal_review_verdict_confirmation("hard_reject") {
                            self.status_line = "Hard reject this goal?".to_string();
                        }
                    } else if self
                        .input_notice
                        .as_ref()
                        .is_some_and(|notice| notice.text.contains("operator profile"))
                    {
                        self.retry_operator_profile_request();
                        self.status_line = "Retrying operator profile operation…".to_string();
                        self.show_input_notice(
                            "Retrying operator profile operation…",
                            InputNoticeKind::Success,
                            40,
                            true,
                        );
                    }
                };
                Some(false)
            }
            KeyCode::PageDown if self.focus == FocusArea::Chat => {
                {
                    if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.step_detail_view_scroll((self.height / 2) as i32);
                    } else {
                        let half_page = (self.height / 2) as i32;
                        self.chat.reduce(chat::ChatAction::ScrollChat(-half_page));
                    }
                };
                Some(false)
            }
            KeyCode::PageUp if self.focus == FocusArea::Chat => {
                {
                    if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.step_detail_view_scroll(-((self.height / 2) as i32));
                    } else {
                        let half_page = (self.height / 2) as i32;
                        self.chat.reduce(chat::ChatAction::ScrollChat(half_page));
                    }
                };
                Some(false)
            }
            KeyCode::Esc => {
                {
                    if self.dismiss_active_main_pane(FocusArea::Chat) {
                        self.clear_pending_stop();
                        return Some(false);
                    }
                    if self.assistant_busy() {
                        if self.pending_stop_active() {
                            self.cancelled_thread_id =
                                self.chat.active_thread_id().map(String::from);
                            self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                            self.clear_active_thread_activity();
                            self.status_line = "Stopped stream".to_string();
                            self.show_input_notice(
                                "Stopped stream",
                                InputNoticeKind::Success,
                                100,
                                false,
                            );
                            self.pending_stop = false;
                        } else {
                            self.pending_stop = true;
                            self.pending_stop_tick = self.tick_counter;
                            self.status_line = "Press Esc again to stop stream".to_string();
                            self.show_input_notice(
                                "Press Esc again to stop stream",
                                InputNoticeKind::Warning,
                                100,
                                true,
                            );
                        }
                    } else {
                        self.clear_pending_stop();
                        if self.focus == FocusArea::Chat {
                            match &self.main_pane_view {
                                MainPaneView::Collaboration
                                | MainPaneView::Workspace
                                | MainPaneView::Task(_)
                                | MainPaneView::WorkContext
                                | MainPaneView::FilePreview(_)
                                | MainPaneView::GoalComposer => {}
                                MainPaneView::Conversation => {
                                    if self.chat.selected_message().is_some() {
                                        self.chat.select_message(None);
                                        let current_scroll = self.chat.scroll_offset() as i32;
                                        if current_scroll > 0 {
                                            self.chat.reduce(chat::ChatAction::ScrollChat(
                                                -current_scroll,
                                            ));
                                        }
                                    }
                                }
                            }
                        } else if self.focus == FocusArea::Input {
                            self.focus = FocusArea::Chat;
                        }
                    }
                };
                Some(false)
            }
            KeyCode::Tab => {
                {
                    if self.focus == FocusArea::Input {
                        if let Some(suggestion) = self.next_reply_offer().map(str::to_string) {
                            self.input.set_text(&suggestion);
                            return Some(false);
                        }
                        let completion = self.input.complete_active_at_token_with_agents(
                            &self.known_agent_directive_aliases(),
                        );
                        if let Some(notice) = completion.notice {
                            self.status_line = notice.clone();
                            self.show_input_notice(notice, InputNoticeKind::Warning, 40, true);
                        }
                        if completion.consumed {
                            return Some(false);
                        }
                    }
                    self.focus_next();
                };
                Some(false)
            }
            KeyCode::BackTab => {
                self.focus_prev();
                Some(false)
            }
            KeyCode::Left if ctrl && self.focus == FocusArea::Input => {
                {
                    self.input.reduce(input::InputAction::MoveCursorWordLeft);
                };
                Some(false)
            }
            KeyCode::Right if ctrl && self.focus == FocusArea::Input => {
                {
                    self.input.reduce(input::InputAction::MoveCursorWordRight);
                };
                Some(false)
            }
            KeyCode::Left if self.focus == FocusArea::Input => {
                {
                    self.input.reduce(input::InputAction::MoveCursorLeft);
                };
                Some(false)
            }
            KeyCode::Right if self.focus == FocusArea::Input => {
                {
                    self.input.reduce(input::InputAction::MoveCursorRight);
                };
                Some(false)
            }
            KeyCode::Up if self.focus == FocusArea::Input => {
                {
                    if self.handle_goal_composer_input_vertical(-1) {
                        return Some(false);
                    }
                    if self.input.can_browse_sent_history() {
                        self.input.reduce(input::InputAction::HistoryPrevious);
                    } else {
                        let wrap_w = self.input_wrap_width();
                        self.input
                            .reduce(input::InputAction::MoveCursorUpVisual(wrap_w));
                    }
                };
                Some(false)
            }
            KeyCode::Down if self.focus == FocusArea::Input => {
                {
                    if self.handle_goal_composer_input_vertical(1) {
                        return Some(false);
                    }
                    if self.input.can_browse_sent_history() {
                        self.input.reduce(input::InputAction::HistoryNext);
                    } else {
                        let wrap_w = self.input_wrap_width();
                        self.input
                            .reduce(input::InputAction::MoveCursorDownVisual(wrap_w));
                    }
                };
                Some(false)
            }
            KeyCode::Home if self.focus == FocusArea::Input => {
                {
                    self.input.reduce(input::InputAction::MoveCursorHome);
                };
                Some(false)
            }
            KeyCode::End if self.focus == FocusArea::Input => {
                {
                    self.input.reduce(input::InputAction::MoveCursorEnd);
                };
                Some(false)
            }
            KeyCode::Char('z') if ctrl && self.focus == FocusArea::Input => {
                {
                    self.input.reduce(input::InputAction::Undo);
                };
                Some(false)
            }
            KeyCode::Char('y') if ctrl && self.focus == FocusArea::Input => {
                {
                    self.input.reduce(input::InputAction::Redo);
                };
                Some(false)
            }
            KeyCode::Home if self.focus == FocusArea::Chat => {
                {
                    if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.scroll_detail_view_to_top();
                    } else {
                        self.chat.reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));
                        self.chat.select_message(Some(0));
                    }
                };
                Some(false)
            }
            KeyCode::End if self.focus == FocusArea::Chat => {
                {
                    if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.scroll_detail_view_to_bottom();
                    } else {
                        let offset = self.chat.scroll_offset() as i32;
                        self.chat.reduce(chat::ChatAction::ScrollChat(-offset));
                        self.chat.select_message(None);
                    }
                };
                Some(false)
            }
            _ => None,
        }
    }
}
