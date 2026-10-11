use super::*;
use crate::widgets;
impl TuiModel {
    pub(crate) fn activate_workspace_toolbar_action(
        &mut self,
        action: widgets::workspace_board::WorkspaceBoardToolbarAction,
    ) {
        match action {
            widgets::workspace_board::WorkspaceBoardToolbarAction::NewTask => {
                self.open_workspace_create_modal(zorai_protocol::WorkspaceTaskType::Thread);
            }
            widgets::workspace_board::WorkspaceBoardToolbarAction::ToggleOperator => {
                let operator =
                    if self.workspace.operator() == zorai_protocol::WorkspaceOperator::User {
                        zorai_protocol::WorkspaceOperator::Svarog
                    } else {
                        zorai_protocol::WorkspaceOperator::User
                    };
                self.switch_workspace_operator_from_ui(operator.clone());
                self.status_line = match operator {
                    zorai_protocol::WorkspaceOperator::Svarog => {
                        "Switching workspace operator to svarog..."
                    }
                    zorai_protocol::WorkspaceOperator::User => {
                        "Switching workspace operator to user..."
                    }
                }
                .to_string();
            }
        }
    }

    pub(crate) fn activate_workspace_task_action(
        &mut self,
        task_id: String,
        status: zorai_protocol::WorkspaceTaskStatus,
        action: widgets::workspace_board::WorkspaceBoardAction,
    ) {
        if status == zorai_protocol::WorkspaceTaskStatus::InReview
            && matches!(
                action,
                widgets::workspace_board::WorkspaceBoardAction::Run
                    | widgets::workspace_board::WorkspaceBoardAction::Pause
                    | widgets::workspace_board::WorkspaceBoardAction::Stop
                    | widgets::workspace_board::WorkspaceBoardAction::OpenRuntime
            )
        {
            self.activate_workspace_review_task_action(&task_id, action);
            return;
        }

        match action {
            widgets::workspace_board::WorkspaceBoardAction::ToggleActions => {
                if !self.workspace_expanded_task_ids.insert(task_id.clone()) {
                    self.workspace_expanded_task_ids.remove(&task_id);
                    self.status_line = "Collapsed workspace task actions".to_string();
                } else {
                    self.status_line = "Expanded workspace task actions".to_string();
                }
                self.sync_workspace_board_scroll_to_selection();
            }
            widgets::workspace_board::WorkspaceBoardAction::RunBlocked => {
                self.status_line = "Assign workspace task before running".to_string();
            }
            widgets::workspace_board::WorkspaceBoardAction::Run => {
                self.workspace.start_task_run_locally(&task_id);
                self.send_daemon_command(DaemonCommand::RunWorkspaceTask(task_id));
                self.status_line = "Running workspace task...".to_string();
            }
            widgets::workspace_board::WorkspaceBoardAction::Pause => {
                self.send_daemon_command(DaemonCommand::PauseWorkspaceTask(task_id));
                self.status_line = "Pausing workspace task...".to_string();
            }
            widgets::workspace_board::WorkspaceBoardAction::Stop => {
                self.send_daemon_command(DaemonCommand::StopWorkspaceTask(task_id));
                self.status_line = "Stopping workspace task...".to_string();
            }
            widgets::workspace_board::WorkspaceBoardAction::MoveNext => {
                self.send_daemon_command(DaemonCommand::MoveWorkspaceTask(
                    zorai_protocol::WorkspaceTaskMove {
                        task_id,
                        status: next_workspace_status_for_commands(&status),
                        sort_order: Some(
                            self.workspace
                                .append_sort_order(&next_workspace_status_for_commands(&status)),
                        ),
                    },
                ));
                self.status_line = "Moving workspace task...".to_string();
            }
            widgets::workspace_board::WorkspaceBoardAction::Review => {
                if crate::app::workspace_review_modal::workspace_review_action_opens_modal(&status)
                {
                    self.open_workspace_review_modal(task_id);
                } else {
                    self.send_daemon_command(DaemonCommand::MoveWorkspaceTask(
                        zorai_protocol::WorkspaceTaskMove {
                            task_id,
                            status: zorai_protocol::WorkspaceTaskStatus::InReview,
                            sort_order: None,
                        },
                    ));
                    self.status_line = "Sending workspace task to review...".to_string();
                }
            }
            widgets::workspace_board::WorkspaceBoardAction::Assign => {
                self.open_workspace_actor_picker(
                    task_id,
                    crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Assignee,
                );
            }
            widgets::workspace_board::WorkspaceBoardAction::Reviewer => {
                self.open_workspace_actor_picker(
                    task_id,
                    crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Reviewer,
                );
            }
            widgets::workspace_board::WorkspaceBoardAction::Details => {
                self.open_workspace_detail_modal(task_id);
            }
            widgets::workspace_board::WorkspaceBoardAction::OpenRuntime => {
                self.open_workspace_task_runtime(task_id);
            }
            widgets::workspace_board::WorkspaceBoardAction::History => {
                self.open_workspace_history_modal(task_id);
            }
            widgets::workspace_board::WorkspaceBoardAction::Edit => {
                self.open_workspace_edit_modal_by_id(task_id);
            }
            widgets::workspace_board::WorkspaceBoardAction::Delete => {
                self.send_daemon_command(DaemonCommand::DeleteWorkspaceTask(task_id));
                self.status_line = "Deleting workspace task...".to_string();
            }
        }
    }

