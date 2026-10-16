use super::*;
impl TuiModel {
    pub(crate) fn open_new_goal_view(&mut self) {
        let current_goal_target = self.current_goal_target_for_mission_control();
        self.set_mission_control_source_goal_target(current_goal_target.clone());
        self.clear_mission_control_return_context();
        self.cleanup_concierge_on_navigate();
        let fallback_profile = self.current_conversation_agent_profile();
        let fallback_main_assignment = task::GoalAgentAssignment {
            role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
            enabled: true,
            provider: fallback_profile.provider,
            model: fallback_profile.model,
            reasoning_effort: fallback_profile.reasoning_effort,
            inherit_from_main: false,
        };
        let preferred_goal_snapshot = current_goal_target
            .as_ref()
            .and_then(|target| target_goal_run_id(self, target))
            .and_then(|goal_run_id| self.tasks.goal_run_by_id(&goal_run_id).cloned())
            .and_then(|run| {
                if !run.launch_assignment_snapshot.is_empty() {
                    Some(run.launch_assignment_snapshot)
                } else if !run.runtime_assignment_list.is_empty() {
                    Some(run.runtime_assignment_list)
                } else {
                    None
                }
            });
        let latest_goal_snapshot = self
            .tasks
            .goal_runs()
            .iter()
            .max_by_key(|run| run.updated_at)
            .and_then(|run| {
                if !run.launch_assignment_snapshot.is_empty() {
                    Some(run.launch_assignment_snapshot.clone())
                } else if !run.runtime_assignment_list.is_empty() {
                    Some(run.runtime_assignment_list.clone())
                } else {
                    None
                }
            });
        self.goal_mission_control = if let Some(snapshot) = self
            .goal_composer_saved_defaults
            .clone()
            .filter(|assignments| !assignments.is_empty())
        {
            let mut state = goal_mission_control::GoalMissionControlState::from_goal_snapshot(
                snapshot,
                fallback_main_assignment,
                "Saved default",
            );
            state.set_save_as_default_pending(true);
            state
        } else {
            match preferred_goal_snapshot.or(latest_goal_snapshot) {
                Some(snapshot) => {
                    goal_mission_control::GoalMissionControlState::from_goal_snapshot(
                        snapshot,
                        fallback_main_assignment,
                        "Previous goal snapshot",
                    )
                }
                None => goal_mission_control::GoalMissionControlState::from_main_assignment(
                    fallback_main_assignment.clone(),
                    vec![fallback_main_assignment],
                    "Main agent inheritance",
                ),
            }
        };
        self.goal_mission_control.set_prompt_text(String::new());
        self.goal_mission_control
            .set_focused_section(goal_mission_control::GoalMissionControlSection::Prompt);
        self.main_pane_view = MainPaneView::GoalComposer;
        self.task_view_scroll = 0;
        self.focus = FocusArea::Input;
        self.set_input_text("");
        self.attachments.clear();
        self.status_line = "Mission Control preflight is ready".to_string();
    }

    pub(crate) fn open_mission_control_goal_thread(&mut self) -> bool {
        let Some((thread_id, used_root_fallback)) = self.mission_control_thread_target() else {
            self.status_line = if self.mission_control_source_goal_target().is_some() {
                "Mission Control source goal has no active or root thread".to_string()
            } else {
                "Mission Control has no source goal thread to open".to_string()
            };
            return false;
        };

        self.open_thread_conversation(thread_id.clone());
        self.status_line = if used_root_fallback {
            "Opened root goal thread as fallback because no active goal thread was available"
                .to_string()
        } else {
            format!("Opened active goal thread {thread_id}")
        };
        true
    }

    pub(super) fn return_to_goal_from_mission_control(&mut self) -> bool {
        let Some(target) = self.mission_control_return_to_goal_target() else {
            return false;
        };

        self.clear_mission_control_return_context();
        self.open_sidebar_target(target);
        self.focus = FocusArea::Chat;
        self.status_line = "Returned to goal".to_string();
        true
    }

