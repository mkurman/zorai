use super::*;
impl TuiModel {
    pub(crate) fn chat_scrollbar_layout(&self) -> Option<widgets::chat::ChatScrollbarLayout> {
        self.chat_scrollbar_layout_for_area(self.pane_layout().chat)
    }

    pub(crate) fn chat_scrollbar_layout_for_area(
        &self,
        area: Rect,
    ) -> Option<widgets::chat::ChatScrollbarLayout> {
        if let Some(snapshot) = self.chat_selection_snapshot.as_ref().filter(|snapshot| {
            widgets::chat::cached_snapshot_matches_render_key(
                snapshot,
                area,
                &self.chat,
                self.tick_counter,
                self.retry_wait_start_selected,
            )
        }) {
            return widgets::chat::scrollbar_layout_from_cached_snapshot(
                snapshot,
                &self.chat,
                self.tick_counter,
                self.retry_wait_start_selected,
            );
        }

        widgets::chat::scrollbar_layout(
            area,
            &self.chat,
            &self.theme,
            self.tick_counter,
            self.retry_wait_start_selected,
        )
    }

    pub(crate) fn chat_scrollbar_scroll_offset_for_pointer(
        &self,
        area: Rect,
        pointer_row: u16,
        grab_offset: u16,
    ) -> Option<usize> {
        if let Some(snapshot) = self.chat_selection_snapshot.as_ref().filter(|snapshot| {
            widgets::chat::cached_snapshot_matches_render_key(
                snapshot,
                area,
                &self.chat,
                self.tick_counter,
                self.retry_wait_start_selected,
            )
        }) {
            return widgets::chat::scrollbar_scroll_offset_for_pointer_from_cached_snapshot(
                snapshot,
                &self.chat,
                self.tick_counter,
                self.retry_wait_start_selected,
                pointer_row,
                grab_offset,
            );
        }

        widgets::chat::scrollbar_scroll_offset_for_pointer(
            area,
            &self.chat,
            &self.theme,
            self.tick_counter,
            self.retry_wait_start_selected,
            pointer_row,
            grab_offset,
        )
    }

    pub(crate) fn clamp_chat_scroll_offset_for_area(&mut self, area: Rect) {
        let max_scroll = self
            .chat_scrollbar_layout_for_area(area)
            .map(|layout| layout.max_scroll)
            .unwrap_or(0);
        let current_scroll = self.chat.scroll_offset();
        if current_scroll <= max_scroll {
            return;
        }

        self.chat_selection_snapshot = None;
        self.set_chat_scroll_offset(max_scroll);
    }

    pub(crate) fn capture_locked_chat_viewport(
        &self,
        thread_id: Option<&str>,
    ) -> Option<(usize, usize)> {
        let thread_id = thread_id?;
        if self.chat.active_thread_id() != Some(thread_id) || self.chat.scroll_offset() == 0 {
            return None;
        }

        self.chat_scrollbar_layout()
            .map(|layout| (layout.scroll, layout.max_scroll))
    }

    pub(crate) fn restore_locked_chat_viewport(&mut self, anchor: Option<(usize, usize)>) {
        let Some((before_scroll, before_max_scroll)) = anchor else {
            return;
        };

        let after_max_scroll = self
            .chat_scrollbar_layout()
            .map(|layout| layout.max_scroll)
            .unwrap_or(0);
        let target = if after_max_scroll >= before_max_scroll {
            before_scroll.saturating_add(after_max_scroll - before_max_scroll)
        } else {
            before_scroll.saturating_sub(before_max_scroll - after_max_scroll)
        };

        self.set_chat_scroll_offset(target.min(after_max_scroll));
    }

    pub(crate) fn reduce_chat_for_thread(
        &mut self,
        thread_id: Option<&str>,
        action: chat::ChatAction,
    ) {
        let anchor = self.capture_locked_chat_viewport(thread_id);
        self.chat.reduce(action);
        self.restore_locked_chat_viewport(anchor);
    }

    pub(crate) fn sidebar_visible(&self) -> bool {
        if matches!(
            self.main_pane_view,
            MainPaneView::Task(SidebarItemTarget::GoalRun { .. })
        ) {
            return false;
        }
        if !matches!(
            self.main_pane_view,
            MainPaneView::Conversation | MainPaneView::WorkContext
        ) {
            return false;
        }
        let Some(thread_id) = self.chat.active_thread_id() else {
            return false;
        };
        !self.tasks.todos_for_thread(thread_id).is_empty()
            || self
                .tasks
                .work_context_for_thread(thread_id)
                .is_some_and(|context| !context.entries.is_empty())
            || self.chat.active_thread_has_pinned_messages()
            || crate::widgets::sidebar::has_spawned_tab(&self.tasks, &self.chat, Some(thread_id))
    }

