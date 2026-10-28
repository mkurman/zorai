use super::*;
impl TuiModel {
    pub(crate) fn send_continue_message(&mut self, thread_id: String) {
        self.send_daemon_command(DaemonCommand::SendMessage {
            thread_id: Some(thread_id),
            content: "continue".to_string(),
            content_blocks_json: None,
            session_id: self.default_session_id.clone(),
            target_agent_id: None,
        });
    }

    pub(crate) fn capture_pending_reconnect_restore(&mut self) {
        if self.pending_reconnect_restore.is_some() {
            return;
        }

        let Some(thread) = self.chat.active_thread() else {
            return;
        };
        if thread.id == "concierge"
            || Self::is_internal_agent_thread(&thread.id, Some(thread.title.as_str()))
            || Self::is_hidden_agent_thread(&thread.id, Some(thread.title.as_str()))
        {
            return;
        }

        self.pending_reconnect_restore = Some(PendingReconnectRestore {
            thread_id: thread.id.clone(),
            should_resume: self.assistant_busy(),
        });
    }

    pub(crate) fn begin_pending_reconnect_restore(&mut self) -> bool {
        let Some(pending) = self.pending_reconnect_restore.clone() else {
            return false;
        };

        self.clear_mission_control_return_context();
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.clear_task_view_drag_selection();
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeDismissed);
        self.chat.reduce(chat::ChatAction::DismissConciergeWelcome);
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
        self.set_main_pane_conversation(FocusArea::Chat);
        self.chat
            .reduce(chat::ChatAction::SelectThread(pending.thread_id.clone()));
        self.request_authoritative_thread_refresh(pending.thread_id.clone(), true);
        self.status_line = "Restoring thread after reconnect...".to_string();
        true
    }

    pub(crate) fn fallback_pending_reconnect_restore(&mut self) -> bool {
        let Some(pending) = self.pending_reconnect_restore.as_ref() else {
            return false;
        };
        let thread_exists = self
            .chat
            .threads()
            .iter()
            .any(|thread| thread.id == pending.thread_id);
        if thread_exists {
            return false;
        }

        self.pending_reconnect_restore = None;
        if self.connected && self.agent_config_loaded {
            self.request_concierge_welcome();
        }
        true
    }

    pub(crate) fn finish_pending_reconnect_restore(&mut self, thread_id: &str) {
        let Some(pending) = self.pending_reconnect_restore.clone() else {
            return;
        };
        if pending.thread_id != thread_id {
            return;
        }

        self.pending_reconnect_restore = None;
        if pending.should_resume {
            self.send_continue_message(thread_id.to_string());
            self.status_line = "Resuming thread after reconnect...".to_string();
        }
    }

    fn goal_sidebar_items_for_tab(
        &self,
        goal_run_id: &str,
        tab: GoalSidebarTab,
    ) -> Vec<GoalSidebarSelectionAnchor> {
        let Some(run) = self.tasks.goal_run_by_id(goal_run_id) else {
            return Vec::new();
        };

        match tab {
            GoalSidebarTab::Steps => {
                let mut steps: Vec<_> = run.steps.iter().collect();
                steps.sort_by_key(|step| step.order);
                steps
                    .into_iter()
                    .map(|step| GoalSidebarSelectionAnchor::Step(step.id.clone()))
                    .collect()
            }
            GoalSidebarTab::Checkpoints => self
                .tasks
                .checkpoints_for_goal_run(goal_run_id)
                .iter()
                .map(|checkpoint| GoalSidebarSelectionAnchor::Checkpoint(checkpoint.id.clone()))
                .collect(),
            GoalSidebarTab::Tasks => {
                let tasks: Vec<_> = if !run.child_task_ids.is_empty() {
                    run.child_task_ids
                        .iter()
                        .filter_map(|task_id| self.tasks.task_by_id(task_id))
                        .collect()
                } else {
                    self.tasks
                        .tasks()
                        .iter()
                        .filter(|task| task.goal_run_id.as_deref() == Some(goal_run_id))
                        .collect()
                };
                tasks
                    .into_iter()
                    .map(|task| GoalSidebarSelectionAnchor::Task(task.id.clone()))
                    .collect()
            }
            GoalSidebarTab::Files => {
                let Some(thread_id) = run.thread_id.as_deref() else {
                    return Vec::new();
                };
                let Some(context) = self.tasks.work_context_for_thread(thread_id) else {
                    return Vec::new();
                };
                context
                    .entries
                    .iter()
                    .filter(|entry| match entry.goal_run_id.as_deref() {
                        Some(entry_goal_run_id) => entry_goal_run_id == goal_run_id,
                        None => true,
                    })
                    .map(|entry| GoalSidebarSelectionAnchor::File {
                        thread_id: thread_id.to_string(),
                        path: entry.path.clone(),
                    })
                    .collect()
            }
        }
    }

    fn current_goal_sidebar_selection_anchor(&self) -> Option<GoalSidebarSelectionAnchor> {
        let goal_run_id = match &self.main_pane_view {
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) => {
                goal_run_id.as_str()
            }
            _ => return None,
        };
        let items = self.goal_sidebar_items_for_tab(goal_run_id, self.goal_sidebar.active_tab());
        items.get(self.goal_sidebar.selected_row()).cloned()
    }

    pub(crate) fn sync_goal_sidebar_selection_anchor(&mut self) {
        self.goal_sidebar_selection_anchor = self.current_goal_sidebar_selection_anchor();
    }

    pub(crate) fn chat_history_page_size(&self) -> usize {
        self.config.tui_chat_history_page_size.max(20) as usize
    }

    pub(crate) fn chat_history_delete_backfill_target_size(&self) -> usize {
        self.chat_history_page_size().saturating_mul(11) / 10
    }

    pub(crate) fn request_thread_page(
        &mut self,
        thread_id: String,
        message_limit: usize,
        message_offset: usize,
        show_loading: bool,
    ) {
        let local_deleted_count = self.chat.local_deleted_message_count_for_thread(&thread_id);
        let adjusted_message_offset = if message_offset > 0 {
            message_offset.saturating_add(local_deleted_count)
        } else {
            message_offset
        };
        tracing::info!(
            thread_id = %thread_id,
            message_limit,
            message_offset = adjusted_message_offset,
            local_message_offset = message_offset,
            show_loading,
            local_deleted_count,
            history_config_messages = self.chat_history_page_size(),
            history_target_messages = self.chat_history_delete_backfill_target_size(),
            active_thread_id = ?self.chat.active_thread_id(),
            "tui requesting thread messages"
        );
        if show_loading {
            self.pending_local_message_delete_backfills.clear();
            self.pending_local_message_delete_fetches.clear();
            self.begin_thread_loading(thread_id.clone());
        }
        self.send_daemon_command(DaemonCommand::RequestThread {
            thread_id,
            message_limit: Some(message_limit),
            message_offset: Some(adjusted_message_offset),
        });
    }

    pub(crate) fn request_latest_thread_page(&mut self, thread_id: String, show_loading: bool) {
        self.request_thread_page(
            thread_id.clone(),
            self.chat_history_delete_backfill_target_size(),
            0,
            show_loading,
        );
        self.send_daemon_command(DaemonCommand::ListPromptQueue {
            thread_id: Some(thread_id),
        });
    }

    fn thread_needs_expanded_latest_page(&self, thread_id: &str) -> bool {
        self.chat.threads().iter().any(|thread| {
            thread.id == thread_id
                && (!thread.thread_participants.is_empty()
                    || !thread.queued_participant_suggestions.is_empty())
        })
    }

    fn authoritative_thread_refresh_page(&self, thread_id: &str) -> (usize, usize) {
        let base_limit = self.chat_history_delete_backfill_target_size();
        let fallback_limit = if self.thread_needs_expanded_latest_page(thread_id) {
            base_limit.saturating_mul(2)
        } else {
            base_limit
        };
        let Some(thread) = self
            .chat
            .threads()
            .iter()
            .find(|thread| thread.id == thread_id)
        else {
            return (fallback_limit, 0);
        };

        let window = chat::chat_window::MessageWindow::from_thread(thread);
        let loaded_len = window.end.saturating_sub(window.start);
        if loaded_len == 0 {
            return (fallback_limit, 0);
        }

        let message_limit = loaded_len.max(fallback_limit);
        let message_offset = window.total.saturating_sub(window.end);
        (message_limit, message_offset)
    }

    pub(crate) fn request_authoritative_thread_refresh(
        &mut self,
        thread_id: String,
        show_loading: bool,
    ) {
        let (message_limit, message_offset) = self.authoritative_thread_refresh_page(&thread_id);
        self.request_thread_page(thread_id, message_limit, message_offset, show_loading);
    }

    pub(crate) fn request_authoritative_goal_run_refresh(&mut self, goal_run_id: String) {
        self.send_daemon_command(DaemonCommand::RequestGoalRunDetail(goal_run_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestGoalRunCheckpoints(goal_run_id));
    }

    pub(crate) fn request_full_goal_view_refresh(&mut self, goal_run_id: String) {
        self.request_authoritative_goal_run_refresh(goal_run_id);
        self.send_daemon_command(DaemonCommand::Refresh);
        self.send_daemon_command(DaemonCommand::RefreshServices);
    }

    pub(in crate::app) fn schedule_goal_hydration_refresh(&mut self, goal_run_id: String) {
        if self
            .pending_goal_hydration_refreshes
            .insert(goal_run_id.clone())
        {
            self.send_daemon_command(DaemonCommand::ScheduleGoalHydrationRefresh(goal_run_id));
        }
    }

    pub(in crate::app) fn clear_goal_hydration_refresh(&mut self, goal_run_id: &str) {
        self.pending_goal_hydration_refreshes.remove(goal_run_id);
    }

    pub(crate) fn goal_sidebar_item_count_for_tab(
        &self,
        goal_run_id: &str,
        tab: GoalSidebarTab,
    ) -> usize {
        self.goal_sidebar_items_for_tab(goal_run_id, tab).len()
    }

    pub(crate) fn reconcile_goal_sidebar_selection_for_active_goal_pane(&mut self) {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun {
            goal_run_id,
            step_id,
        }) = &self.main_pane_view
        else {
            return;
        };

        let tab = self.goal_sidebar.active_tab();
        let items = self.goal_sidebar_items_for_tab(goal_run_id, tab);
        let anchored_item = match (tab, step_id.as_deref()) {
            (GoalSidebarTab::Steps, Some(step_id)) => {
                Some(GoalSidebarSelectionAnchor::Step(step_id.to_string()))
            }
            _ => self.goal_sidebar_selection_anchor.clone(),
        };
        let matched_anchor_row = anchored_item
            .as_ref()
            .and_then(|anchor| items.iter().position(|item| item == anchor));
        let target_row = matched_anchor_row.unwrap_or_else(|| self.goal_sidebar.selected_row());

        self.goal_sidebar.select_row(target_row, items.len());
        self.goal_sidebar_selection_anchor = if matched_anchor_row.is_some() {
            items.get(self.goal_sidebar.selected_row()).cloned()
        } else {
            anchored_item.or_else(|| items.get(self.goal_sidebar.selected_row()).cloned())
        };
    }

    pub(crate) fn maybe_request_older_chat_history(&mut self) {
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };
        if self
            .pending_local_message_delete_backfills
            .get(&thread_id)
            .copied()
            .unwrap_or(0)
            > 0
            || self
                .pending_local_message_delete_fetches
                .contains_key(&thread_id)
        {
            if self.chat.active_thread_older_page_pending() {
                self.chat.mark_active_thread_older_page_pending(
                    false,
                    self.tick_counter,
                    chat::CHAT_HISTORY_FETCH_DEBOUNCE_TICKS,
                );
            }
            return;
        }
        let Some(message_offset) = self.chat.active_thread_next_page_offset(self.tick_counter)
        else {
            return;
        };

        let chat_area = self.pane_layout().chat;
        let near_loaded_top = self
            .chat_scrollbar_layout()
            .map(|layout| layout.max_scroll.saturating_sub(layout.scroll) <= 3)
            .unwrap_or_else(|| {
                self.chat.scroll_offset() > 0 || (chat_area.width > 0 && chat_area.height > 0)
            });
        if !near_loaded_top {
            return;
        };

        self.chat.mark_active_thread_older_page_pending(
            true,
            self.tick_counter,
            chat::CHAT_HISTORY_FETCH_DEBOUNCE_TICKS,
        );
        self.request_thread_page(
            thread_id,
            self.chat_history_delete_backfill_target_size(),
            message_offset,
            false,
        );
    }

    pub(crate) fn maybe_request_older_goal_run_history(&mut self) {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return;
        };
        if self.task_view_scroll > 3 {
            return;
        }

        let Some((step_offset, step_limit, event_offset, event_limit)) = self
            .tasks
            .goal_run_next_page_request(goal_run_id, self.tick_counter)
        else {
            return;
        };

        self.tasks.mark_goal_run_older_page_pending(
            goal_run_id,
            true,
            self.tick_counter,
            task::GOAL_RUN_HISTORY_FETCH_DEBOUNCE_TICKS,
        );
        self.send_daemon_command(DaemonCommand::RequestGoalRunDetailPage {
            goal_run_id: goal_run_id.clone(),
            step_offset,
            step_limit,
            event_offset,
            event_limit,
        });
    }

    pub(crate) fn maybe_schedule_chat_history_collapse(&mut self) {
        if !self.chat.is_following_bottom() {
            return;
        }
        if self.chat.active_thread().is_some_and(|thread| {
            thread.history_window_expanded && thread.collapse_deadline_tick.is_none()
        }) && widgets::chat::latest_messages_overflow_viewport(
            self.pane_layout().chat,
            &self.chat,
            &self.theme,
            self.tick_counter,
            self.retry_wait_start_selected,
            self.chat.history_page_size(),
        ) {
            self.chat.schedule_history_collapse(
                self.tick_counter,
                chat::CHAT_HISTORY_COLLAPSE_DELAY_TICKS,
            );
        }
    }

    pub(crate) fn thread_picker_target_agent_id(tab: modal::ThreadPickerTab) -> Option<String> {
        tab.agent_id().map(str::to_string)
    }

    pub(crate) fn cleanup_concierge_on_navigate(&mut self) {
        if !self.concierge.auto_cleanup_on_navigate {
            return;
        }

        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.clear_task_view_drag_selection();
        self.ignore_pending_concierge_welcome = true;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeDismissed);
        self.chat.reduce(chat::ChatAction::DismissConciergeWelcome);
        self.send_daemon_command(DaemonCommand::DismissConciergeWelcome);

        if self.chat.active_thread_id() == Some("concierge") && self.assistant_busy() {
            let thread_id = "concierge".to_string();
            self.cancelled_thread_id = Some(thread_id.clone());
            self.chat.reduce(chat::ChatAction::ForceStopStreaming);
            self.clear_agent_activity_for(Some(thread_id.as_str()));
            self.send_daemon_command(DaemonCommand::StopStream { thread_id });
        }

        self.clear_pending_stop();
    }

    pub(crate) fn clear_pending_new_thread_target(&mut self) {
        self.pending_new_thread_target_agent = None;
        self.pending_new_thread_execution_profile = None;
    }

    pub(crate) fn open_thread_conversation(&mut self, thread_id: String) {
        self.set_mission_control_return_targets(self.current_goal_return_target(), None);
        self.cleanup_concierge_on_navigate();
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.clear_task_view_drag_selection();
        self.clear_pending_new_thread_target();
        self.chat
            .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
        self.request_latest_thread_page(thread_id, true);
        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
        self.sync_contextual_approval_overlay();
    }

    pub(crate) fn restore_conversation_thread(&mut self, thread_id: String, focus: FocusArea) {
        self.cleanup_concierge_on_navigate();
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.clear_task_view_drag_selection();
        self.clear_pending_new_thread_target();
        self.chat
            .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
        self.request_latest_thread_page(thread_id, true);
        self.main_pane_view = MainPaneView::Conversation;
        self.task_view_scroll = 0;
        self.focus = focus;
        self.sync_contextual_approval_overlay();
    }

    pub(crate) fn restore_current_conversation_view(&mut self, focus: FocusArea) {
        self.cleanup_concierge_on_navigate();
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.clear_task_view_drag_selection();
        self.clear_pending_new_thread_target();
        self.main_pane_view = MainPaneView::Conversation;
        self.task_view_scroll = 0;
        self.focus = focus;
        self.sync_contextual_approval_overlay();
    }

    pub(crate) fn start_new_thread_view(&mut self) {
        self.start_new_thread_view_for_agent(None);
    }

    pub(crate) fn start_new_thread_view_for_agent(&mut self, target_agent_id: Option<&str>) {
        self.clear_mission_control_return_context();
        self.cleanup_concierge_on_navigate();
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.clear_task_view_drag_selection();
        self.pending_new_thread_execution_profile = None;
        self.pending_new_thread_target_agent = target_agent_id.map(str::to_string);
        self.thread_loading_id = None;
        self.chat.reduce(chat::ChatAction::NewThread);
        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Input;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
    }

    fn begin_concierge_welcome_request(&mut self) {
        self.clear_mission_control_return_context();
        self.pending_reconnect_restore = None;
        self.ignore_pending_concierge_welcome = false;
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.clear_task_view_drag_selection();
        self.thread_loading_id = None;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeDismissed);
        self.chat.reduce(chat::ChatAction::DismissConciergeWelcome);
        self.chat
            .reduce(chat::ChatAction::SelectThread(String::new()));
        self.set_main_pane_conversation(FocusArea::Chat);
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(true));
    }

    pub(in crate::app) fn request_concierge_welcome(&mut self) {
        self.begin_concierge_welcome_request();
        self.send_daemon_command(DaemonCommand::RequestConciergeWelcome);
    }

    pub(in crate::app) fn retry_operator_profile_request(&mut self) {
        self.begin_concierge_welcome_request();
        self.send_daemon_command(DaemonCommand::RetryOperatorProfile);
    }

    pub(crate) fn set_main_pane_conversation(&mut self, focus: FocusArea) {
        self.main_pane_view = MainPaneView::Conversation;
        self.task_view_scroll = 0;
        self.focus = focus;
    }
}
