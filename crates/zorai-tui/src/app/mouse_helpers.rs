use super::*;

impl TuiModel {
    pub(in super::super) fn clear_chat_drag_selection(&mut self) {
        self.chat_drag_anchor = None;
        self.chat_drag_current = None;
        self.chat_drag_anchor_point = None;
        self.chat_drag_current_point = None;
        self.chat_selection_snapshot = None;
        self.chat_scrollbar_drag_grab_offset = None;
    }

    pub(in super::super) fn set_chat_scroll_offset(&mut self, target: usize) {
        let current = self.chat.scroll_offset();
        if target == current {
            return;
        }

        let delta = if target > current {
            (target - current).min(i32::MAX as usize) as i32
        } else {
            -((current - target).min(i32::MAX as usize) as i32)
        };
        self.chat.reduce(chat::ChatAction::ScrollChat(delta));
    }

    pub(crate) fn cached_chat_hit_test(
        &mut self,
        chat_area: Rect,
        mouse: Position,
    ) -> Option<chat::ChatHitTarget> {
        if self
            .chat_selection_snapshot
            .as_ref()
            .is_none_or(|snapshot| {
                !widgets::chat::cached_snapshot_matches_area(snapshot, chat_area)
            })
        {
            self.chat_selection_snapshot = widgets::chat::build_selection_snapshot(
                chat_area,
                &self.chat,
                &self.theme,
                self.tick_counter,
                self.retry_wait_start_selected,
            );
        }

        self.chat_selection_snapshot.as_ref().and_then(|snapshot| {
            widgets::chat::hit_test_from_cached_snapshot(
                snapshot,
                &self.chat,
                self.tick_counter,
                mouse,
            )
        })
    }

    pub(in super::super) fn clear_work_context_drag_selection(&mut self) {
        self.work_context_drag_anchor = None;
        self.work_context_drag_current = None;
        self.work_context_drag_anchor_point = None;
        self.work_context_drag_current_point = None;
    }

    pub(in super::super) fn clear_task_view_drag_selection(&mut self) {
        self.task_view_drag_anchor = None;
        self.task_view_drag_current = None;
        self.task_view_drag_anchor_point = None;
        self.task_view_drag_current_point = None;
        self.file_preview_scrollbar_drag_grab_offset = None;
    }

    pub(in super::super) fn copy_active_drag_selection_to_clipboard(&mut self) -> bool {
        if self.copy_active_task_drag_selection_to_clipboard() {
            return true;
        }
        if self.copy_active_work_context_drag_selection_to_clipboard() {
            return true;
        }
        false
    }

    fn copy_active_task_drag_selection_to_clipboard(&mut self) -> bool {
        let Some((anchor_point, current_point)) = self
            .task_view_drag_anchor_point
            .zip(self.task_view_drag_current_point)
        else {
            return false;
        };
        if anchor_point == current_point {
            return false;
        }

        let layout = self.pane_layout();
        let text = match &self.main_pane_view {
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) => {
                let task_area = self.task_content_area().unwrap_or(layout.chat);
                widgets::goal_workspace::selected_text(
                    task_area,
                    &self.tasks,
                    goal_run_id,
                    &self.goal_workspace,
                    anchor_point,
                    current_point,
                )
            }
            MainPaneView::Task(target) => {
                let task_area = self.task_content_area().unwrap_or(layout.chat);
                widgets::task_view::selected_text(
                    task_area,
                    &self.tasks,
                    target,
                    &self.theme,
                    self.task_view_scroll,
                    self.task_show_live_todos,
                    self.task_show_timeline,
                    self.task_show_files,
                    anchor_point,
                    current_point,
                )
            }
            MainPaneView::FilePreview(target) => widgets::file_preview::selected_text(
                layout.chat,
                &self.tasks,
                target,
                &self.theme,
                self.task_view_scroll,
                anchor_point,
                current_point,
            ),
            _ => None,
        };