    pub(crate) fn return_from_mission_control_navigation(&mut self) -> bool {
        if let Some(thread_id) = self.mission_control_return_to_thread_id() {
            self.set_mission_control_return_to_thread_id(
                self.chat
                    .thread_history_stack()
                    .last()
                    .filter(|parent| **parent != thread_id)
                    .cloned(),
            );
            self.cleanup_concierge_on_navigate();
            self.clear_chat_drag_selection();
            self.clear_work_context_drag_selection();
            self.clear_task_view_drag_selection();
            self.pending_new_thread_target_agent = None;
            self.pending_new_thread_execution_profile = None;
            self.chat
                .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
            self.request_latest_thread_page(thread_id.clone(), true);
            self.main_pane_view = MainPaneView::Conversation;
            self.task_view_scroll = 0;
            self.focus = FocusArea::Chat;
            self.status_line = format!("Returned to {thread_id}");
            return true;
        }

        if self.mission_control_return_to_workspace() {
            self.clear_mission_control_return_context();
            self.cleanup_concierge_on_navigate();
            self.clear_chat_drag_selection();
            self.clear_work_context_drag_selection();
            self.clear_task_view_drag_selection();
            self.main_pane_view = MainPaneView::Workspace;
            self.task_view_scroll = 0;
            self.focus = FocusArea::Chat;
            self.status_line = "Returned to workspace".to_string();
            return true;
        }

        self.return_to_goal_from_mission_control()
    }

    #[cfg(test)]
    pub(crate) fn start_goal_run_from_prompt(&mut self, goal: String) {
        self.goal_mission_control.set_prompt_text(goal);
        self.start_goal_run_from_mission_control();
    }

    pub(crate) fn consume_attachments_for_text_prompt(
        &mut self,
        prompt: String,
    ) -> (String, Vec<serde_json::Value>) {
        let drained_attachments = self.attachments.drain(..).collect::<Vec<_>>();
        let mut content_blocks = Vec::new();
        let content_with_attachments = if drained_attachments.is_empty() {
            prompt
        } else {
            let mut parts: Vec<String> = Vec::new();
            for att in drained_attachments {
                match att.payload {
                    AttachmentPayload::Text(content) => parts.push(format!(
                        "<attached_file name=\"{}\">\n{}\n</attached_file>",
                        att.filename, content
                    )),
                    AttachmentPayload::ContentBlock(block) => content_blocks.push(block),
                }
            }
            parts.push(prompt);
            parts.join("\n\n")
        };
        (content_with_attachments, content_blocks)
    }

    pub(crate) fn start_goal_run_from_mission_control(&mut self) {
        if !self.connected {
            self.status_line = "Not connected to daemon".to_string();
            return;
        }
        let raw_goal = self.goal_mission_control.prompt_text().trim().to_string();
        if raw_goal.is_empty() {
            self.status_line = "Enter a goal before launching".to_string();
            return;
        }
        self.cleanup_concierge_on_navigate();
        let (goal_with_attachments, _content_blocks) =
            self.consume_attachments_for_text_prompt(raw_goal);
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let goal = input_refs::append_referenced_files_footer(&goal_with_attachments, &cwd);
        let launch_assignments = if self
            .goal_mission_control
            .display_role_assignments()
            .is_empty()
        {
            let fallback_profile = self.current_conversation_agent_profile();
            vec![task::GoalAgentAssignment {
                role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: fallback_profile.provider,
                model: fallback_profile.model,
                reasoning_effort: fallback_profile.reasoning_effort,
                inherit_from_main: false,
            }]
        } else {
            self.goal_mission_control
                .display_role_assignments()
                .to_vec()
        };
        self.send_daemon_command(DaemonCommand::StartGoalRun {
            goal,
            thread_id: None,
            session_id: None,
            launch_assignments,
        });
        self.status_line = "Starting goal run...".to_string();
    }

    pub(crate) fn sync_goal_mission_control_prompt_from_input(&mut self) {
        if matches!(self.main_pane_view, MainPaneView::GoalComposer)
            && self.focus == FocusArea::Input
        {
            self.goal_mission_control
                .set_prompt_text(self.input.buffer().to_string());
        }
    }

    pub(super) fn open_workspace_view(&mut self) {
        let concierge_active = self.concierge.loading
            || self.concierge.has_active_welcome()
            || (self.chat.active_thread_id() == Some("concierge")
                && !self.chat.active_actions().is_empty());
        if concierge_active {
            self.cleanup_concierge_on_navigate();
        }
        self.main_pane_view = MainPaneView::Workspace;
        self.focus = FocusArea::Chat;
        self.refresh_workspace_board();
        self.status_line = "Loading workspace...".to_string();
    }