    fn activate_workspace_review_task_action(
        &mut self,
        task_id: &str,
        action: widgets::workspace_board::WorkspaceBoardAction,
    ) {
        let Some(review_task_id) = self.workspace.review_task_id_for(task_id) else {
            self.refresh_workspace_board();
            self.status_line = "Review task is not loaded yet; refreshing workspace".to_string();
            return;
        };
        if action == widgets::workspace_board::WorkspaceBoardAction::Stop {
            self.send_daemon_command(DaemonCommand::CancelTask {
                task_id: review_task_id.clone(),
            });
            self.status_line = format!("Stopping review task {review_task_id}...");
            return;
        }
        if let Some(thread_id) = self
            .tasks
            .task_by_id(&review_task_id)
            .and_then(|task| task.thread_id.clone())
        {
            if thread_id.starts_with("dm:") {
                self.send_daemon_command(DaemonCommand::ListTasks);
                self.status_line = format!(
                    "Review task {review_task_id} points to an internal thread; refreshing tasks"
                );
                return;
            }
            self.open_thread_conversation(thread_id.clone());
            self.status_line = format!("Opened review task {review_task_id}");
        } else {
            self.send_daemon_command(DaemonCommand::ListTasks);
            self.status_line = format!("Review task {review_task_id} is queued; refreshing tasks");
        }
    }

    pub(crate) fn is_builtin_command(&self, command: &str) -> bool {
        matches!(
            command,
            "provider"
                | "model"
                | "image"
                | "tools"
                | "effort"
                | "context"
                | "thread"
                | "new"
                | "goal"
                | "tasks"
                | "conversation"
                | "chat"
                | "settings"
                | "view"
                | "status"
                | "migrate"
                | "statistics"
                | "stats"
                | "notifications"
                | "approvals"
                | "participants"
                | "handoff"
                | "gateway"
                | "compact"
                | "quit"
                | "prompt"
                | "new-goal"
                | "workspace"
                | "new-workspace"
                | "workspace-run"
                | "workspace-pause"
                | "workspace-stop"
                | "workspace-delete"
                | "workspace-move"
                | "workspace-assign"
                | "workspace-update"
                | "workspace-reviewer"
                | "workspace-review"
                | "attach"
                | "plugins"
                | "plugins install"
                | "skills install"
                | "guidelines install"
                | "help"
                | "explain"
                | "diverge"
        )
    }

