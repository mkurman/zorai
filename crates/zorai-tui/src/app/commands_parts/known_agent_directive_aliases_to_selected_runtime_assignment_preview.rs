use super::*;
use crate::widgets;
impl TuiModel {
    pub(crate) fn known_agent_directive_aliases(&self) -> Vec<String> {
        let mut aliases = vec![
            "main".to_string(),
            "svarog".to_string(),
            "swarog".to_string(),
            "weles".to_string(),
            "veles".to_string(),
            zorai_protocol::AGENT_ID_RAROG.to_string(),
            "swarozyc".to_string(),
            "radogost".to_string(),
            "domowoj".to_string(),
            "swietowit".to_string(),
            "perun".to_string(),
            "mokosh".to_string(),
            "dazhbog".to_string(),
            "rod".to_string(),
        ];
        for entry in &self.subagents.entries {
            aliases.push(entry.id.clone());
            aliases.push(entry.name.clone());
        }
        aliases.sort();
        aliases.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        aliases
    }

    pub(crate) fn participant_display_name(&self, agent_alias: &str) -> String {
        if let Some(display_name) = builtin_participant_display_name(agent_alias) {
            return display_name;
        }
        if let Some(entry) = self.subagents.entries.iter().find(|entry| {
            entry.id.eq_ignore_ascii_case(agent_alias)
                || entry.name.eq_ignore_ascii_case(agent_alias)
        }) {
            return entry.name.clone();
        }
        agent_alias.to_string()
    }

    pub(crate) fn is_explicit_builtin_persona(&self, agent_alias: &str) -> bool {
        matches!(
            agent_alias.trim().to_ascii_lowercase().as_str(),
            "swarozyc"
                | "radogost"
                | "domowoj"
                | "swietowit"
                | "perun"
                | "mokosh"
                | "dazhbog"
                | "rod"
        )
    }