    pub(crate) fn refresh_workspace_board(&mut self) {
        let workspace_id = self.workspace.workspace_id().to_string();
        let include_deleted = self.workspace.filter().include_deleted;
        self.send_daemon_command(DaemonCommand::GetWorkspaceSettings {
            workspace_id: workspace_id.clone(),
        });
        self.send_daemon_command(DaemonCommand::ListWorkspaceTasks {
            workspace_id: workspace_id.clone(),
            include_deleted,
        });
        self.send_daemon_command(DaemonCommand::ListWorkspaceNotices {
            workspace_id,
            task_id: None,
        });
    }

    pub(crate) fn open_workspace_picker(&mut self) {
        self.send_daemon_command(DaemonCommand::ListWorkspaceSettings);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::WorkspacePicker));
        self.sync_workspace_picker_item_count();
        self.status_line = "Loading workspaces...".to_string();
    }

    pub(crate) fn sync_workspace_picker_item_count(&mut self) {
        let count = self
            .workspace
            .workspace_picker_items(self.modal.command_query())
            .len()
            .max(1);
        self.modal.set_picker_item_count(count);
    }

    pub(crate) fn submit_workspace_picker(&mut self) {
        let cursor = self.modal.picker_cursor();
        let workspace_id = self
            .workspace
            .selected_workspace_id(cursor, self.modal.command_query())
            .unwrap_or_else(|| self.workspace.workspace_id().to_string());
        self.close_top_modal();
        self.switch_workspace_from_ui(&workspace_id);
    }

    pub(crate) fn switch_workspace_operator_from_ui(
        &mut self,
        operator: zorai_protocol::WorkspaceOperator,
    ) {
        let workspace_id = self.workspace.workspace_id().to_string();
        self.workspace.set_operator(operator.clone());
        self.send_daemon_command(DaemonCommand::SetWorkspaceOperator {
            workspace_id: workspace_id.clone(),
            operator,
        });
        self.send_daemon_command(DaemonCommand::ListWorkspaceTasks {
            workspace_id,
            include_deleted: self.workspace.filter().include_deleted,
        });
    }

    pub(super) fn switch_workspace_from_ui(&mut self, workspace_id: &str) {
        let workspace_id = workspace_id.trim();
        if workspace_id.is_empty() {
            self.status_line = "Usage: /workspace <workspace-id>".to_string();
            return;
        }
        self.workspace.switch_workspace(workspace_id);
        self.open_workspace_view();
        self.status_line = format!("Switched workspace to {workspace_id}");
    }

    pub(crate) fn handle_workspace_command(&mut self, args: &str) {
        let arg = args.trim();
        if matches!(arg, "auto" | "svarog" | "user") {
            let operator = if arg == "user" {
                zorai_protocol::WorkspaceOperator::User
            } else {
                zorai_protocol::WorkspaceOperator::Svarog
            };
            let status = format!("Switching workspace operator to {:?}", operator);
            self.switch_workspace_operator_from_ui(operator);
            self.main_pane_view = MainPaneView::Workspace;
            self.status_line = status;
            return;
        }
        if arg.eq_ignore_ascii_case("clear") || arg.eq_ignore_ascii_case("reset") {
            self.workspace.clear_filter();
            self.open_workspace_view();
            self.status_line = "Workspace filters cleared".to_string();
            return;
        }
        if !arg.is_empty() {
            if !arg.contains('=') {
                self.switch_workspace_from_ui(arg);
                return;
            }
            let mut filter = self.workspace.filter().clone();
            for token in arg.split_whitespace() {
                let Some((key, value)) = token.split_once('=') else {
                    self.status_line =
                        "Usage: /workspace [<workspace-id>|auto|user|clear|workspace=<id>|status=<status> priority=<priority> assignee=<actor> reviewer=<actor> deleted=<true|false>]".to_string();
                    return;
                };
                match key.trim().to_ascii_lowercase().as_str() {
                    "workspace" | "workspace_id" | "id" => {
                        self.switch_workspace_from_ui(value);
                        return;
                    }
                    "status" => {
                        let Some(status) = parse_workspace_status(value) else {
                            self.status_line = format!("Unknown workspace status: {value}");
                            return;
                        };
                        filter.status = Some(status);
                    }
                    "priority" => {
                        let Some(priority) = parse_workspace_priority(value) else {
                            self.status_line = format!("Unknown workspace priority: {value}");
                            return;
                        };
                        filter.priority = Some(priority);
                    }
                    "assignee" => {
                        let Some(actor) = parse_workspace_actor(value) else {
                            self.status_line = format!("Unknown workspace assignee: {value}");
                            return;
                        };
                        filter.assignee = Some(actor);
                    }
                    "reviewer" => {
                        let Some(actor) = parse_workspace_actor(value) else {
                            self.status_line = format!("Unknown workspace reviewer: {value}");
                            return;
                        };
                        filter.reviewer = Some(actor);
                    }
                    "deleted" | "include_deleted" => {
                        filter.include_deleted = matches!(
                            value.trim().to_ascii_lowercase().as_str(),
                            "1" | "true" | "yes" | "show"
                        );
                    }
                    _ => {
                        self.status_line = format!("Unknown workspace filter: {key}");
                        return;
                    }
                }
            }
            self.workspace.set_filter(filter);
            self.open_workspace_view();
            self.status_line = "Workspace filters applied".to_string();
            return;
        }
        self.open_workspace_picker();
    }

    pub(crate) fn create_workspace_from_args(&mut self, args: &str) {
        let args = args.trim();
        if args.is_empty() {
            self.open_workspace_create_workspace_modal();
            return;
        }

        let mut parts = args.split_whitespace();
        let workspace_id = parts.next().unwrap_or("").trim();
        let operator = match parts.next() {
            None => zorai_protocol::WorkspaceOperator::User,
            Some(raw) if raw.eq_ignore_ascii_case("user") => {
                zorai_protocol::WorkspaceOperator::User
            }
            Some(raw) if raw.eq_ignore_ascii_case("svarog") || raw.eq_ignore_ascii_case("auto") => {
                zorai_protocol::WorkspaceOperator::Svarog
            }
            Some(raw) => {
                self.status_line = format!("Unknown workspace operator: {raw}");
                return;
            }
        };
        if parts.next().is_some() {
            self.status_line = "Usage: /new-workspace [<workspace-id> [user|svarog]]".to_string();
            return;
        }
        self.open_workspace_create_workspace_modal_with_values(workspace_id.to_string(), operator);
    }

    pub(crate) fn resolve_workspace_task_id(&mut self, raw: &str) -> Option<String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        let mut matches = self
            .workspace
            .projection()
            .columns
            .iter()
            .flat_map(|column| column.tasks.iter())
            .filter(|task| task.id == raw || task.id.starts_with(raw))
            .map(|task| task.id.clone())
            .collect::<Vec<_>>();
        matches.sort();
        matches.dedup();
        match matches.len() {
            0 => Some(raw.to_string()),
            1 => matches.pop(),
            _ => {
                self.status_line = format!("Workspace task id prefix is ambiguous: {raw}");
                None
            }
        }
    }

    pub(crate) fn run_workspace_task_from_args(&mut self, args: &str) {
        let Some(task_id) = self.resolve_workspace_task_id(args) else {
            self.status_line = "Usage: /workspace-run <task_id>".to_string();
            return;
        };
        if self.workspace.task_run_blocked(&task_id) {
            self.status_line = "Assign workspace task before running".to_string();
            return;
        }
        self.workspace.start_task_run_locally(&task_id);
        self.send_daemon_command(DaemonCommand::RunWorkspaceTask(task_id));
        self.main_pane_view = MainPaneView::Workspace;
        self.status_line = "Running workspace task...".to_string();
    }

    pub(crate) fn pause_workspace_task_from_args(&mut self, args: &str) {
        let Some(task_id) = self.resolve_workspace_task_id(args) else {
            self.status_line = "Usage: /workspace-pause <task_id>".to_string();
            return;
        };
        self.send_daemon_command(DaemonCommand::PauseWorkspaceTask(task_id));
        self.status_line = "Pausing workspace task...".to_string();
    }

    pub(crate) fn stop_workspace_task_from_args(&mut self, args: &str) {
        let Some(task_id) = self.resolve_workspace_task_id(args) else {
            self.status_line = "Usage: /workspace-stop <task_id>".to_string();
            return;
        };
        self.send_daemon_command(DaemonCommand::StopWorkspaceTask(task_id));
        self.status_line = "Stopping workspace task...".to_string();
    }

    pub(crate) fn delete_workspace_task_from_args(&mut self, args: &str) {
        let Some(task_id) = self.resolve_workspace_task_id(args) else {
            self.status_line = "Usage: /workspace-delete <task_id>".to_string();
            return;
        };
        self.send_daemon_command(DaemonCommand::DeleteWorkspaceTask(task_id));
        self.status_line = "Deleting workspace task...".to_string();
    }
}