    pub(crate) fn execute_command(&mut self, command: &str) {
        tracing::info!("execute_command: {:?}", command);
        match command {
            "provider" => {
                if self.open_active_thread_target_provider_picker() {
                    return;
                }
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
                self.sync_provider_picker_item_count();
            }
            "model" => {
                if self.open_active_thread_target_model_picker() {
                    return;
                }
                let target = self
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Model);
                self.open_provider_backed_model_picker(
                    target,
                    self.config.provider.clone(),
                    self.config.base_url.clone(),
                    self.config.api_key.clone(),
                    self.config.auth_source.clone(),
                );
            }
            "image" => {
                self.input.set_text("/image ");
                self.focus = FocusArea::Input;
                self.status_line = "Describe the image and press Enter".to_string();
            }
            "tools" => {
                self.open_settings_tab(SettingsTab::Tools);
            }
            "effort" => {
                if self.open_active_thread_target_effort_picker() {
                    return;
                }
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
                self.sync_effort_picker_cursor_to_current();
            }
            "context" => {
                self.open_active_thread_context_window_editor();
            }
            "thread" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
                if let Some(tab) = self.active_thread_owner_agent_id().and_then(|agent_id| {
                    widgets::thread_picker::resolve_thread_picker_tab(
                        &agent_id,
                        &self.chat,
                        &self.subagents,
                    )
                }) {
                    self.modal.set_thread_picker_tab(tab.clone());
                    self.sync_thread_picker_item_count();
                    self.queue_threads_for_picker_tab_refresh(&tab);
                } else {
                    self.sync_thread_picker_item_count();
                }
            }
            "goal" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
                self.sync_goal_picker_item_count();
            }
            "new" => {
                let target_agent_id = self
                    .active_thread_owner_agent_id()
                    .unwrap_or_else(|| zorai_protocol::AGENT_ID_SWAROG.to_string());
                self.start_new_thread_view_for_agent(Some(target_agent_id.as_str()));
            }
            "tasks" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
                self.sync_goal_picker_item_count();
            }
            "conversation" | "chat" => {
                self.main_pane_view = MainPaneView::Conversation;
            }
            "settings" => {
                self.open_settings_tab(SettingsTab::Auth);
            }
            "view" => {
                let next = match self.chat.transcript_mode() {
                    chat::TranscriptMode::Compact => chat::TranscriptMode::Tools,
                    chat::TranscriptMode::Tools => chat::TranscriptMode::Full,
                    chat::TranscriptMode::Full => chat::TranscriptMode::Compact,
                };
                self.chat.reduce(chat::ChatAction::SetTranscriptMode(next));
                self.status_line = format!("View: {:?}", next);
            }
            "status" => {
                self.open_status_modal_loading();
                self.send_daemon_command(DaemonCommand::RequestAgentStatus);
                self.status_line = "Requesting zorai status...".to_string();
            }
            "migrate" => {
                self.send_daemon_command(DaemonCommand::ExternalRuntimeMigrationStatus);
                self.status_line = "Checking migration sources...".to_string();
            }
            "statistics" | "stats" => {
                self.request_statistics_window(self.statistics_modal_window);
            }
            "notifications" => {
                self.toggle_notifications_modal();
                self.status_line = "Viewing notifications".to_string();
            }
            "approvals" => {
                self.toggle_approval_center();
                self.status_line = "Viewing approvals".to_string();
            }
            "participants" => {
                self.open_thread_participants_modal();
                self.status_line = "Viewing thread participants".to_string();
            }
            "handoff" => {
                self.open_thread_handoff_modal();
            }
            "gateway" => {
                self.open_gateway_link_modal();
            }
            "compact" => {
                let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
                    self.status_line = "Start or load thread first".to_string();
                    self.show_input_notice(
                        "Start or load thread first",
                        InputNoticeKind::Warning,
                        90,
                        false,
                    );
                    return;
                };
                self.set_agent_activity_for(Some(thread_id.clone()), "compacting");
                self.send_daemon_command(DaemonCommand::ForceCompact { thread_id });
                self.status_line = "Forcing compaction...".to_string();
            }
            "quit" => self.pending_quit = true,
            "prompt" => {
                self.request_prompt_inspection(None);
            }
            "new-goal" => {
                self.open_new_goal_view();
            }
            "workspace" => {
                self.open_workspace_picker();
            }
            "new-workspace" => {
                self.open_workspace_create_workspace_modal();
            }
            "workspace-run" => {
                self.input.set_text("/workspace-run <task_id>");
                self.focus = FocusArea::Input;
                self.status_line = "Usage: /workspace-run <task_id>".to_string();
            }
            "workspace-pause" => {
                self.input.set_text("/workspace-pause <task_id>");
                self.focus = FocusArea::Input;
                self.status_line = "Usage: /workspace-pause <task_id>".to_string();
            }
            "workspace-stop" => {
                self.input.set_text("/workspace-stop <task_id>");
                self.focus = FocusArea::Input;
                self.status_line = "Usage: /workspace-stop <task_id>".to_string();
            }
            "workspace-delete" => {
                self.input.set_text("/workspace-delete <task_id>");
                self.focus = FocusArea::Input;
                self.status_line = "Usage: /workspace-delete <task_id>".to_string();
            }
            "workspace-move" => {
                self.input
                    .set_text("/workspace-move <task_id> <todo|in-progress|in-review|done>");
                self.focus = FocusArea::Input;
                self.status_line =
                    "Usage: /workspace-move <task_id> <todo|in-progress|in-review|done>"
                        .to_string();
            }
            "workspace-assign" => {
                self.input
                    .set_text("/workspace-assign <task_id> <svarog|agent:id|subagent:id|none>");
                self.focus = FocusArea::Input;
                self.status_line =
                    "Usage: /workspace-assign <task_id> <svarog|agent:id|subagent:id|none>"
                        .to_string();
            }
            "workspace-update" => {
                self.input.set_text("/workspace-update <task_id> --title <title> --description <description> --priority low --assignee svarog --reviewer user --dod <definition>");
                self.focus = FocusArea::Input;
                self.status_line = "Usage: /workspace-update <task_id> [--title text] [--description text] [--priority high] [--assignee svarog] [--reviewer user|none] [--dod text|--clear-dod]".to_string();
            }
            "workspace-reviewer" => {
                self.input.set_text(
                    "/workspace-reviewer <task_id> <user|svarog|agent:id|subagent:id|none>",
                );
                self.focus = FocusArea::Input;
                self.status_line =
                    "Usage: /workspace-reviewer <task_id> <user|svarog|agent:id|subagent:id|none>"
                        .to_string();
            }
            "workspace-review" => {
                self.input
                    .set_text("/workspace-review <task_id> <pass|fail> [message]");
                self.focus = FocusArea::Input;
                self.status_line =
                    "Usage: /workspace-review <task_id> <pass|fail> [message]".to_string();
            }
            "attach" => {
                self.status_line =
                    "Usage: /attach <path>  — attach a file to the next message".to_string();
            }
            "plugins" => {
                self.open_settings_tab(SettingsTab::Plugins);
                self.status_line = "Opened plugin settings".to_string();
            }
            "plugins install" => {
                self.open_settings_tab(SettingsTab::Plugins);
                self.plugin_settings.install_mode = true;
                self.plugin_settings.install_source_buffer.clear();
                self.plugin_settings.install_source_cursor = 0;
                self.status_line = "Enter a plugin source to install".to_string();
            }
            "skills install" => {
                self.input.set_text("zorai skill import ");
                self.focus = FocusArea::Input;
                self.status_line = "Edit the skill source and run it in the terminal".to_string();
            }
            "guidelines install" => {
                self.input.set_text("zorai guideline install ");
                self.focus = FocusArea::Input;
                self.status_line =
                    "Edit the guideline source and run it in the terminal".to_string();
            }
            "help" => {
                self.help_modal_scroll = 0;
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::Help));
                self.modal.set_picker_item_count(100);
            }
            "explain" => {
                let action_id = self
                    .tasks
                    .goal_runs()
                    .iter()
                    .max_by_key(|run| run.updated_at)
                    .map(|run| run.id.clone());
                if let Some(action_id) = action_id {
                    self.send_daemon_command(DaemonCommand::ExplainAction {
                        action_id,
                        step_index: None,
                    });
                    self.status_line = "Requesting explainability report...".to_string();
                } else {
                    self.status_line = "No goal run available to explain".to_string();
                }
            }
            "diverge" => {
                if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                    self.input.set_text(&format!(
                        "/diverge-start {thread_id} Compare two implementation approaches for the current task"
                    ));
                    self.focus = FocusArea::Input;
                    self.status_line = "Edit /diverge-start prompt and press Enter".to_string();
                } else {
                    self.status_line = "Open a thread first, then run /diverge".to_string();
                }
            }
            _ => {
                self.input.set_text(&format!("/{command} "));
                self.focus = FocusArea::Chat;
            }
        }
    }

    pub(crate) fn submit_image_prompt(&mut self, prompt: String) {
        if !self.connected {
            self.status_line = "Not connected to daemon".to_string();
            return;
        }

        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            self.execute_command("image");
            return;
        }

        self.cleanup_concierge_on_navigate();
        self.attachments.clear();

        let args_json = serde_json::json!({
            "thread_id": self.chat.active_thread_id().map(str::to_string),
            "prompt": trimmed,
        })
        .to_string();
        self.send_daemon_command(DaemonCommand::GenerateImage { args_json });

        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
        self.input.set_mode(input::InputMode::Insert);
        self.status_line = "Generating image...".to_string();
        self.error_active = false;
    }
}