    pub(crate) fn current_attention_target(&self) -> (String, Option<String>, Option<String>) {
        if let Some(modal) = self.modal.top() {
            let surface = match modal {
                modal::ModalKind::Settings => {
                    format!(
                        "modal:settings:{}",
                        settings_tab_label(self.settings.active_tab())
                    )
                }
                modal::ModalKind::ApprovalOverlay => "modal:approval".to_string(),
                modal::ModalKind::GoalApprovalRejectPrompt => {
                    "modal:goal_approval_reject".to_string()
                }
                modal::ModalKind::OperatorQuestionOverlay => "modal:operator_question".to_string(),
                modal::ModalKind::OperatorProfileOnboarding => {
                    "modal:operator_profile_onboarding".to_string()
                }
                modal::ModalKind::ApprovalCenter => "modal:approval_center".to_string(),
                modal::ModalKind::ChatActionConfirm => "modal:chat_action_confirm".to_string(),
                modal::ModalKind::PinnedBudgetExceeded => {
                    "modal:pinned_budget_exceeded".to_string()
                }
                modal::ModalKind::CommandPalette => "modal:command_palette".to_string(),
                modal::ModalKind::ThreadParticipants => "modal:thread_participants".to_string(),
                modal::ModalKind::ThreadHandoff => "modal:thread_handoff".to_string(),
                modal::ModalKind::ThreadParticipantAgentPicker => {
                    "modal:thread_participant_agent_picker".to_string()
                }
                modal::ModalKind::ThreadParticipantActions => {
                    "modal:thread_participant_actions".to_string()
                }
                modal::ModalKind::GatewayDefaultAgentPicker => {
                    "modal:gateway_default_agent_picker".to_string()
                }
                modal::ModalKind::GatewayLink => "modal:gateway_link".to_string(),
                modal::ModalKind::ThreadPicker => "modal:thread_picker".to_string(),
                modal::ModalKind::GoalPicker => "modal:goal_picker".to_string(),
                modal::ModalKind::WorkspacePicker => "modal:workspace_picker".to_string(),
                modal::ModalKind::WorkspaceCreate => "modal:workspace_create".to_string(),
                modal::ModalKind::WorkspaceCreateTask => "modal:workspace_create_task".to_string(),
                modal::ModalKind::WorkspaceReviewTask => "modal:workspace_review_task".to_string(),
                modal::ModalKind::WorkspaceEditTask => "modal:workspace_edit_task".to_string(),
                modal::ModalKind::WorkspaceTaskDetail => "modal:workspace_task_detail".to_string(),
                modal::ModalKind::WorkspaceTaskHistory => {
                    "modal:workspace_task_history".to_string()
                }
                modal::ModalKind::WorkspaceActorPicker => {
                    "modal:workspace_actor_picker".to_string()
                }
                modal::ModalKind::GoalStepActionPicker => {
                    "modal:goal_step_action_picker".to_string()
                }
                modal::ModalKind::QueuedPrompts => "modal:queued_prompts".to_string(),
                modal::ModalKind::ProviderPicker => "modal:provider_picker".to_string(),
                modal::ModalKind::ModelPicker => "modal:model_picker".to_string(),
                modal::ModalKind::OpenRouterProviderPicker => {
                    "modal:openrouter_provider_picker".to_string()
                }
                modal::ModalKind::RolePicker => "modal:role_picker".to_string(),
                modal::ModalKind::OpenAIAuth => "modal:openai_auth".to_string(),
                modal::ModalKind::ErrorViewer => "modal:error_viewer".to_string(),
                modal::ModalKind::EffortPicker => "modal:effort_picker".to_string(),
                modal::ModalKind::ContextWindowEditor => "modal:context_window_editor".to_string(),
                modal::ModalKind::CustomModelEditor => "modal:custom_model_editor".to_string(),
                modal::ModalKind::Notifications => "modal:notifications".to_string(),
                modal::ModalKind::ToolsPicker => "modal:tools_picker".to_string(),
                modal::ModalKind::ViewPicker => "modal:view_picker".to_string(),
                modal::ModalKind::Status => "modal:status".to_string(),
                modal::ModalKind::Statistics => "modal:statistics".to_string(),
                modal::ModalKind::PromptViewer => "modal:prompt".to_string(),
                modal::ModalKind::Help => "modal:help".to_string(),
                modal::ModalKind::WhatsAppLink => "modal:whatsapp_link".to_string(),
            };
            return (
                surface,
                self.chat.active_thread_id().map(str::to_string),
                None,
            );
        }

        match &self.main_pane_view {
            MainPaneView::Conversation => match self.focus {
                FocusArea::Chat => (
                    "conversation:chat".to_string(),
                    self.chat.active_thread_id().map(str::to_string),
                    None,
                ),
                FocusArea::Input => {
                    if self.should_show_provider_onboarding() {
                        ("conversation:onboarding".to_string(), None, None)
                    } else {
                        (
                            "conversation:input".to_string(),
                            self.chat.active_thread_id().map(str::to_string),
                            None,
                        )
                    }
                }
                FocusArea::Sidebar => (
                    format!(
                        "conversation:sidebar:{}",
                        sidebar_tab_label(self.sidebar.active_tab())
                    ),
                    self.chat.active_thread_id().map(str::to_string),
                    None,
                ),
            },
            MainPaneView::Collaboration => (
                "collaboration:workspace".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                None,
            ),
            MainPaneView::Workspace => (
                "workspace:board".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                None,
            ),
            MainPaneView::Task(target) => (
                "task:detail".to_string(),
                self.target_thread_id(target),
                target_goal_run_id(self, target),
            ),
            MainPaneView::WorkContext => (
                "task:work_context".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                None,
            ),
            MainPaneView::FilePreview(target) => (
                "task:file_preview".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                target.repo_relative_path.clone(),
            ),
            MainPaneView::GoalComposer => (
                "task:goal_composer".to_string(),
                self.chat.active_thread_id().map(str::to_string),
                None,
            ),
        }
    }

    pub(crate) fn publish_attention_surface_if_changed(&mut self) {
        if !self.connected {
            return;
        }
        let (surface, thread_id, goal_run_id) = self.current_attention_target();
        let signature = format!(
            "{}|{}|{}",
            surface,
            thread_id.as_deref().unwrap_or_default(),
            goal_run_id.as_deref().unwrap_or_default()
        );
        if self.last_attention_surface.as_deref() == Some(signature.as_str()) {
            return;
        }
        self.last_attention_surface = Some(signature);
        self.send_daemon_command(DaemonCommand::RecordAttention {
            surface,
            thread_id,
            goal_run_id,
        });
    }
}
