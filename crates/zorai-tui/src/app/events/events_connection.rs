use super::*;

impl TuiModel {
    fn last_error_is_transport_disconnect(&self) -> bool {
        self.last_error.as_ref().is_some_and(|message| {
            let normalized = message.to_ascii_lowercase();
            normalized.starts_with("send error:")
                || normalized.starts_with("keepalive send error:")
                || normalized.starts_with("connection error:")
                || normalized.starts_with("connection lost:")
        })
    }

    pub(in crate::app) fn handle_connected_event(&mut self) {
        self.connected = true;
        self.agent_config_loaded = false;
        self.ignore_pending_concierge_welcome = false;
        self.operator_profile_auto_start_requested = false;
        self.operator_profile_auto_start_pending_summary = false;
        self.operator_profile.loading = false;
        self.operator_profile.bool_answer = None;
        self.close_operator_profile_onboarding_modal();
        self.status_line = "Connected to daemon".to_string();
        self.send_daemon_command(DaemonCommand::Refresh);
        self.send_daemon_command(DaemonCommand::GetConfig);
        self.send_daemon_command(DaemonCommand::ListPromptQueue { thread_id: None });
        self.send_daemon_command(DaemonCommand::ListGatewayChats);
    }

    pub(in crate::app) fn handle_disconnected_event(&mut self) {
        let clear_transport_error = self.last_error_is_transport_disconnect();
        self.capture_pending_reconnect_restore();
        self.connected = false;
        self.agent_config_loaded = false;
        self.last_attention_surface = None;
        self.default_session_id = None;
        self.clear_all_agent_activity();
        self.participant_playground_activity.clear();
        self.thread_loading_id = None;
        self.operator_profile.visible = false;
        self.operator_profile.loading = false;
        self.operator_profile.bool_answer = None;
        self.close_operator_profile_onboarding_modal();
        self.operator_profile_auto_start_pending_summary = false;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
        self.chat.reduce(chat::ChatAction::ResetStreaming);
        self.clear_pending_stop();
        self.clear_openai_auth_modal_state();
        if clear_transport_error {
            self.last_error = None;
            self.error_active = false;
            if self.modal.top() == Some(crate::state::modal::ModalKind::ErrorViewer) {
                self.close_top_modal();
            }
        }
        self.status_line = "Disconnected from daemon".to_string();
    }

    pub(in crate::app) fn handle_reconnecting_event(&mut self, delay_secs: u64) {
        self.capture_pending_reconnect_restore();
        self.connected = false;
        self.last_attention_surface = None;
        self.default_session_id = None;
        self.clear_all_agent_activity();
        self.participant_playground_activity.clear();
        self.thread_loading_id = None;
        self.operator_profile.visible = false;
        self.operator_profile.loading = false;
        self.operator_profile.bool_answer = None;
        self.close_operator_profile_onboarding_modal();
        self.operator_profile_auto_start_pending_summary = false;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
        self.chat.reduce(chat::ChatAction::ResetStreaming);
        self.clear_pending_stop();
        self.clear_openai_auth_modal_state();
        self.status_line = format!("Connection lost. Retrying in {}s", delay_secs);
    }

    pub(in crate::app) fn handle_session_spawned_event(&mut self, session_id: String) {
        self.default_session_id = Some(session_id.clone());
        self.status_line = format!("Session: {}", session_id);
    }

    pub(in crate::app) fn handle_approval_required_event(
        &mut self,
        approval_id: String,
        command: String,
        rationale: Option<String>,
        reasons: Vec<String>,
        risk_level: String,
        blast_radius: String,
    ) {
        let task_match = self
            .tasks
            .tasks()
            .iter()
            .find(|task| task.awaiting_approval_id.as_deref() == Some(approval_id.as_str()));
        let goal_match = self.tasks.goal_runs().iter().find(|goal_run| {
            goal_run.awaiting_approval_id.as_deref() == Some(approval_id.as_str())
        });
        let thread_id = task_match
            .and_then(|task| task.thread_id.clone())
            .or_else(|| goal_match.and_then(|goal_run| goal_run.thread_id.clone()));
        let thread_title = self.thread_title_for_id(thread_id.as_deref());
        let is_current_thread = match thread_id.as_deref() {
            Some(thread_id) => Some(thread_id) == self.chat.active_thread_id(),
            None => true,
        };
        self.approval
            .reduce(crate::state::ApprovalAction::ApprovalRequired(
                crate::state::PendingApproval {
                    approval_id: approval_id.clone(),
                    task_id: task_match
                        .map(|task| task.id.clone())
                        .unwrap_or_else(|| approval_id.clone()),
                    task_title: task_match.map(|task| task.title.clone()),
                    thread_id: thread_id.clone(),
                    thread_title: thread_title.clone(),
                    workspace_id: self.current_workspace_id().map(str::to_string),
                    rationale,
                    reasons,
                    command,
                    risk_level: crate::state::RiskLevel::from_str_lossy(&risk_level),
                    blast_radius,
                    received_at: Self::current_unix_ms().max(0) as u64,
                    seen_at: None,
                },
            ));
        self.approval
            .reduce(crate::state::ApprovalAction::SelectApproval(
                approval_id.clone(),
            ));
        let is_current_goal = self
            .active_goal_approval_context()
            .is_some_and(|context| context.approval_id == approval_id);
        if is_current_thread || is_current_goal {
            self.sync_contextual_approval_overlay();
        } else if !is_current_thread {
            let thread_label = thread_title
                .clone()
                .filter(|title: &String| !title.trim().is_empty())
                .unwrap_or_else(|| {
                    thread_id
                        .clone()
                        .unwrap_or_else(|| "another thread".to_string())
                });
            self.show_input_notice(
                format!("Approval pending in {thread_label}. Press Ctrl+A."),
                InputNoticeKind::Warning,
                160,
                true,
            );
        }
        self.status_line = if is_current_goal {
            "Approval required in current goal".to_string()
        } else if is_current_thread {
            "Approval required in current thread".to_string()
        } else {
            format!(
                "Approval required in {}",
                thread_title.unwrap_or_else(|| "background thread".to_string())
            )
        };
    }

    pub(in crate::app) fn handle_approval_resolved_event(
        &mut self,
        approval_id: String,
        decision: String,
    ) {
        let goal_run_id = self
            .tasks
            .goal_runs()
            .iter()
            .find(|goal_run| goal_run.awaiting_approval_id.as_deref() == Some(approval_id.as_str()))
            .map(|goal_run| goal_run.id.clone());
        self.approval.reduce(crate::state::ApprovalAction::Resolve {
            approval_id: approval_id.clone(),
            decision,
        });
        if let Some(goal_run_id) = goal_run_id {
            self.request_authoritative_goal_run_refresh(goal_run_id);
        }
        self.sync_contextual_approval_overlay();
        self.status_line = "Approval resolved".to_string();
    }

    pub(in crate::app) fn handle_task_approval_rules_event(
        &mut self,
        rules: Vec<zorai_protocol::TaskApprovalRule>,
    ) {
        self.approval.reduce(crate::state::ApprovalAction::SetRules(
            rules
                .into_iter()
                .map(|rule| crate::state::approval::SavedApprovalRule {
                    id: rule.id,
                    command: rule.command,
                    created_at: rule.created_at,
                    last_used_at: rule.last_used_at,
                    use_count: rule.use_count,
                })
                .collect(),
        ));
    }
}