        if let Some(text) = text {
            conversion::copy_to_clipboard(&text);
            self.status_line = "Copied selection to clipboard".to_string();
            true
        } else {
            false
        }
    }

    fn copy_active_work_context_drag_selection_to_clipboard(&mut self) -> bool {
        let Some((anchor_point, current_point)) = self
            .work_context_drag_anchor_point
            .zip(self.work_context_drag_current_point)
        else {
            return false;
        };
        if anchor_point == current_point
            || !matches!(self.main_pane_view, MainPaneView::WorkContext)
        {
            return false;
        }

        let layout = self.pane_layout();
        let text = widgets::work_context_view::selected_text(
            layout.chat,
            &self.tasks,
            self.chat.active_thread_id(),
            self.sidebar.active_tab(),
            self.sidebar.selected_item(),
            &self.theme,
            self.task_view_scroll,
            anchor_point,
            current_point,
        );
        if let Some(text) = text {
            conversion::copy_to_clipboard(&text);
            self.status_line = "Copied selection to clipboard".to_string();
            true
        } else {
            false
        }
    }

    pub(in super::super) fn clear_workspace_drag(&mut self) {
        self.workspace_drag_task = None;
        self.workspace_drag_status = None;
        self.workspace_drag_start_target = None;
    }

    pub(in crate::app) fn current_detail_view_max_scroll(&self) -> usize {
        let area = self.pane_layout().chat;
        match &self.main_pane_view {
            MainPaneView::Task(target) => match target {
                sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. } => {
                    match self.goal_workspace.focused_pane() {
                        crate::state::goal_workspace::GoalWorkspacePane::Plan
                        | crate::state::goal_workspace::GoalWorkspacePane::CommandBar => {
                            widgets::goal_workspace::max_plan_scroll(
                                area,
                                &self.tasks,
                                goal_run_id,
                                &self.goal_workspace,
                            )
                        }
                        crate::state::goal_workspace::GoalWorkspacePane::Timeline => {
                            widgets::goal_workspace::max_timeline_scroll(
                                area,
                                &self.tasks,
                                goal_run_id,
                                &self.goal_workspace,
                            )
                        }
                        crate::state::goal_workspace::GoalWorkspacePane::Details => {
                            widgets::goal_workspace::max_detail_scroll(
                                area,
                                &self.tasks,
                                goal_run_id,
                                &self.goal_workspace,
                            )
                        }
                    }
                }
                _ => widgets::task_view::max_scroll(
                    area,
                    &self.tasks,
                    target,
                    &self.theme,
                    self.task_show_live_todos,
                    self.task_show_timeline,
                    self.task_show_files,
                ),
            },
            MainPaneView::WorkContext => widgets::work_context_view::max_scroll(
                area,
                &self.tasks,
                self.chat.active_thread_id(),
                self.sidebar.active_tab(),
                self.sidebar.selected_item(),
                &self.theme,
            ),
            MainPaneView::FilePreview(target) => {
                widgets::file_preview::max_scroll(area, &self.tasks, target, &self.theme)
            }
            _ => 0,
        }
    }

    pub(in super::super) fn clamp_detail_view_scroll(&mut self) {
        let max_scroll = self.current_detail_view_max_scroll();
        if matches!(
            self.main_pane_view,
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
        ) {
            match self.goal_workspace.focused_pane() {
                crate::state::goal_workspace::GoalWorkspacePane::Plan
                | crate::state::goal_workspace::GoalWorkspacePane::CommandBar => self
                    .goal_workspace
                    .set_plan_scroll(self.goal_workspace.plan_scroll().min(max_scroll)),
                crate::state::goal_workspace::GoalWorkspacePane::Timeline => self
                    .goal_workspace
                    .set_timeline_scroll(self.goal_workspace.timeline_scroll().min(max_scroll)),
                crate::state::goal_workspace::GoalWorkspacePane::Details => self
                    .goal_workspace
                    .set_detail_scroll(self.goal_workspace.detail_scroll().min(max_scroll)),
            }
        } else {
            self.task_view_scroll = self.task_view_scroll.min(max_scroll);
        }
    }

    pub(in super::super) fn step_detail_view_scroll(&mut self, delta: i32) {
        let max_scroll = self.current_detail_view_max_scroll();
        if matches!(
            self.main_pane_view,
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
        ) {
            match self.goal_workspace.focused_pane() {
                crate::state::goal_workspace::GoalWorkspacePane::Plan
                | crate::state::goal_workspace::GoalWorkspacePane::CommandBar => {
                    let next = if delta >= 0 {
                        self.goal_workspace
                            .plan_scroll()
                            .saturating_add(delta as usize)
                            .min(max_scroll)
                    } else {
                        self.goal_workspace
                            .plan_scroll()
                            .saturating_sub((-delta) as usize)
                    };
                    self.goal_workspace.set_plan_scroll(next);
                }
                crate::state::goal_workspace::GoalWorkspacePane::Timeline => {
                    let next = if delta >= 0 {
                        self.goal_workspace
                            .timeline_scroll()
                            .saturating_add(delta as usize)
                            .min(max_scroll)
                    } else {
                        self.goal_workspace
                            .timeline_scroll()
                            .saturating_sub((-delta) as usize)
                    };
                    self.goal_workspace.set_timeline_scroll(next);
                }
                crate::state::goal_workspace::GoalWorkspacePane::Details => {
                    let next = if delta >= 0 {
                        self.goal_workspace
                            .detail_scroll()
                            .saturating_add(delta as usize)
                            .min(max_scroll)
                    } else {
                        self.goal_workspace
                            .detail_scroll()
                            .saturating_sub((-delta) as usize)
                    };
                    self.goal_workspace.set_detail_scroll(next);
                }
            }
        } else {
            if delta >= 0 {
                self.task_view_scroll = self
                    .task_view_scroll
                    .saturating_add(delta as usize)
                    .min(max_scroll);
            } else {
                self.task_view_scroll = self.task_view_scroll.saturating_sub((-delta) as usize);
            }
        }
    }

    pub(in super::super) fn scroll_detail_view_to_top(&mut self) {
        if matches!(
            self.main_pane_view,
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
        ) {
            match self.goal_workspace.focused_pane() {
                crate::state::goal_workspace::GoalWorkspacePane::Plan
                | crate::state::goal_workspace::GoalWorkspacePane::CommandBar => {
                    self.goal_workspace.set_plan_scroll(0)
                }
                crate::state::goal_workspace::GoalWorkspacePane::Timeline => {
                    self.goal_workspace.set_timeline_scroll(0)
                }
                crate::state::goal_workspace::GoalWorkspacePane::Details => {
                    self.goal_workspace.set_detail_scroll(0)
                }
            }
        } else {
            self.task_view_scroll = 0;
        }
    }

    pub(in super::super) fn scroll_detail_view_to_bottom(&mut self) {
        let max_scroll = self.current_detail_view_max_scroll();
        if matches!(
            self.main_pane_view,
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
        ) {
            match self.goal_workspace.focused_pane() {
                crate::state::goal_workspace::GoalWorkspacePane::Plan
                | crate::state::goal_workspace::GoalWorkspacePane::CommandBar => {
                    self.goal_workspace.set_plan_scroll(max_scroll)
                }
                crate::state::goal_workspace::GoalWorkspacePane::Timeline => {
                    self.goal_workspace.set_timeline_scroll(max_scroll)
                }
                crate::state::goal_workspace::GoalWorkspacePane::Details => {
                    self.goal_workspace.set_detail_scroll(max_scroll)
                }
            }
        } else {
            self.task_view_scroll = max_scroll;
        }
    }

    pub(in super::super) fn step_goal_workspace_pane_scroll(
        &mut self,
        pane: crate::state::goal_workspace::GoalWorkspacePane,
        delta: i32,
    ) {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return;
        };
        let area = self.pane_layout().chat;
        match pane {
            crate::state::goal_workspace::GoalWorkspacePane::Plan
            | crate::state::goal_workspace::GoalWorkspacePane::CommandBar => {
                let max_scroll = widgets::goal_workspace::max_plan_scroll(
                    area,
                    &self.tasks,
                    goal_run_id,
                    &self.goal_workspace,
                );
                let next = if delta >= 0 {
                    self.goal_workspace
                        .plan_scroll()
                        .saturating_add(delta as usize)
                        .min(max_scroll)
                } else {
                    self.goal_workspace
                        .plan_scroll()
                        .saturating_sub((-delta) as usize)
                };
                self.goal_workspace.set_plan_scroll(next);
            }
            crate::state::goal_workspace::GoalWorkspacePane::Timeline => {
                let max_scroll = widgets::goal_workspace::max_timeline_scroll(
                    area,
                    &self.tasks,
                    goal_run_id,
                    &self.goal_workspace,
                );
                let next = if delta >= 0 {
                    self.goal_workspace
                        .timeline_scroll()
                        .saturating_add(delta as usize)
                        .min(max_scroll)
                } else {
                    self.goal_workspace
                        .timeline_scroll()
                        .saturating_sub((-delta) as usize)
                };
                self.goal_workspace.set_timeline_scroll(next);
            }
            crate::state::goal_workspace::GoalWorkspacePane::Details => {
                let max_scroll = widgets::goal_workspace::max_detail_scroll(
                    area,
                    &self.tasks,
                    goal_run_id,
                    &self.goal_workspace,
                );
                let next = if delta >= 0 {
                    self.goal_workspace
                        .detail_scroll()
                        .saturating_add(delta as usize)
                        .min(max_scroll)
                } else {
                    self.goal_workspace
                        .detail_scroll()
                        .saturating_sub((-delta) as usize)
                };
                self.goal_workspace.set_detail_scroll(next);
            }
        }
    }

    fn byte_offset_for_display_col(text: &str, target_col: usize) -> usize {
        use unicode_width::UnicodeWidthChar;

        let mut used = 0usize;
        for (idx, ch) in text.char_indices() {
            let width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if target_col <= used || target_col < used + width {
                return idx;
            }
            used += width;
        }
        text.len()
    }

    pub(super) fn input_offset_from_mouse(
        &self,
        input_start_row: u16,
        mouse: MouseEvent,
    ) -> Option<usize> {
        let inner_width = self.input_wrap_width();
        if inner_width == 0 {
            return Some(0);
        }

        let inner_row = mouse.row.saturating_sub(input_start_row + 1) as usize;
        let inner_col = mouse.column.saturating_sub(2) as usize;
        let attachment_rows = self.attachments.len();
        if inner_row < attachment_rows {
            return None;
        }

        let target_visual_row = inner_row - attachment_rows;
        let wrapped = self.input.wrapped_display_buffer(inner_width);
        if wrapped.is_empty() {
            return Some(0);
        }

        let mut wrapped_offset = 0usize;
        for (row_idx, line) in wrapped.split('\n').enumerate() {
            if row_idx == target_visual_row {
                let capped_col = inner_col.min(inner_width);
                let byte_in_line = Self::byte_offset_for_display_col(line, capped_col);
                return Some(self.input.wrapped_display_offset_to_buffer_offset(
                    wrapped_offset + byte_in_line,
                    inner_width,
                ));
            }
            wrapped_offset += line.len() + 1;
        }

        Some(self.input.buffer().len())
    }

    pub(super) fn handle_chat_click(&mut self, chat_area: Rect, mouse: Position) {
        match self.cached_chat_hit_test(chat_area, mouse) {
            Some(chat::ChatHitTarget::Message(idx)) => self.chat.toggle_message_selection(idx),
            Some(chat::ChatHitTarget::ReasoningToggle(idx)) => {
                self.chat.select_message(Some(idx));
                self.chat.toggle_reasoning(idx);
            }
            Some(chat::ChatHitTarget::ToolToggle(idx)) => {
                self.chat.select_message(Some(idx));
                self.chat.toggle_tool_expansion(idx);
            }
            Some(chat::ChatHitTarget::ToolFilePath { message_index }) => {
                self.chat.select_message(Some(message_index));
                self.open_chat_tool_file_preview(message_index);
            }
            Some(chat::ChatHitTarget::MessageImage { message_index }) => {
                self.chat.select_message(Some(message_index));
                self.open_chat_message_image_preview(message_index);
            }
            Some(chat::ChatHitTarget::RetryStartNow) => {
                if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                    self.retry_wait_start_selected = true;
                    self.chat.reduce(chat::ChatAction::ClearRetryStatus {
                        thread_id: thread_id.clone(),
                    });
                    self.send_daemon_command(DaemonCommand::RetryStreamNow { thread_id });
                    self.status_line = "Retrying now...".to_string();
                    self.set_active_thread_activity("retrying");
                }
            }
            Some(chat::ChatHitTarget::RetryStop) => {
                if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                    self.retry_wait_start_selected = false;
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
            Some(chat::ChatHitTarget::MessageAction {
                message_index,
                action_index,
            }) => {
                self.chat.select_message(Some(message_index));
                self.chat.select_message_action(action_index);
                self.execute_concierge_message_action(message_index, action_index);
            }
            Some(chat::ChatHitTarget::CopyMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.copy_message(idx);
            }
            Some(chat::ChatHitTarget::ForkMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.fork_message(idx);
            }
            Some(chat::ChatHitTarget::ResendMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.resend_message(idx);
            }
            Some(chat::ChatHitTarget::RegenerateMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.request_regenerate_message(idx);
            }
            Some(chat::ChatHitTarget::PinMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.pin_message_for_compaction(idx);
            }
            Some(chat::ChatHitTarget::UnpinMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.unpin_message_for_compaction(idx);
            }
            Some(chat::ChatHitTarget::DeleteMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.request_delete_message(idx);
            }
            Some(chat::ChatHitTarget::ThumbsUp(idx)) => {
                self.chat.select_message(Some(idx));
                self.submit_message_feedback(idx, zorai_protocol::Reaction::Up);
            }
            Some(chat::ChatHitTarget::ThumbsDown(idx)) => {
                self.chat.select_message(Some(idx));
                self.submit_message_feedback(idx, zorai_protocol::Reaction::Down);
            }
            Some(chat::ChatHitTarget::ExportMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.export_thread(idx);
            }
            None => {}
        }
    }

    pub(super) fn handle_task_view_click(&mut self, chat_area: Rect, mouse: Position) {
        let MainPaneView::Task(target) = &self.main_pane_view else {
            return;
        };
        if let sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. } = target {
            if let Some(pane) = widgets::goal_workspace::pane_at(chat_area, mouse) {
                self.goal_workspace.set_focused_pane(pane);
                self.focus = FocusArea::Chat;
            }
            let Some(hit) = widgets::goal_workspace::hit_test(
                chat_area,
                &self.tasks,
                goal_run_id,
                &self.goal_workspace,
                mouse,
            ) else {
                return;
            };
            match hit {
                widgets::goal_workspace::GoalWorkspaceHitTarget::ModeTab(mode) => {
                    let _ = self.set_goal_workspace_mode(mode);
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::PlanPromptToggle => {
                    let _ = self.select_goal_workspace_plan_item(
                        crate::state::goal_workspace::GoalPlanSelection::PromptToggle,
                    );
                    let _ = self.activate_goal_workspace_plan_target();
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::PlanMainThread(thread_id) => {
                    let _ = self.select_goal_workspace_plan_item(
                        crate::state::goal_workspace::GoalPlanSelection::MainThread {
                            thread_id: thread_id.clone(),
                        },
                    );
                    let _ = self.activate_goal_workspace_plan_target();
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::PlanStep(step_id) => {
                    let _ = self.select_goal_workspace_plan_item(
                        crate::state::goal_workspace::GoalPlanSelection::Step { step_id },
                    );
                    let _ = self.activate_goal_workspace_plan_target();
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::PlanTodo { step_id, todo_id } => {
                    let _ = self.select_goal_workspace_plan_item(
                        crate::state::goal_workspace::GoalPlanSelection::Todo { step_id, todo_id },
                    );
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::TimelineRow(row) => {
                    self.goal_workspace.set_selected_timeline_row(row);
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::ThreadRow(thread_id) => {
                    if let Some((row, _)) = widgets::goal_workspace::timeline_targets(
                        &self.tasks,
                        goal_run_id,
                        &self.goal_workspace,
                    )
                    .into_iter()
                    .find(|(_, target)| {
                        *target
                            == widgets::goal_workspace::GoalWorkspaceHitTarget::ThreadRow(
                                thread_id.clone(),
                            )
                    }) {
                        self.goal_workspace.set_selected_timeline_row(row);
                    }
                    let _ = self.activate_goal_workspace_timeline_target();
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::DetailFile(path) => {
                    if let Some((row, _)) = widgets::goal_workspace::timeline_targets(
                        &self.tasks,
                        goal_run_id,
                        &self.goal_workspace,
                    )
                    .into_iter()
                    .find(|(_, target)| {
                        *target
                            == widgets::goal_workspace::GoalWorkspaceHitTarget::DetailFile(
                                path.clone(),
                            )
                    }) {
                        self.goal_workspace.set_selected_timeline_row(row);
                        let _ = self.activate_goal_workspace_timeline_target();
                    } else if let Some(row) = widgets::goal_workspace::detail_row_for_target(
                        &self.tasks,
                        goal_run_id,
                        &self.goal_workspace,
                        &widgets::goal_workspace::GoalWorkspaceHitTarget::DetailFile(path.clone()),
                    ) {
                        self.goal_workspace.set_selected_detail_row(row);
                        let _ = self.activate_goal_workspace_detail_target();
                    }
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::DetailCheckpoint(id) => {
                    let target_row = widgets::goal_workspace::detail_row_for_target(
                        &self.tasks,
                        goal_run_id,
                        &self.goal_workspace,
                        &widgets::goal_workspace::GoalWorkspaceHitTarget::DetailCheckpoint(id),
                    );
                    if let Some(row) = target_row {
                        self.goal_workspace.set_selected_detail_row(row);
                    }
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::DetailTask(task_id) => {
                    if let Some(row) = widgets::goal_workspace::detail_row_for_target(
                        &self.tasks,
                        goal_run_id,
                        &self.goal_workspace,
                        &widgets::goal_workspace::GoalWorkspaceHitTarget::DetailTask(task_id),
                    ) {
                        self.goal_workspace.set_selected_detail_row(row);
                    }
                    let _ = self.activate_goal_workspace_detail_target();
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::DetailThread(thread_id) => {
                    if let Some(row) = widgets::goal_workspace::detail_row_for_target(
                        &self.tasks,
                        goal_run_id,
                        &self.goal_workspace,
                        &widgets::goal_workspace::GoalWorkspaceHitTarget::DetailThread(thread_id),
                    ) {
                        self.goal_workspace.set_selected_detail_row(row);
                    }
                    let _ = self.activate_goal_workspace_detail_target();
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::DetailAction(action) => {
                    if let Some(row) = widgets::goal_workspace::detail_row_for_target(
                        &self.tasks,
                        goal_run_id,
                        &self.goal_workspace,
                        &widgets::goal_workspace::GoalWorkspaceHitTarget::DetailAction(action),
                    ) {
                        self.goal_workspace.set_selected_detail_row(row);
                    }
                    let _ = self.activate_goal_workspace_detail_target();
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::FooterAction(action) => {
                    let _ = self.activate_goal_workspace_action(action);
                }
                widgets::goal_workspace::GoalWorkspaceHitTarget::DetailTimelineDetails(index) => {
                    if let Some(row) = widgets::goal_workspace::detail_row_for_target(
                        &self.tasks,
                        goal_run_id,
                        &self.goal_workspace,
                        &widgets::goal_workspace::GoalWorkspaceHitTarget::DetailTimelineDetails(
                            index,
                        ),
                    ) {
                        self.goal_workspace.set_selected_detail_row(row);
                    }
                    let _ = self.activate_goal_workspace_detail_target();
                }
            }
            return;
        }

        let Some(hit) = widgets::task_view::hit_test(
            chat_area,
            &self.tasks,
            target,
            &self.theme,
            self.task_view_scroll,
            self.task_show_live_todos,
            self.task_show_timeline,
            self.task_show_files,
            mouse,
        ) else {
            return;
        };
        match hit {
            widgets::task_view::TaskViewHitTarget::BackToGoal => {
                let sidebar::SidebarItemTarget::Task { task_id } = target else {
                    return;
                };
                let Some(parent_target) = self.parent_goal_target_for_task(task_id) else {
                    return;
                };
                self.open_sidebar_target(parent_target);
                self.focus = FocusArea::Chat;
            }
            widgets::task_view::TaskViewHitTarget::GoalStep(step_id) => {
                let _ = self.select_goal_step_in_active_run(step_id);
            }
            widgets::task_view::TaskViewHitTarget::WorkPath(path) => {
                let Some(thread_id) = self.target_thread_id(target) else {
                    return;
                };
                self.tasks.reduce(task::TaskAction::SelectWorkPath {
                    thread_id: thread_id.clone(),
                    path: Some(path),
                });
                self.request_preview_for_selected_path(&thread_id);
            }
            widgets::task_view::TaskViewHitTarget::ClosePreview => {
                let Some(thread_id) = self.target_thread_id(target) else {
                    return;
                };
                self.tasks.reduce(task::TaskAction::SelectWorkPath {
                    thread_id,
                    path: None,
                });
            }
        }
    }

    pub(in super::super) fn goal_sidebar_hit_test(
        &self,
        sidebar_area: Rect,
        mouse: MouseEvent,
    ) -> Option<widgets::goal_sidebar::GoalSidebarHitTarget> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return None;
        };

        widgets::goal_sidebar::hit_test(
            sidebar_area,
            &self.tasks,
            goal_run_id,
            &self.goal_sidebar,
            Position::new(mouse.column, mouse.row),
        )
    }

    pub(in crate::app) fn modal_navigate_to(&mut self, target: usize) {
        let current = self.modal.picker_cursor();
        self.modal_navigate(target as i32 - current as i32);
    }

    pub(in crate::app) fn modal_navigate(&mut self, delta: i32) {
        self.modal.reduce(modal::ModalAction::Navigate(delta));
    }

    pub(in super::super) fn settings_navigate_to(&mut self, target: usize) {
        let current = self.settings.field_cursor();
        self.settings
            .navigate_field(target as i32 - current as i32, self.settings_field_count());
    }

    pub(super) fn handle_modal_mouse(&mut self, mouse: MouseEvent) {
        let Some((kind, overlay_area)) = self.current_modal_area() else {
            return;
        };

        let inside = mouse.column >= overlay_area.x
            && mouse.column < overlay_area.x.saturating_add(overlay_area.width)
            && mouse.row >= overlay_area.y
            && mouse.row < overlay_area.y.saturating_add(overlay_area.height);

        match mouse.kind {
            MouseEventKind::ScrollUp if inside => match kind {
                modal::ModalKind::Settings => {
                    self.step_settings_modal_scroll(-3);
                }
                modal::ModalKind::CommandPalette | modal::ModalKind::OperatorProfileOnboarding => {
                    self.modal_navigate(-1);
                }
                modal::ModalKind::ThreadPicker
                | modal::ModalKind::GoalPicker
                | modal::ModalKind::WorkspacePicker
                | modal::ModalKind::WorkspaceActorPicker
                | modal::ModalKind::QueuedPrompts
                | modal::ModalKind::ProviderPicker
                | modal::ModalKind::ModelPicker
                | modal::ModalKind::RolePicker
                | modal::ModalKind::OpenAIAuth
                | modal::ModalKind::WorkspaceTaskHistory
                | modal::ModalKind::ThreadParticipants
                | modal::ModalKind::ThreadHandoff
                | modal::ModalKind::ThreadParticipantAgentPicker
                | modal::ModalKind::ThreadParticipantActions
                | modal::ModalKind::GatewayDefaultAgentPicker
                | modal::ModalKind::GatewayLink
                | modal::ModalKind::EffortPicker => {
                    self.modal.reduce(modal::ModalAction::Navigate(-1));
                }
                modal::ModalKind::Notifications => {
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusHeader(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusRowAction(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::Navigate(-1));
                }
                modal::ModalKind::ApprovalCenter => {
                    self.step_approval_selection(-1);
                }
                modal::ModalKind::Status => {
                    self.step_status_modal_scroll(-3);
                }
                modal::ModalKind::Statistics => {
                    self.step_statistics_modal_scroll(-3);
                }
                modal::ModalKind::PromptViewer => {
                    self.step_prompt_modal_scroll(-3);
                }
                modal::ModalKind::WorkspaceEditTask => {
                    self.step_workspace_edit_modal_scroll(-3);
                }
                modal::ModalKind::Help => {
                    self.step_help_modal_scroll(-3);
                }
                _ => {}
            },
            MouseEventKind::ScrollDown if inside => match kind {
                modal::ModalKind::Settings => {
                    self.step_settings_modal_scroll(3);
                }
                modal::ModalKind::CommandPalette | modal::ModalKind::OperatorProfileOnboarding => {
                    self.modal_navigate(1);
                }
                modal::ModalKind::ThreadPicker
                | modal::ModalKind::GoalPicker
                | modal::ModalKind::WorkspacePicker
                | modal::ModalKind::WorkspaceActorPicker
                | modal::ModalKind::QueuedPrompts
                | modal::ModalKind::ProviderPicker
                | modal::ModalKind::ModelPicker
                | modal::ModalKind::RolePicker
                | modal::ModalKind::OpenAIAuth
                | modal::ModalKind::WorkspaceTaskHistory
                | modal::ModalKind::ThreadParticipants
                | modal::ModalKind::ThreadHandoff
                | modal::ModalKind::ThreadParticipantAgentPicker
                | modal::ModalKind::ThreadParticipantActions
                | modal::ModalKind::GatewayDefaultAgentPicker
                | modal::ModalKind::GatewayLink
                | modal::ModalKind::EffortPicker => {
                    self.modal.reduce(modal::ModalAction::Navigate(1));
                }
                modal::ModalKind::Notifications => {
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusHeader(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusRowAction(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::Navigate(1));
                }
                modal::ModalKind::ApprovalCenter => {
                    self.step_approval_selection(1);
                }
                modal::ModalKind::Status => {
                    self.step_status_modal_scroll(3);
                }
                modal::ModalKind::Statistics => {
                    self.step_statistics_modal_scroll(3);
                }
                modal::ModalKind::PromptViewer => {
                    self.step_prompt_modal_scroll(3);
                }
                modal::ModalKind::WorkspaceEditTask => {
                    self.step_workspace_edit_modal_scroll(3);
                }
                modal::ModalKind::Help => {
                    self.step_help_modal_scroll(3);
                }
                _ => {}
            },
            MouseEventKind::Down(MouseButton::Left) if !inside => {
                if matches!(
                    kind,
                    modal::ModalKind::Help
                        | modal::ModalKind::Status
                        | modal::ModalKind::Statistics
                        | modal::ModalKind::PromptViewer
                        | modal::ModalKind::CommandPalette
                        | modal::ModalKind::ThreadHandoff
                        | modal::ModalKind::ThreadPicker
                        | modal::ModalKind::GoalPicker
                        | modal::ModalKind::WorkspacePicker
                        | modal::ModalKind::WorkspaceCreateTask
                        | modal::ModalKind::WorkspaceEditTask
                        | modal::ModalKind::WorkspaceTaskDetail
                        | modal::ModalKind::WorkspaceTaskHistory
                        | modal::ModalKind::WorkspaceActorPicker
                        | modal::ModalKind::QueuedPrompts
                        | modal::ModalKind::ProviderPicker
                        | modal::ModalKind::ModelPicker
                        | modal::ModalKind::RolePicker
                        | modal::ModalKind::OpenAIAuth
                        | modal::ModalKind::ErrorViewer
                        | modal::ModalKind::Notifications
                        | modal::ModalKind::ApprovalCenter
                        | modal::ModalKind::EffortPicker
                        | modal::ModalKind::ChatActionConfirm
                        | modal::ModalKind::PinnedBudgetExceeded
                ) {
                    if kind == modal::ModalKind::ChatActionConfirm {
                        self.close_chat_action_confirm();
                    } else if kind == modal::ModalKind::PinnedBudgetExceeded {
                        self.close_pinned_budget_exceeded_modal();
                    } else {
                        self.close_top_modal();
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Right) if inside => {
                if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                    if !text.is_empty() {
                        self.handle_paste(text);
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Left) => match kind {
                modal::ModalKind::Settings => {
                    match widgets::settings::hit_test(
                        overlay_area,
                        &self.settings,
                        &self.config,
                        &self.auth,
                        &self.subagents,
                        self.settings_modal_scroll,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        Some(widgets::settings::SettingsHitTarget::EditCursor { line, col }) => {
                            self.settings
                                .reduce(SettingsAction::SetCursorLineCol(line, col));
                        }
                        Some(widgets::settings::SettingsHitTarget::Tab(tab)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.settings.reduce(SettingsAction::SwitchTab(tab));
                            self.settings_modal_scroll = 0;
                            if matches!(tab, SettingsTab::SubAgents) {
                                self.send_daemon_command(DaemonCommand::ListSubAgents);
                            } else if matches!(tab, SettingsTab::Concierge) {
                                self.send_daemon_command(DaemonCommand::GetConciergeConfig);
                            } else if matches!(tab, SettingsTab::Gateway) {
                                self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                            } else if matches!(tab, SettingsTab::Plugins) {
                                self.plugin_settings.list_mode = true;
                                self.send_daemon_command(DaemonCommand::PluginList);
                            } else if matches!(tab, SettingsTab::Mlflow) {
                                self.send_daemon_command(DaemonCommand::GetMlflowTracingStatus);
                            }
                        }
                        Some(widgets::settings::SettingsHitTarget::AuthProviderItem(index)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.auth.selected =
                                index.min(self.auth.entries.len().saturating_sub(1));
                            self.auth.actions_focused = false;
                        }
                        Some(widgets::settings::SettingsHitTarget::AuthAction {
                            index,
                            action,
                        }) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.auth.selected =
                                index.min(self.auth.entries.len().saturating_sub(1));
                            self.auth.actions_focused = true;
                            self.auth.action_cursor = match action {
                                widgets::settings::AuthTabAction::Primary => 0,
                                widgets::settings::AuthTabAction::Test => 1,
                            };
                            self.run_auth_tab_action();
                        }
                        Some(widgets::settings::SettingsHitTarget::SubAgentListItem(index)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.subagents
                                .reduce(crate::state::subagents::SubAgentsAction::Select(index));
                            self.subagents.actions_focused = false;
                        }
                        Some(widgets::settings::SettingsHitTarget::SubAgentAction(action)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.subagents.actions_focused = true;
                            self.subagents.action_cursor = match action {
                                widgets::settings::SubAgentTabAction::Add => 0,
                                widgets::settings::SubAgentTabAction::Edit => 1,
                                widgets::settings::SubAgentTabAction::Delete => 2,
                                widgets::settings::SubAgentTabAction::Toggle => 3,
                            };
                            self.run_subagent_action();
                        }
                        Some(widgets::settings::SettingsHitTarget::SubAgentRowAction {
                            index,
                            action,
                        }) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.subagents
                                .reduce(crate::state::subagents::SubAgentsAction::Select(index));
                            self.subagents.actions_focused = true;
                            self.subagents.action_cursor = match action {
                                widgets::settings::SubAgentTabAction::Add => 0,
                                widgets::settings::SubAgentTabAction::Edit => 1,
                                widgets::settings::SubAgentTabAction::Delete => 2,
                                widgets::settings::SubAgentTabAction::Toggle => 3,
                            };
                            self.run_subagent_action();
                        }
                        Some(widgets::settings::SettingsHitTarget::Field(field)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.settings_navigate_to(field);
                            if self.settings_field_click_uses_toggle() {
                                self.toggle_settings_field();
                            } else {
                                self.activate_settings_field();
                            }
                        }
                        None => {}
                    }
                }
                modal::ModalKind::Notifications => {
                    if let Some(target) = widgets::notifications::hit_test(
                        overlay_area,
                        &self.notifications,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        match target {
                            widgets::notifications::NotificationsHitTarget::MarkAllRead => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusHeader(Some(
                                        crate::state::NotificationsHeaderAction::MarkAllRead,
                                    )),
                                );
                                self.mark_all_notifications_read();
                            }
                            widgets::notifications::NotificationsHitTarget::ArchiveRead => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusHeader(Some(
                                        crate::state::NotificationsHeaderAction::ArchiveRead,
                                    )),
                                );
                                self.archive_read_notifications();
                            }
                            widgets::notifications::NotificationsHitTarget::Close => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusHeader(Some(
                                        crate::state::NotificationsHeaderAction::Close,
                                    )),
                                );
                                self.close_top_modal();
                            }
                            widgets::notifications::NotificationsHitTarget::Row(index) => {
                                self.notifications
                                    .reduce(crate::state::NotificationsAction::FocusHeader(None));
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(None),
                                );
                                self.notifications
                                    .reduce(crate::state::NotificationsAction::Select(index));
                                if let Some(id) = self
                                    .notifications
                                    .selected_item()
                                    .map(|notification| notification.id.clone())
                                {
                                    self.toggle_notification_expand(id);
                                }
                            }
                            widgets::notifications::NotificationsHitTarget::ToggleExpand(id) => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(0)),
                                );
                                self.execute_notification_row_action(&id, 0);
                            }
                            widgets::notifications::NotificationsHitTarget::MarkRead(id) => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(1)),
                                );
                                self.execute_notification_row_action(&id, 1);
                            }
                            widgets::notifications::NotificationsHitTarget::Archive(id) => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(2)),
                                );
                                self.execute_notification_row_action(&id, 2);
                            }
                            widgets::notifications::NotificationsHitTarget::Delete(id) => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(3)),
                                );
                                self.execute_notification_row_action(&id, 3);
                            }
                            widgets::notifications::NotificationsHitTarget::Action {
                                notification_id,
                                action_index,
                            } => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(
                                        action_index + 4,
                                    )),
                                );
                                self.execute_notification_row_action(
                                    &notification_id,
                                    action_index + 4,
                                );
                            }
                        }
                    }
                }
                modal::ModalKind::ApprovalCenter => {
                    if let Some(target) = widgets::approval_center::hit_test(
                        overlay_area,
                        &self.approval,
                        self.chat.active_thread_id(),
                        self.current_workspace_id(),
                        Position::new(mouse.column, mouse.row),
                    ) {
                        match target {
                            widgets::approval_center::ApprovalCenterHitTarget::Filter(filter) => {
                                self.approval
                                    .reduce(crate::state::ApprovalAction::SetFilter(filter));
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::Row(index) => {
                                self.select_approval_center_row(index);
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::RuleRow(index) => {
                                self.select_approval_center_rule_row(index);
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::ThreadJump(
                                thread_id,
                            ) => {
                                self.open_thread_conversation(thread_id);
                                self.close_top_modal();
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::ApproveOnce(
                                approval_id,
                            ) => {
                                self.resolve_approval(approval_id, "allow_once");
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::ApproveSession(
                                approval_id,
                            ) => {
                                self.resolve_approval(approval_id, "allow_session");
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::AlwaysApprove(
                                approval_id,
                            ) => {
                                self.create_task_approval_rule(approval_id);
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::RevokeRule(
                                rule_id,
                            ) => {
                                self.approval
                                    .reduce(crate::state::ApprovalAction::RemoveRule(
                                        rule_id.clone(),
                                    ));
                                self.send_daemon_command(DaemonCommand::RevokeTaskApprovalRule {
                                    rule_id,
                                });
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::Deny(
                                approval_id,
                            ) => {
                                self.handle_reject_selected_approval(approval_id);
                            }
                        }
                    }
                }
                modal::ModalKind::ThreadHandoff => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let body_height = inner.height.saturating_sub(1);
                    let inside_body = mouse.column >= inner.x
                        && mouse.column < inner.x.saturating_add(inner.width)
                        && mouse.row >= inner.y
                        && mouse.row < inner.y.saturating_add(body_height);
                    if inside_body {
                        let body_line = self
                            .thread_handoff_modal_cursor_scroll()
                            .saturating_add(mouse.row.saturating_sub(inner.y) as usize);
                        if let Some(index) = body_line
                            .checked_sub(self.thread_handoff_modal_action_start_line())
                            .filter(|index| *index < self.thread_handoff_modal_actions().len())
                        {
                            self.modal_navigate_to(index);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::CommandPalette => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1),
                            Constraint::Length(1),
                            Constraint::Min(1),
                            Constraint::Length(1),
                        ])
                        .split(inner);
                    if mouse.row >= chunks[2].y
                        && mouse.row < chunks[2].y.saturating_add(chunks[2].height)
                    {
                        let row_idx = mouse.row.saturating_sub(chunks[2].y) as usize;
                        let (visible_start, visible_len) = widgets::thread_picker::visible_window(
                            self.modal.picker_cursor(),
                            self.modal.filtered_items().len(),
                            chunks[2].height as usize,
                        );
                        if row_idx < visible_len {
                            self.modal_navigate_to(visible_start + row_idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::ThreadPicker => {
                    match widgets::thread_picker::hit_test_for_workspace(
                        overlay_area,
                        &self.chat,
                        &self.modal,
                        &self.subagents,
                        &self.tasks,
                        &self.workspace,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        Some(widgets::thread_picker::ThreadPickerHitTarget::Tab(tab)) => {
                            self.modal.set_thread_picker_tab(tab.clone());
                            self.sync_thread_picker_item_count();
                            self.queue_threads_for_picker_tab_refresh(&tab);
                        }
                        Some(widgets::thread_picker::ThreadPickerHitTarget::Item(idx)) => {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                        None => {}
                    }
                }
                modal::ModalKind::OperatorProfileOnboarding => {
                    let view = self.operator_profile_onboarding_view();
                    if let Some(index) = widgets::operator_profile_onboarding::index_at_position(
                        overlay_area,
                        &view,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        self.modal_navigate_to(index);
                        self.handle_modal_enter(kind);
                    }
                }
                modal::ModalKind::GoalPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1),
                            Constraint::Length(1),
                            Constraint::Min(1),
                            Constraint::Length(1),
                        ])
                        .split(inner);
                    if mouse.row >= chunks[2].y
                        && mouse.row < chunks[2].y.saturating_add(chunks[2].height)
                    {
                        let row_idx = mouse.row.saturating_sub(chunks[2].y) as usize;
                        let total_items = self.filtered_goal_runs().len() + 1;
                        let (visible_start, visible_len) = widgets::thread_picker::visible_window(
                            self.modal.picker_cursor(),
                            total_items,
                            chunks[2].height as usize,
                        );
                        if row_idx < visible_len {
                            self.modal_navigate_to(visible_start + row_idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::WorkspacePicker => {
                    if let Some(widgets::workspace_picker::WorkspacePickerHitTarget::Item(idx)) =
                        widgets::workspace_picker::hit_test(
                            overlay_area,
                            &self.workspace,
                            &self.modal,
                            Position::new(mouse.column, mouse.row),
                        )
                    {
                        self.modal_navigate_to(idx);
                        self.handle_modal_enter(kind);
                    }
                }
                modal::ModalKind::WorkspaceCreateTask => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let options_start_row = inner.y.saturating_add(2);
                    if mouse.row >= options_start_row && mouse.row < options_start_row + 9 {
                        let index = mouse.row.saturating_sub(options_start_row) as usize;
                        if let Some(form) = self.pending_workspace_create_form.as_mut() {
                            form.field = match index {
                                0 => crate::app::workspace_create_modal::WorkspaceCreateTaskField::Title,
                                1 => crate::app::workspace_create_modal::WorkspaceCreateTaskField::TaskType,
                                2 => crate::app::workspace_create_modal::WorkspaceCreateTaskField::Description,
                                3 => crate::app::workspace_create_modal::WorkspaceCreateTaskField::DefinitionOfDone,
                                4 => crate::app::workspace_create_modal::WorkspaceCreateTaskField::Priority,
                                5 => crate::app::workspace_create_modal::WorkspaceCreateTaskField::Assignee,
                                6 => crate::app::workspace_create_modal::WorkspaceCreateTaskField::Reviewer,
                                7 => crate::app::workspace_create_modal::WorkspaceCreateTaskField::Submit,
                                _ => crate::app::workspace_create_modal::WorkspaceCreateTaskField::Cancel,
                            };
                        }
                        if index == 1 || index >= 4 {
                            self.handle_workspace_create_modal_key(
                                KeyCode::Enter,
                                KeyModifiers::NONE,
                            );
                        }
                    }
                }
                modal::ModalKind::WorkspaceEditTask => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let options_start_row = inner.y.saturating_add(2);
                    if mouse.row >= options_start_row && mouse.row < options_start_row + 8 {
                        let index = mouse.row.saturating_sub(options_start_row) as usize;
                        if let Some(form) = self.pending_workspace_edit_form.as_mut() {
                            form.field = match index {
                                0 => crate::app::workspace_edit_modal::WorkspaceEditField::Title,
                                1 => crate::app::workspace_edit_modal::WorkspaceEditField::Description,
                                2 => crate::app::workspace_edit_modal::WorkspaceEditField::DefinitionOfDone,
                                3 => crate::app::workspace_edit_modal::WorkspaceEditField::Priority,
                                4 => crate::app::workspace_edit_modal::WorkspaceEditField::Assignee,
                                5 => crate::app::workspace_edit_modal::WorkspaceEditField::Reviewer,
                                6 => crate::app::workspace_edit_modal::WorkspaceEditField::Submit,
                                _ => crate::app::workspace_edit_modal::WorkspaceEditField::Cancel,
                            };
                        }
                        if index >= 3 {
                            self.handle_workspace_edit_modal_key(
                                KeyCode::Enter,
                                KeyModifiers::NONE,
                            );
                        }
                    }
                }
                modal::ModalKind::WorkspaceActorPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let options = self
                        .pending_workspace_actor_picker
                        .as_ref()
                        .map(|pending| {
                            crate::app::workspace_actor_picker::workspace_actor_picker_options(
                                pending.mode,
                                &self.subagents,
                            )
                        })
                        .unwrap_or_default();
                    let body_bottom = inner.y.saturating_add(inner.height.saturating_sub(1));
                    if mouse.row >= inner.y && mouse.row < body_bottom {
                        let body_line = self
                            .workspace_actor_picker_scroll()
                            .saturating_add(mouse.row.saturating_sub(inner.y) as usize);
                        let index = body_line.saturating_sub(3);
                        if body_line < 3 || index >= options.len() {
                            return;
                        }
                        self.modal_navigate_to(index);
                        self.handle_modal_enter(kind);
                    }
                }
                modal::ModalKind::WorkspaceTaskHistory => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let row_start = inner.y.saturating_add(3);
                    let Some(task_id) = self.pending_workspace_history_task_id.as_deref() else {
                        return;
                    };
                    let Some(task) = self.workspace.task_by_id(task_id) else {
                        return;
                    };
                    let item_count = task.runtime_history.len();
                    if mouse.row >= row_start
                        && mouse.row < row_start.saturating_add(item_count.saturating_mul(3) as u16)
                    {
                        let index = mouse.row.saturating_sub(row_start) as usize / 3;
                        if index < item_count {
                            self.modal_navigate_to(index);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::QueuedPrompts => {
                    if let Some(target) = widgets::queued_prompts::hit_test(
                        overlay_area,
                        &self.queued_prompts,
                        self.modal.picker_cursor(),
                        self.tick_counter,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        match target {
                            widgets::queued_prompts::QueuedPromptsHitTarget::Row(index) => {
                                self.modal_navigate_to(index);
                                self.queued_prompt_action = QueuedPromptAction::Expand;
                                self.execute_selected_queued_prompt_action();
                            }
                            widgets::queued_prompts::QueuedPromptsHitTarget::Action {
                                message_index,
                                action,
                            } => {
                                self.modal_navigate_to(message_index);
                                self.queued_prompt_action = action;
                                self.execute_selected_queued_prompt_action();
                            }
                        }
                    }
                }
                modal::ModalKind::Statistics => {
                    match widgets::statistics::hit_test(
                        overlay_area,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        Some(widgets::statistics::StatisticsHitTarget::Tab(tab)) => {
                            self.select_statistics_tab(tab);
                        }
                        Some(widgets::statistics::StatisticsHitTarget::Window(window)) => {
                            if window != self.statistics_modal_window {
                                self.request_statistics_window(window);
                            }
                        }
                        None => {}
                    }
                }
                modal::ModalKind::ProviderPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    if mouse.row >= inner.y
                        && mouse.row < inner.y.saturating_add(inner.height.saturating_sub(1))
                    {
                        let idx = mouse.row.saturating_sub(inner.y) as usize;
                        if idx < self.filtered_provider_picker_defs().len() {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::ModelPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    if mouse.row >= inner.y
                        && mouse.row < inner.y.saturating_add(inner.height.saturating_sub(1))
                    {
                        let idx = mouse.row.saturating_sub(inner.y) as usize;
                        if idx <= self.filtered_model_picker_models().len() {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::RolePicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    if mouse.row >= inner.y
                        && mouse.row < inner.y.saturating_add(inner.height.saturating_sub(1))
                    {
                        let idx = mouse.row.saturating_sub(inner.y) as usize;
                        if idx < crate::state::subagents::role_picker_item_count() {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::OpenAIAuth => {}
                modal::ModalKind::EffortPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    if mouse.row >= inner.y
                        && mouse.row < inner.y.saturating_add(inner.height.saturating_sub(1))
                    {
                        let idx = mouse.row.saturating_sub(inner.y) as usize;
                        if idx < 7 {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::ApprovalOverlay => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let action_row = inner.y.saturating_add(inner.height.saturating_sub(1));
                    if mouse.row == action_row {
                        let third = inner.width / 3;
                        let rel_x = mouse.column.saturating_sub(inner.x);
                        let key = if rel_x < third {
                            KeyCode::Char('y')
                        } else if rel_x < third.saturating_mul(2) {
                            KeyCode::Char('a')
                        } else {
                            KeyCode::Char('n')
                        };
                        let _ = self.handle_key_modal(key, KeyModifiers::NONE, kind);
                    }
                }
                modal::ModalKind::ChatActionConfirm => {
                    if let Some((confirm_rect, cancel_rect)) =
                        render_helpers::chat_action_confirm_button_bounds(overlay_area)
                    {
                        if contains_mouse(confirm_rect, mouse) {
                            self.chat_action_confirm_accept_selected = true;
                        } else if contains_mouse(cancel_rect, mouse) {
                            self.chat_action_confirm_accept_selected = false;
                        }
                    }
                }
                modal::ModalKind::Help => {
                    self.close_top_modal();
                }
                _ => {}
            },
            MouseEventKind::Up(MouseButton::Left)
                if kind == modal::ModalKind::ChatActionConfirm =>
            {
                if let Some((confirm_rect, cancel_rect)) =
                    render_helpers::chat_action_confirm_button_bounds(overlay_area)
                {
                    if contains_mouse(confirm_rect, mouse) {
                        self.chat_action_confirm_accept_selected = true;
                        self.confirm_pending_chat_action();
                    } else if contains_mouse(cancel_rect, mouse) {
                        self.chat_action_confirm_accept_selected = false;
                        self.close_chat_action_confirm();
                    }
                }
            }
            _ => {}
        }
    }
}

pub(super) fn contains_mouse(rect: Rect, mouse: MouseEvent) -> bool {
    rect.width > 0
        && rect.height > 0
        && mouse.column >= rect.x
        && mouse.column < rect.x.saturating_add(rect.width)
        && mouse.row >= rect.y
        && mouse.row < rect.y.saturating_add(rect.height)
}
