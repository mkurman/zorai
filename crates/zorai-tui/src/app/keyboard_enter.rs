use super::*;

impl TuiModel {
    fn parse_migration_config_path(parts: &[&str]) -> Option<String> {
        parts
            .windows(2)
            .find_map(|window| {
                matches!(window[0], "--config" | "--config-path").then(|| window[1].to_string())
            })
            .or_else(|| {
                parts.iter().find_map(|part| {
                    part.strip_prefix("--config=")
                        .or_else(|| part.strip_prefix("--config-path="))
                        .map(str::to_string)
                })
            })
    }

    fn parse_migration_limit(parts: &[&str]) -> Option<usize> {
        parts
            .windows(2)
            .find_map(|window| {
                (window[0] == "--limit")
                    .then(|| window[1].parse::<usize>().ok())
                    .flatten()
            })
            .or_else(|| {
                parts.iter().find_map(|part| {
                    part.strip_prefix("--limit=")
                        .and_then(|value| value.parse::<usize>().ok())
                })
            })
    }

    fn parse_migration_conflict_policy(parts: &[&str]) -> String {
        parts
            .windows(2)
            .find_map(|window| (window[0] == "--conflict-policy").then(|| window[1].to_string()))
            .or_else(|| {
                parts
                    .iter()
                    .find_map(|part| part.strip_prefix("--conflict-policy=").map(str::to_string))
            })
            .unwrap_or_else(|| "stage_for_review".to_string())
    }

    fn migration_runtime_arg(parts: &[&str]) -> Option<String> {
        parts.iter().copied().find_map(|part| {
            (!part.starts_with("--")
                && matches!(part.to_ascii_lowercase().as_str(), "hermes" | "openclaw"))
            .then(|| part.to_ascii_lowercase())
        })
    }

    fn execute_migration_slash_command(&mut self, args: &str) {
        let parts = args.split_whitespace().collect::<Vec<_>>();
        let action = parts.first().copied().unwrap_or("status");
        match action {
            "" | "status" => {
                self.send_daemon_command(DaemonCommand::ExternalRuntimeMigrationStatus);
                self.status_line = "Checking migration sources...".to_string();
            }
            "preview" => {
                let runtime = Self::migration_runtime_arg(&parts[1..]);
                if let Some(runtime) = runtime {
                    self.send_daemon_command(DaemonCommand::ExternalRuntimeMigrationPreview {
                        runtime: runtime.clone(),
                        config_path: Self::parse_migration_config_path(&parts[1..]),
                    });
                    self.status_line = format!("Previewing {runtime} migration...");
                } else {
                    self.status_line =
                        "Usage: /migrate preview <hermes|openclaw> [--config <path>]".to_string();
                }
            }
            "apply" | "import" => {
                let runtime = Self::migration_runtime_arg(&parts[1..]);
                if let Some(runtime) = runtime {
                    self.send_daemon_command(DaemonCommand::ExternalRuntimeMigrationApply {
                        runtime: runtime.clone(),
                        config_path: Self::parse_migration_config_path(&parts[1..]),
                        conflict_policy: Self::parse_migration_conflict_policy(&parts[1..]),
                    });
                    self.status_line = format!("Importing {runtime} migration...");
                } else {
                    self.status_line =
                        "Usage: /migrate apply <hermes|openclaw> [--config <path>]".to_string();
                }
            }
            "report" => {
                self.send_daemon_command(DaemonCommand::ExternalRuntimeMigrationReport {
                    runtime: Self::migration_runtime_arg(&parts[1..]),
                    limit: Self::parse_migration_limit(&parts[1..]),
                });
                self.status_line = "Loading migration report...".to_string();
            }
            "shadow-run" | "shadow" => {
                let runtime = Self::migration_runtime_arg(&parts[1..]);
                if let Some(runtime) = runtime {
                    self.send_daemon_command(DaemonCommand::ExternalRuntimeMigrationShadowRun {
                        runtime: runtime.clone(),
                    });
                    self.status_line = format!("Previewing {runtime} shadow run...");
                } else {
                    self.status_line = "Usage: /migrate shadow-run <hermes|openclaw>".to_string();
                }
            }
            _ => {
                self.status_line =
                    "Usage: /migrate status|preview|apply|report|shadow-run".to_string();
            }
        }
    }

