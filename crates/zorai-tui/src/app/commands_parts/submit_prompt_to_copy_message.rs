use super::*;
impl TuiModel {
    pub(crate) fn submit_prompt(&mut self, prompt: String) {
        if !self.connected {
            self.status_line = "Not connected to daemon".to_string();
            return;
        }
        if self.pending_participant_instruction_edit.is_some() {
            self.submit_participant_instruction_edit(prompt);
            return;
        }
        if self.should_queue_submitted_prompt() {
            self.queue_prompt(prompt);
            return;
        }

        self.cleanup_concierge_on_navigate();

        let (content_with_attachments, mut content_blocks) =
            self.consume_attachments_for_text_prompt(prompt.clone());
        if !content_blocks.is_empty() {
            content_blocks.insert(
                0,
                serde_json::json!({
                    "type": "text",
                    "text": content_with_attachments.clone(),
                }),
            );
        }
        let content_blocks_json = (!content_blocks.is_empty())
            .then(|| serde_json::to_string(&content_blocks).ok())
            .flatten();
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let known_agent_aliases = self.known_agent_directive_aliases();
        if let Some(mut directive) = input_refs::parse_leading_agent_directive(
            &content_with_attachments,
            &known_agent_aliases,
        ) {
            if let Some(agent_id) = crate::agent_aliases::agent_id_for_alias(&directive.agent_alias)
            {
                directive.agent_alias = agent_id;
            }
            if matches!(
                directive.agent_alias.to_ascii_lowercase().as_str(),
                "swarozyc"
                    | "radogost"
                    | "domowoj"
                    | "swietowit"
                    | "perun"
                    | "mokosh"
                    | "dazhbog"
                    | "rod"
            ) && !self.builtin_persona_configured(&directive.agent_alias)
            {
                self.open_builtin_persona_prompt_setup_flow(
                    &directive.agent_alias,
                    content_with_attachments.clone(),
                );
                return;
            }
            let directive_content =
                input_refs::append_referenced_files_footer(&directive.body, &cwd);
            match directive.kind {
                input_refs::LeadingAgentDirectiveKind::InternalDelegate => {
                    if let Some(thread_id) = self.chat.active_thread_id().map(String::from) {
                        if self.restore_prompt_and_show_budget_exceeded_notice(&thread_id, &prompt)
                        {
                            return;
                        }
                    }
                    self.send_daemon_command(DaemonCommand::InternalDelegate {
                        thread_id: self.chat.active_thread_id().map(String::from),
                        target_agent_id: directive.agent_alias.clone(),
                        content: directive_content,
                        session_id: None,
                    });
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.input.set_mode(input::InputMode::Insert);
                    self.status_line = format!("Delegated internally to {}", directive.agent_alias);
                    self.clear_active_thread_activity();
                    self.error_active = false;
                    return;
                }
                input_refs::LeadingAgentDirectiveKind::ParticipantUpsert => {
                    let participant_name = crate::agent_aliases::display_agent_name(
                        &self.participant_display_name(&directive.agent_alias),
                    );
                    let Some(thread_id) = self.chat.active_thread_id().map(String::from) else {
                        self.status_line =
                            "Participant commands require an active thread".to_string();
                        self.show_input_notice(
                            format!(
                                "Open a thread before adding {participant_name} as a participant"
                            ),
                            InputNoticeKind::Warning,
                            120,
                            false,
                        );
                        return;
                    };
                    if self.restore_prompt_and_show_budget_exceeded_notice(&thread_id, &prompt) {
                        return;
                    }
                    self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
                        thread_id,
                        target_agent_id: directive.agent_alias.clone(),
                        action: "upsert".to_string(),
                        instruction: Some(directive_content),
                        session_id: None,
                    });
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.input.set_mode(input::InputMode::Insert);
                    self.status_line = format!("Participant {} updated", directive.agent_alias);
                    self.show_input_notice(
                        format!("Participant {participant_name} updated for this thread"),
                        InputNoticeKind::Success,
                        120,
                        false,
                    );
                    self.clear_active_thread_activity();
                    self.error_active = false;
                    return;
                }
                input_refs::LeadingAgentDirectiveKind::ParticipantDeactivate => {
                    let participant_name = crate::agent_aliases::display_agent_name(
                        &self.participant_display_name(&directive.agent_alias),
                    );
                    let Some(thread_id) = self.chat.active_thread_id().map(String::from) else {
                        self.status_line =
                            "Participant commands require an active thread".to_string();
                        self.show_input_notice(
                            format!(
                                "Open a thread before removing {participant_name} as a participant"
                            ),
                            InputNoticeKind::Warning,
                            120,
                            false,
                        );
                        return;
                    };
                    if self.restore_prompt_and_show_budget_exceeded_notice(&thread_id, &prompt) {
                        return;
                    }
                    let refresh_thread_id = thread_id.clone();
                    self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
                        thread_id,
                        target_agent_id: directive.agent_alias.clone(),
                        action: "deactivate".to_string(),
                        instruction: None,
                        session_id: None,
                    });
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.input.set_mode(input::InputMode::Insert);
                    self.status_line = format!("Stop request sent for {}", directive.agent_alias);
                    self.show_input_notice(
                        format!(
                            "Stop request sent for {participant_name}; refreshing thread to confirm"
                        ),
                        InputNoticeKind::Success,
                        120,
                        false,
                    );
                    self.clear_active_thread_activity();
                    self.error_active = false;
                    self.request_latest_thread_page(refresh_thread_id, false);
                    return;
                }
            }
        }

        let final_content =
            input_refs::append_referenced_files_footer(&content_with_attachments, &cwd);

        let goal_target = self.current_goal_target_for_mission_control();
        let goal_thread_target = self.goal_prompt_thread_target();
        if goal_target.is_some() && goal_thread_target.is_none() {
            self.input.set_text(&prompt);
            self.status_line =
                "Goal input accepts only slash commands until an active goal thread is available"
                    .to_string();
            self.show_input_notice(
                "Goal input needs an active step thread before it can send a prompt".to_string(),
                InputNoticeKind::Warning,
                120,
                false,
            );
            return;
        }

        let thread_id = goal_thread_target
            .as_ref()
            .map(|(_, thread_id)| thread_id.clone())
            .or_else(|| self.chat.active_thread_id().map(String::from));
        if let Some(thread_id) = thread_id.as_deref() {
            if self.restore_prompt_and_show_budget_exceeded_notice(thread_id, &prompt) {
                return;
            }
        }
        let target_agent_id = if thread_id.is_none() {
            self.pending_new_thread_target_agent.clone()
        } else {
            None
        };
        let local_target_agent_name = target_agent_id
            .as_deref()
            .map(|agent_id| self.participant_display_name(agent_id));
        if thread_id.as_deref() == self.cancelled_thread_id.as_deref() {
            self.cancelled_thread_id = None;
        }
        if let Some((target, thread_id)) = &goal_thread_target {
            self.set_mission_control_return_targets(Some(target.clone()), None);
            if self.chat.active_thread_id() != Some(thread_id.as_str()) {
                self.chat
                    .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
            }
        }
        if thread_id.is_none() {
            let local_thread_id = format!("local-{}", self.tick_counter);
            let local_title = if prompt.chars().count() > 40 {
                format!("{}...", prompt.chars().take(40).collect::<String>())
            } else {
                prompt.clone()
            };
            self.chat.reduce(chat::ChatAction::ThreadCreated {
                thread_id: local_thread_id.clone(),
                title: local_title.clone(),
            });
            if let Some(agent_name) = local_target_agent_name {
                self.chat
                    .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
                        id: local_thread_id,
                        agent_name: Some(agent_name),
                        title: local_title,
                        ..Default::default()
                    }));
            }
            if let Some(profile) = self.pending_new_thread_execution_profile.take() {
                if let Some(thread) = self.chat.active_thread_mut() {
                    thread.profile_provider = Some(profile.provider_id.clone());
                    thread.profile_model = Some(profile.model.clone());
                    thread.runtime_provider = Some(profile.provider_id.clone());
                    thread.runtime_model = Some(profile.model.clone());
                }
                push_active_thread_execution_profile_to_daemon(
                    self,
                    &profile.provider_id,
                    &profile.model,
                    profile.reasoning_effort.as_deref(),
                );
            }
        }

        let optimistic_thread_id = thread_id
            .clone()
            .or_else(|| self.chat.active_thread_id().map(String::from));

        if let Some(thread_id) = optimistic_thread_id.as_ref() {
            let active_thread_id = thread_id.clone();
            self.reduce_chat_for_thread(
                Some(active_thread_id.as_str()),
                chat::ChatAction::AppendMessage {
                    thread_id: active_thread_id.clone(),
                    message: chat::AgentMessage {
                        role: chat::MessageRole::User,
                        content: final_content.clone(),
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0),
                        ..Default::default()
                    },
                },
            );
        }

        self.send_daemon_command(DaemonCommand::SendMessage {
            thread_id: thread_id.clone(),
            content: final_content,
            content_blocks_json,
            session_id: None,
            target_agent_id,
        });

        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
        self.input.set_mode(input::InputMode::Insert);
        self.status_line = "Prompt sent".to_string();
        let activity_thread_id = optimistic_thread_id;
        if let Some(thread_id) = activity_thread_id.as_ref() {
            self.mark_pending_prompt_response_thread(thread_id.clone());
        }
        self.set_agent_activity_for(activity_thread_id, "thinking");
        self.error_active = false;
    }

    pub(crate) fn focus_next(&mut self) {
        if matches!(self.main_pane_view, MainPaneView::Collaboration) {
            match self.focus {
                FocusArea::Chat => match self.collaboration.focus() {
                    CollaborationPaneFocus::Navigator => self.collaboration.reduce(
                        CollaborationAction::SetFocus(CollaborationPaneFocus::Detail),
                    ),
                    CollaborationPaneFocus::Detail => self.focus = FocusArea::Input,
                },
                FocusArea::Input => {
                    self.focus = FocusArea::Chat;
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Navigator,
                    ));
                }
                FocusArea::Sidebar => self.focus = FocusArea::Input,
            }
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        if self.focus_next_goal_composer_section() {
            return;
        }

        if self.focus_next_goal_workspace_pane() {
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        self.focus = if self.sidebar_visible() {
            match self.focus {
                FocusArea::Chat => FocusArea::Sidebar,
                FocusArea::Sidebar => FocusArea::Input,
                FocusArea::Input => FocusArea::Chat,
            }
        } else {
            match self.focus {
                FocusArea::Chat | FocusArea::Sidebar => FocusArea::Input,
                FocusArea::Input => FocusArea::Chat,
            }
        };
        self.input.set_mode(input::InputMode::Insert);
    }

    pub(crate) fn focus_prev(&mut self) {
        if matches!(self.main_pane_view, MainPaneView::Collaboration) {
            match self.focus {
                FocusArea::Input => {
                    self.focus = FocusArea::Chat;
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Detail,
                    ));
                }
                FocusArea::Chat => match self.collaboration.focus() {
                    CollaborationPaneFocus::Detail => self.collaboration.reduce(
                        CollaborationAction::SetFocus(CollaborationPaneFocus::Navigator),
                    ),
                    CollaborationPaneFocus::Navigator => self.focus = FocusArea::Input,
                },
                FocusArea::Sidebar => {
                    self.focus = FocusArea::Chat;
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Navigator,
                    ));
                }
            }
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        if self.focus_prev_goal_composer_section() {
            return;
        }

        if self.focus == FocusArea::Input
            && matches!(
                self.main_pane_view,
                MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
            )
        {
            self.focus = FocusArea::Chat;
            self.goal_workspace
                .set_focused_pane(crate::state::goal_workspace::GoalWorkspacePane::CommandBar);
            self.input.set_mode(input::InputMode::Insert);
            return;
        }
        if self.focus_prev_goal_workspace_pane() {
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        self.focus = if self.sidebar_visible() {
            match self.focus {
                FocusArea::Chat => FocusArea::Input,
                FocusArea::Sidebar => FocusArea::Chat,
                FocusArea::Input => FocusArea::Sidebar,
            }
        } else {
            match self.focus {
                FocusArea::Chat | FocusArea::Sidebar => FocusArea::Input,
                FocusArea::Input => FocusArea::Chat,
            }
        };
        self.input.set_mode(input::InputMode::Insert);
    }

    pub(crate) fn handle_sidebar_enter(&mut self) {
        if self.sidebar_uses_goal_sidebar() {
            let _ = self.handle_goal_sidebar_enter();
            return;
        }

        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };

        if self.should_toggle_work_context_from_sidebar(&thread_id) {
            self.set_main_pane_conversation(FocusArea::Sidebar);
            self.status_line = "Closed preview".to_string();
            return;
        }

        match self.sidebar.active_tab() {
            sidebar::SidebarTab::Files => {
                let Some(path) = self.selected_sidebar_file_path() else {
                    return;
                };
                let status_line = path.clone();
                self.open_work_context_for_thread(
                    thread_id.clone(),
                    Some(path),
                    Some(thread_id),
                    self.current_goal_return_target(),
                    status_line,
                );
            }
            sidebar::SidebarTab::Todos => {
                self.open_work_context_for_thread(
                    thread_id.clone(),
                    None,
                    Some(thread_id),
                    self.current_goal_return_target(),
                    "Todo details".to_string(),
                );
            }
            sidebar::SidebarTab::Spawned => {
                self.open_selected_spawned_thread();
            }
            sidebar::SidebarTab::Pinned => {
                let Some(pinned_message) = self.selected_sidebar_pinned_message() else {
                    return;
                };
                if let Some(message_index) = self
                    .chat
                    .resolve_active_pinned_message_to_loaded_index(&pinned_message)
                {
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.chat.select_message(Some(message_index));
                    self.status_line = "Pinned message".to_string();
                    return;
                }

                let Some(thread) = self.chat.active_thread() else {
                    return;
                };
                let total_messages = thread.total_message_count.max(thread.loaded_message_end);
                let page_size = self.chat_history_page_size().max(1);
                let end = pinned_message
                    .absolute_index
                    .saturating_add(1)
                    .max(page_size)
                    .min(total_messages);
                let start = end.saturating_sub(page_size);
                let limit = end.saturating_sub(start).max(1);
                let offset = total_messages.saturating_sub(end);
                self.pending_pinned_jump = Some(PendingPinnedJump {
                    thread_id: thread_id.clone(),
                    message_id: pinned_message.message_id.clone(),
                    absolute_index: pinned_message.absolute_index,
                });
                self.request_thread_page(thread_id, limit, offset, false);
                self.status_line = "Loading pinned message".to_string();
            }
        }
    }

    pub(crate) fn submit_selected_collaboration_vote(&mut self) {
        if let (Some(session), Some(disagreement), Some(position)) = (
            self.collaboration.selected_session(),
            self.collaboration.selected_disagreement(),
            self.collaboration.selected_position(),
        ) {
            if let Some(parent_task_id) = session.parent_task_id.clone() {
                self.send_daemon_command(DaemonCommand::VoteOnCollaborationDisagreement {
                    parent_task_id,
                    disagreement_id: disagreement.id.clone(),
                    task_id: "operator".to_string(),
                    position: position.to_string(),
                    confidence: Some(1.0),
                });
                self.status_line = format!("Casting vote: {position}");
            }
        }
    }

    pub(crate) fn copy_message(&mut self, index: usize) {
        let Some(thread) = self.chat.active_thread() else {
            return;
        };
        let Some(message) = thread.messages.get(index) else {
            return;
        };
        let mut text = String::new();
        if let Some(reasoning) = message
            .reasoning
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            text.push_str("Reasoning:\n");
            text.push_str(reasoning);
            if !message.content.is_empty() {
                text.push_str("\n\n-------\n\n");
            }
        }
        if !message.content.is_empty() {
            if !text.is_empty() {
                text.push_str("Content:\n");
            }
            text.push_str(&message.content);
        }
        if text.trim().is_empty() {
            return;
        }
        conversion::copy_to_clipboard(&text);
        self.chat
            .mark_message_copied(index, self.tick_counter.saturating_add(100));
        self.status_line = "Copied to clipboard".to_string();
    }
}