    pub(crate) fn builtin_persona_configured(&self, agent_alias: &str) -> bool {
        let Some(raw) = self.config.agent_config_raw.as_ref() else {
            return false;
        };
        let key = match agent_alias.to_ascii_lowercase().as_str() {
            "swarozyc" => "swarozyc",
            "radogost" => "radogost",
            "domowoj" => "domowoj",
            "swietowit" => "swietowit",
            "perun" => "perun",
            "mokosh" => "mokosh",
            "dazhbog" => "dazhbog",
            "rod" => "rod",
            _ => return true,
        };
        let Some(entry) = raw
            .get("builtin_sub_agents")
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_object())
        else {
            return false;
        };
        let provider = entry
            .get("provider")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let model = entry
            .get("model")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        provider.is_some() && model.is_some()
    }

    fn open_builtin_persona_setup_flow(
        &mut self,
        agent_alias: &str,
        continuation: PendingBuiltinPersonaSetupContinuation,
    ) {
        let target_agent_id = agent_alias.trim().to_ascii_lowercase();
        let target_agent_name = self.participant_display_name(agent_alias);
        let config_snapshot = BuiltinPersonaSetupConfigSnapshot {
            provider: self.config.provider.clone(),
            base_url: self.config.base_url.clone(),
            model: self.config.model.clone(),
            custom_model_name: self.config.custom_model_name.clone(),
            api_key: self.config.api_key.clone(),
            assistant_id: self.config.assistant_id.clone(),
            auth_source: self.config.auth_source.clone(),
            api_transport: self.config.api_transport.clone(),
            custom_context_window_tokens: self.config.custom_context_window_tokens,
            context_window_tokens: self.config.context_window_tokens,
            fetched_models: self.config.fetched_models().to_vec(),
        };
        self.pending_builtin_persona_setup = Some(PendingBuiltinPersonaSetup {
            target_agent_id,
            target_agent_name: target_agent_name.clone(),
            continuation,
            config_snapshot,
        });
        self.settings_picker_target = Some(SettingsPickerTarget::BuiltinPersonaProvider);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
        self.sync_provider_picker_item_count();
        self.status_line = format!("Configure {} provider", target_agent_name);
    }

    pub(crate) fn open_builtin_persona_prompt_setup_flow(
        &mut self,
        agent_alias: &str,
        prompt: String,
    ) {
        self.open_builtin_persona_setup_flow(
            agent_alias,
            PendingBuiltinPersonaSetupContinuation::SubmitPrompt(prompt),
        );
    }

    pub(crate) fn open_builtin_persona_workspace_actor_setup_flow(
        &mut self,
        agent_alias: &str,
        pending: PendingWorkspaceActorPicker,
        actor: zorai_protocol::WorkspaceActor,
    ) {
        self.open_builtin_persona_setup_flow(
            agent_alias,
            PendingBuiltinPersonaSetupContinuation::SelectWorkspaceActor { pending, actor },
        );
    }

    pub(crate) fn begin_add_thread_participant(&mut self, agent_id: &str) {
        let Some(thread_id) = self.chat.active_thread_id().map(String::from) else {
            self.status_line = "Open a thread before adding participants".to_string();
            return;
        };
        let agent_name = self.participant_display_name(agent_id);
        let is_unconfigured_builtin = matches!(
            agent_id.trim().to_ascii_lowercase().as_str(),
            "swarozyc"
                | "radogost"
                | "domowoj"
                | "swietowit"
                | "perun"
                | "mokosh"
                | "dazhbog"
                | "rod"
        ) && !self.builtin_persona_configured(agent_id);
        if is_unconfigured_builtin {
            self.open_builtin_persona_setup_flow(
                agent_id,
                PendingBuiltinPersonaSetupContinuation::AddThreadParticipant {
                    thread_id,
                    agent_id: agent_id.to_string(),
                    agent_name,
                },
            );
            return;
        }
        self.open_participant_instruction_editor(
            thread_id,
            agent_id.to_string(),
            agent_name,
            true,
            String::new(),
        );
    }

    pub(crate) fn restore_builtin_persona_setup_config_snapshot(&mut self) {
        let Some(setup) = self.pending_builtin_persona_setup.as_ref() else {
            return;
        };
        let snapshot = &setup.config_snapshot;
        self.config.provider = snapshot.provider.clone();
        self.config.base_url = snapshot.base_url.clone();
        self.config.model = snapshot.model.clone();
        self.config.custom_model_name = snapshot.custom_model_name.clone();
        self.config.api_key = snapshot.api_key.clone();
        self.config.assistant_id = snapshot.assistant_id.clone();
        self.config.auth_source = snapshot.auth_source.clone();
        self.config.api_transport = snapshot.api_transport.clone();
        self.config.custom_context_window_tokens = snapshot.custom_context_window_tokens;
        self.config.context_window_tokens = snapshot.context_window_tokens;
        self.config.reduce(config::ConfigAction::ModelsFetched(
            snapshot.fetched_models.clone(),
        ));
    }

    fn resolve_preview_path(path: &str) -> PathBuf {
        let raw = PathBuf::from(path);
        if raw.is_absolute() {
            raw
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(raw)
        }
    }

    fn find_repo_root(path: &Path) -> Option<PathBuf> {
        let mut current = path.parent().or_else(|| Some(path));
        while let Some(candidate) = current {
            if candidate.join(".git").exists() {
                return Some(candidate.to_path_buf());
            }
            current = candidate.parent();
        }
        None
    }

    pub(crate) fn open_chat_tool_file_preview(&mut self, message_index: usize) {
        let Some(message) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
        else {
            return;
        };
        let Some(chip) = widgets::chat::tool_file_chip(message) else {
            return;
        };

        let resolved_path = Self::resolve_preview_path(&chip.path);
        let show_plain_preview = message.tool_output_preview_path.is_some()
            || matches!(
                chip.tool_name.as_str(),
                zorai_protocol::tool_names::READ_FILE
                    | zorai_protocol::tool_names::READ_SKILL
                    | zorai_protocol::tool_names::GENERATE_IMAGE
            );
        let repo_root = if show_plain_preview {
            None
        } else {
            Self::find_repo_root(&resolved_path)
        };
        let repo_relative_path = repo_root.as_ref().and_then(|root| {
            resolved_path
                .strip_prefix(root)
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        });
        let target = ChatFilePreviewTarget {
            path: resolved_path.to_string_lossy().to_string(),
            repo_root: repo_root
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            repo_relative_path,
        };

        if let Some(repo_root) = target.repo_root.as_ref() {
            self.send_daemon_command(DaemonCommand::RequestGitDiff {
                repo_path: repo_root.clone(),
                file_path: target.repo_relative_path.clone(),
            });
        } else {
            self.send_daemon_command(DaemonCommand::RequestFilePreview {
                path: target.path.clone(),
                max_bytes: Some(65_536),
            });
        }

        let parent_thread_id = matches!(
            self.main_pane_view,
            MainPaneView::Conversation | MainPaneView::WorkContext
        )
        .then(|| self.chat.active_thread_id().map(str::to_string))
        .flatten();
        self.set_mission_control_return_targets(
            self.current_goal_return_target(),
            parent_thread_id,
        );
        self.main_pane_view = MainPaneView::FilePreview(target);
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
    }

    pub(crate) fn open_chat_message_image_preview(&mut self, message_index: usize) {
        let Some(message) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
        else {
            return;
        };
        let Some(path) = widgets::chat::message_image_preview_path(message) else {
            return;
        };

        let target = ChatFilePreviewTarget {
            path,
            repo_root: None,
            repo_relative_path: None,
        };
        self.send_daemon_command(DaemonCommand::RequestFilePreview {
            path: target.path.clone(),
            max_bytes: Some(65_536),
        });
        let parent_thread_id = matches!(
            self.main_pane_view,
            MainPaneView::Conversation | MainPaneView::WorkContext
        )
        .then(|| self.chat.active_thread_id().map(str::to_string))
        .flatten();
        self.set_mission_control_return_targets(
            self.current_goal_return_target(),
            parent_thread_id,
        );
        self.main_pane_view = MainPaneView::FilePreview(target);
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
    }

    pub(crate) fn open_file_preview_path(&mut self, path: String) {
        let target = ChatFilePreviewTarget {
            path,
            repo_root: None,
            repo_relative_path: None,
        };
        self.send_daemon_command(DaemonCommand::RequestFilePreview {
            path: target.path.clone(),
            max_bytes: Some(65_536),
        });
        let parent_thread_id = matches!(
            self.main_pane_view,
            MainPaneView::Conversation | MainPaneView::WorkContext
        )
        .then(|| self.chat.active_thread_id().map(str::to_string))
        .flatten();
        self.set_mission_control_return_targets(
            self.current_goal_return_target(),
            parent_thread_id,
        );
        self.main_pane_view = MainPaneView::FilePreview(target);
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
    }

    pub(crate) fn filtered_goal_runs(&self) -> Vec<&task::GoalRun> {
        let query = self.modal.command_query().to_lowercase();
        self.tasks
            .goal_runs()
            .iter()
            .filter(|run| {
                query.is_empty()
                    || run.title.to_lowercase().contains(&query)
                    || run.goal.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub(crate) fn selected_thread_picker_thread(&self) -> Option<&chat::AgentThread> {
        let cursor = self.modal.picker_cursor();
        if cursor == 0 {
            return None;
        }
        widgets::thread_picker::filtered_threads_for_workspace(
            &self.chat,
            &self.modal,
            &self.subagents,
            &self.tasks,
            &self.workspace,
        )
        .get(cursor - 1)
        .copied()
    }

    pub(crate) fn selected_goal_picker_run(&self) -> Option<&task::GoalRun> {
        let cursor = self.modal.picker_cursor();
        if cursor == 0 {
            return None;
        }
        self.filtered_goal_runs().get(cursor - 1).copied()
    }

    pub(super) fn can_stop_selected_thread(&self) -> bool {
        self.selected_thread_picker_thread().is_some_and(|thread| {
            self.chat.active_thread_id() == Some(thread.id.as_str()) && self.assistant_busy()
        })
    }

    pub(super) fn can_resume_selected_thread(&self) -> bool {
        self.selected_thread_picker_thread().is_some_and(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| message.role == chat::MessageRole::Assistant)
                .is_some_and(|message| message.content.trim_end().ends_with("[stopped]"))
        })
    }

    pub(crate) fn selected_thread_picker_confirm_action(&self) -> Option<PendingConfirmAction> {
        let thread = self.selected_thread_picker_thread()?;
        let title = widgets::thread_picker::thread_display_title_for_workspace(
            thread,
            &self.tasks,
            &self.workspace,
        );
        if self.can_stop_selected_thread() {
            Some(PendingConfirmAction::StopThread {
                thread_id: thread.id.clone(),
                title,
            })
        } else if self.can_resume_selected_thread() {
            Some(PendingConfirmAction::ResumeThread {
                thread_id: thread.id.clone(),
                title,
            })
        } else {
            None
        }
    }

    pub(crate) fn selected_goal_picker_toggle_action(&self) -> Option<PendingConfirmAction> {
        let run = self.selected_goal_picker_run()?;
        let title = run.title.clone();
        match run.status {
            Some(task::GoalRunStatus::Paused) => Some(PendingConfirmAction::ResumeGoalRun {
                goal_run_id: run.id.clone(),
                title,
            }),
            Some(task::GoalRunStatus::Queued)
            | Some(task::GoalRunStatus::Planning)
            | Some(task::GoalRunStatus::Running)
            | Some(task::GoalRunStatus::AwaitingApproval)
            | Some(task::GoalRunStatus::AwaitingReview) => {
                Some(PendingConfirmAction::PauseGoalRun {
                    goal_run_id: run.id.clone(),
                    title,
                })
            }
            _ => None,
        }
    }

    pub(crate) fn selected_goal_run(&self) -> Option<&task::GoalRun> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return None;
        };
        self.tasks.goal_run_by_id(goal_run_id)
    }

    pub(crate) fn selected_goal_run_id(&self) -> Option<String> {
        self.selected_goal_run()
            .map(|run| run.id.clone())
            .or_else(|| self.goal_mission_control.runtime_goal_run_id.clone())
    }

    pub(crate) fn open_mission_control_runtime_editor(&mut self) -> bool {
        if matches!(self.main_pane_view, MainPaneView::GoalComposer)
            && self.goal_mission_control.runtime_mode()
        {
            if let Some(run) = self.mission_control_goal_run().cloned() {
                let preserve_pending = self.goal_mission_control.pending_role_assignments.is_some();
                self.sync_goal_mission_control_from_run(&run, preserve_pending);
            }
        } else if !self.sync_goal_mission_control_from_selected_goal_run() {
            return false;
        }
        if let Some(target) = self.current_goal_target_for_mission_control() {
            self.set_mission_control_return_targets(Some(target), None);
        }
        self.main_pane_view = MainPaneView::GoalComposer;
        self.goal_mission_control
            .set_focused_section(goal_mission_control::GoalMissionControlSection::RoleAssignments);
        self.focus = FocusArea::Chat;
        self.task_view_scroll = 0;
        true
    }

    pub(crate) fn cancel_goal_mission_control(&mut self) -> bool {
        if !matches!(self.main_pane_view, MainPaneView::GoalComposer) {
            return false;
        }

        let fallback_target =
            self.goal_mission_control
                .runtime_goal_run_id
                .as_ref()
                .map(|goal_run_id| sidebar::SidebarItemTarget::GoalRun {
                    goal_run_id: goal_run_id.clone(),
                    step_id: None,
                });
        let target = self
            .mission_control_source_goal_target()
            .or(fallback_target);

        if let Some(target) = target {
            self.open_sidebar_target(target);
            self.focus = FocusArea::Chat;
            self.status_line = "Closed Mission Control".to_string();
        } else {
            self.set_main_pane_conversation(FocusArea::Chat);
            self.status_line = "Cancelled new goal".to_string();
        }
        true
    }

    pub(crate) fn selected_runtime_assignment_preview(
        &self,
    ) -> Option<(usize, task::GoalAgentAssignment)> {
        let index = self.goal_mission_control.selected_runtime_assignment_index;
        self.goal_mission_control
            .selected_runtime_assignment()
            .cloned()
            .map(|assignment| (index, assignment))
    }
}