    pub(in crate::app) fn execute_slash_command_line(&mut self, prompt: &str) -> bool {
        let trimmed = prompt.trim_start_matches('/');
        let cmd = trimmed.split_whitespace().next().unwrap_or("");
        let args = trimmed[cmd.len()..].trim();
        if cmd == "handoff" {
            self.execute_thread_handoff_slash(args);
            return true;
        }
        if cmd == "gateway" {
            self.open_gateway_link_modal();
            return true;
        }
        if cmd == "migrate" {
            self.execute_migration_slash_command(args);
            return true;
        }
        if cmd == "apikey" && !args.is_empty() {
            self.config.api_key = args.to_string();
            self.status_line = format!("API key set ({}...)", &args[..args.len().min(8)]);
            if let Ok(value_json) =
                serde_json::to_string(&serde_json::Value::String(args.to_string()))
            {
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/api_key".to_string(),
                    value_json: value_json.clone(),
                });
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: format!("/providers/{}/api_key", self.config.provider),
                    value_json: value_json.clone(),
                });
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: format!("/{}/api_key", self.config.provider),
                    value_json,
                });
            }
            return true;
        }
        if cmd == "attach" && !args.is_empty() {
            self.attach_file(args);
            return true;
        }
        if cmd == "image" {
            self.submit_image_prompt(args.to_string());
            return true;
        }
        if cmd == "prompt" {
            let requested_agent = if args.trim().is_empty() {
                None
            } else {
                Some(args.trim().to_string())
            };
            self.request_prompt_inspection(requested_agent);
            return true;
        }
        if cmd == "explain" {
            self.execute_command("explain");
            return true;
        }
        if cmd == "diverge" {
            self.execute_command("diverge");
            return true;
        }
        if cmd == "diverge-start" {
            let mut parts = args.splitn(2, char::is_whitespace);
            let thread_id = parts.next().unwrap_or("").trim();
            let problem_statement = parts.next().unwrap_or("").trim();
            if thread_id.is_empty() || problem_statement.is_empty() {
                self.status_line = "Usage: /diverge-start <thread_id> <problem>".to_string();
            } else {
                self.send_daemon_command(DaemonCommand::StartDivergentSession {
                    problem_statement: problem_statement.to_string(),
                    thread_id: thread_id.to_string(),
                    goal_run_id: None,
                });
                self.status_line = "Starting divergent session...".to_string();
            }
            return true;
        }
        if cmd == "diverge-get" {
            let session_id = args.trim();
            if session_id.is_empty() {
                self.status_line = "Usage: /diverge-get <session_id>".to_string();
            } else {
                self.send_daemon_command(DaemonCommand::GetDivergentSession {
                    session_id: session_id.to_string(),
                });
                self.status_line = "Fetching divergent session...".to_string();
            }
            return true;
        }
        if cmd == "workspace" {
            self.handle_workspace_command(args);
            return true;
        }
        if cmd == "new-workspace" {
            self.create_workspace_from_args(args);
            return true;
        }
        if cmd == "workspace-run" {
            self.run_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-pause" {
            self.pause_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-stop" {
            self.stop_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-delete" {
            self.delete_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-move" {
            self.move_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-assign" {
            self.assign_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-update" {
            self.update_workspace_task_from_args(args);
            return true;
        }
        if cmd == "workspace-reviewer" {
            self.set_workspace_reviewer_from_args(args);
            return true;
        }
        if cmd == "workspace-review" {
            self.review_workspace_task_from_args(args);
            return true;
        }
        if cmd == "plugins" {
            if args.trim() == "install" {
                self.execute_command("plugins install");
            } else {
                self.execute_command("plugins");
            }
            return true;
        }
        if cmd == "new" {
            let target_agent_id = if args.trim().is_empty() {
                Some(
                    self.active_thread_owner_agent_id()
                        .unwrap_or_else(|| zorai_protocol::AGENT_ID_SWAROG.to_string()),
                )
            } else {
                self.resolve_target_agent_id(args.trim())
            };
            match target_agent_id {
                Some(target_agent_id) => {
                    self.start_new_thread_view_for_agent(Some(target_agent_id.as_str()));
                    self.status_line = format!(
                        "New conversation for {}",
                        crate::agent_aliases::display_agent_name(
                            &self.participant_display_name(&target_agent_id)
                        )
                    );
                }
                None => {
                    self.status_line = format!("Unknown agent: {}", args.trim());
                }
            }
            return true;
        }
        if cmd == "thread" && !args.trim().is_empty() {
            let Some(tab) = widgets::thread_picker::resolve_thread_picker_tab(
                args.trim(),
                &self.chat,
                &self.subagents,
            ) else {
                self.status_line = format!("Unknown agent: {}", args.trim());
                return true;
            };
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
            self.modal.set_thread_picker_tab(tab.clone());
            self.sync_thread_picker_item_count();
            self.queue_threads_for_picker_tab_refresh(&tab);
            return true;
        }
        if cmd == "context" {
            if args.is_empty() {
                self.execute_command("context");
            } else if let Some(tokens) = Self::parse_context_window_tokens(args) {
                self.apply_active_thread_context_window(tokens);
            } else {
                self.status_line = "Usage: /context [token_count]".to_string();
            }
            return true;
        }
        if self.is_builtin_command(cmd) {
            self.execute_command(cmd);
            return true;
        }
        false
    }

    pub(super) fn handle_enter_key(&mut self, modifiers: KeyModifiers) -> bool {
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        let alt = modifiers.contains(KeyModifiers::ALT);
        let ctrl_enter = modifiers.contains(KeyModifiers::CONTROL);
        if shift || alt || ctrl_enter {
            if self.focus != FocusArea::Input {
                self.focus = FocusArea::Input;
                self.input.set_mode(input::InputMode::Insert);
            }
            self.input.reduce(input::InputAction::InsertNewline);
            return false;
        }
        if self.focus == FocusArea::Chat
            && matches!(self.main_pane_view, MainPaneView::Collaboration)
            && self.collaboration.focus() == CollaborationPaneFocus::Detail
        {
            self.submit_selected_collaboration_vote();
            return false;
        }
        if self.focus == FocusArea::Chat
            && matches!(
                self.main_pane_view,
                MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
            )
        {
            if self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::Plan
                && self.activate_goal_workspace_plan_target()
            {
                return false;
            }
            if self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::CommandBar
            {
                self.activate_goal_workspace_command_bar();
                return false;
            }
            if self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::Timeline
                && self.activate_goal_workspace_timeline_target()
            {
                return false;
            }
            if self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::Details
                && self.activate_goal_workspace_detail_target()
            {
                return false;
            }
        }
        if self.activate_goal_composer_selection() {
            return false;
        }
        if self.focus == FocusArea::Chat {
            if let Some(sel) = self.chat.selected_message() {
                let is_tool = self
                    .chat
                    .active_thread()
                    .and_then(|thread| thread.messages.get(sel))
                    .map(|msg| msg.role == chat::MessageRole::Tool)
                    .unwrap_or(false);
                if is_tool {
                    self.chat.toggle_tool_expansion(sel);
                }
                let has_reasoning = self
                    .chat
                    .active_thread()
                    .and_then(|thread| thread.messages.get(sel))
                    .map(|msg| {
                        (msg.role == chat::MessageRole::Assistant && msg.reasoning.is_some())
                            || widgets::message::is_collapsible_system_notice_message(msg)
                    })
                    .unwrap_or(false);
                if has_reasoning {
                    self.chat.toggle_reasoning(sel);
                }
                return false;
            }
        }
        if self.focus == FocusArea::Sidebar {
            self.handle_sidebar_enter();
            return false;
        }
        if self.should_show_operator_profile_onboarding()
            && self.is_current_operator_profile_bool_question()
            && !modifiers
                .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL)
        {
            let _ = self.submit_operator_profile_answer();
            return false;
        }
        if self.focus != FocusArea::Input {
            self.focus = FocusArea::Input;
            self.input.set_mode(input::InputMode::Insert);
            return false;
        }
        let goal_composer_prompt_matches_input =
            matches!(self.main_pane_view, MainPaneView::GoalComposer)
                && self.goal_mission_control.prompt_text() == self.input.buffer();
        self.input.reduce(input::InputAction::Submit);
        if let Some(prompt) = self.input.take_submitted() {
            if self.should_show_operator_profile_onboarding() {
                if !prompt.starts_with('/') {
                    self.input.set_text(&prompt);
                    if self.submit_operator_profile_answer() {
                        return false;
                    }
                }
                self.input.set_text(&prompt);
                self.show_input_notice(
                    "Onboarding active: answer in input (Ctrl+S skip, Ctrl+D defer)",
                    InputNoticeKind::Warning,
                    90,
                    true,
                );
                return false;
            }
            if matches!(self.main_pane_view, MainPaneView::GoalComposer) {
                if goal_composer_prompt_matches_input {
                    self.goal_mission_control.set_prompt_text(prompt);
                }
                self.start_goal_run_from_mission_control();
                return false;
            }
            if prompt.starts_with('/') {
                if !self.execute_slash_command_line(&prompt) {
                    self.submit_prompt(prompt);
                }
            } else {
                self.submit_prompt(prompt);
            }
        }
        false
    }
}
