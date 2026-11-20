use super::*;
use crate::widgets;
use crossterm::event::{KeyCode, KeyModifiers, ModifierKeyCode};
impl TuiModel {
    pub(crate) fn execute_selected_inline_message_action(&mut self) -> bool {
        let Some(message_index) = self.chat.selected_message() else {
            return false;
        };
        let Some(message) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
        else {
            return false;
        };

        let action_index = self.chat.selected_message_action();
        let Some((_, target)) = widgets::chat::message_action_targets(
            &self.chat,
            message_index,
            message,
            self.tick_counter,
        )
        .into_iter()
        .nth(action_index) else {
            return false;
        };

        match target {
            chat::ChatHitTarget::MessageAction {
                message_index,
                action_index,
            } => {
                self.chat.select_message(Some(message_index));
                self.chat.select_message_action(action_index);
                self.execute_concierge_message_action(message_index, action_index);
                true
            }
            chat::ChatHitTarget::CopyMessage(index) => {
                self.chat.select_message(Some(index));
                self.copy_message(index);
                true
            }
            chat::ChatHitTarget::ForkMessage(index) => {
                self.chat.select_message(Some(index));
                self.fork_message(index);
                true
            }
            chat::ChatHitTarget::ResendMessage(index) => {
                self.chat.select_message(Some(index));
                self.resend_message(index);
                true
            }
            chat::ChatHitTarget::RegenerateMessage(index) => {
                self.chat.select_message(Some(index));
                self.request_regenerate_message(index);
                true
            }
            chat::ChatHitTarget::PinMessage(index) => {
                self.chat.select_message(Some(index));
                self.pin_message_for_compaction(index);
                true
            }
            chat::ChatHitTarget::UnpinMessage(index) => {
                self.chat.select_message(Some(index));
                self.unpin_message_for_compaction(index);
                true
            }
            chat::ChatHitTarget::DeleteMessage(index) => {
                self.chat.select_message(Some(index));
                self.request_delete_message(index);
                true
            }
            chat::ChatHitTarget::ThumbsUp(index) => {
                self.chat.select_message(Some(index));
                self.submit_message_feedback(index, zorai_protocol::Reaction::Up);
                true
            }
            chat::ChatHitTarget::ThumbsDown(index) => {
                self.chat.select_message(Some(index));
                self.submit_message_feedback(index, zorai_protocol::Reaction::Down);
                true
            }
            chat::ChatHitTarget::ExportMessage(index) => {
                self.chat.select_message(Some(index));
                self.export_thread(index);
                true
            }
            chat::ChatHitTarget::ToolFilePath { message_index } => {
                self.chat.select_message(Some(message_index));
                false
            }
            _ => false,
        }
    }

    pub(crate) fn update_held_modifier(&mut self, code: KeyCode, pressed: bool) {
        let modifier = match code {
            KeyCode::Modifier(
                ModifierKeyCode::LeftShift
                | ModifierKeyCode::RightShift
                | ModifierKeyCode::IsoLevel3Shift
                | ModifierKeyCode::IsoLevel5Shift,
            ) => Some(KeyModifiers::SHIFT),
            KeyCode::Modifier(ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl) => {
                Some(KeyModifiers::CONTROL)
            }
            KeyCode::Modifier(ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt) => {
                Some(KeyModifiers::ALT)
            }
            _ => None,
        };

        if let Some(modifier) = modifier {
            if pressed {
                self.held_key_modifiers.insert(modifier);
            } else {
                self.held_key_modifiers.remove(modifier);
            }
        }
    }

    pub(crate) fn next_reply_offer(&self) -> Option<&str> {
        let (thread_id, suggestion) = self.next_reply_suggestion.as_ref()?;
        (self.input.buffer().is_empty()
            && self.attachments.is_empty()
            && self.chat.active_thread_id() == Some(thread_id.as_str())
            && !self.assistant_busy())
        .then_some(suggestion.as_str())
    }

    pub(crate) fn input_notice_style(&self) -> Option<(&str, Style)> {
        self.input_notice.as_ref().map(|notice| {
            let style = match notice.kind {
                InputNoticeKind::Warning => Style::default().fg(Color::Indexed(214)),
                InputNoticeKind::Success => Style::default().fg(Color::Indexed(114)),
                InputNoticeKind::Error => Style::default().fg(Color::Indexed(203)),
            };
            (notice.text.as_str(), style)
        })
    }

    fn budget_exceeded_task_for_thread(
        &self,
        thread_id: &str,
    ) -> Option<&crate::state::task::AgentTask> {
        self.tasks
            .tasks()
            .iter()
            .filter(|task| {
                task.thread_id.as_deref() == Some(thread_id)
                    && task.status == Some(crate::state::task::TaskStatus::BudgetExceeded)
            })
            .max_by_key(|task| task.created_at)
    }

    fn thread_budget_exceeded_notice(&self, thread_id: &str) -> Option<String> {
        self.budget_exceeded_task_for_thread(thread_id)?;
        Some(format!(
            "Thread budget exceeded for {thread_id}. Review completed work here; continue from the parent thread or respawn with a larger child budget."
        ))
    }

    pub(crate) fn restore_prompt_and_show_budget_exceeded_notice(
        &mut self,
        thread_id: &str,
        prompt: &str,
    ) -> bool {
        let Some(notice) = self.thread_budget_exceeded_notice(thread_id) else {
            return false;
        };
        self.input.set_text(prompt);
        self.status_line = notice.clone();
        self.show_input_notice(notice, InputNoticeKind::Error, 120, false);
        true
    }

    pub(crate) fn active_thread_budget_exceeded_notice(&self) -> Option<String> {
        let thread_id = self.chat.active_thread_id()?;
        self.thread_budget_exceeded_notice(thread_id)
    }

    pub(crate) fn toggle_notifications_modal(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::Notifications) {
            self.close_top_modal();
        } else {
            let header_action = self.notifications.first_enabled_header_action();
            self.notifications
                .reduce(crate::state::NotificationsAction::FocusHeader(
                    header_action,
                ));
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Notifications));
        }
    }

    pub(crate) fn toggle_approval_center(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::ApprovalCenter) {
            self.close_top_modal();
        } else {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::ApprovalCenter));
            self.send_daemon_command(DaemonCommand::ListTaskApprovalRules);
        }
    }

    pub(crate) fn current_workspace_id(&self) -> Option<&str> {
        let workspace = self.config.honcho_workspace_id.trim();
        if workspace.is_empty() {
            None
        } else {
            Some(workspace)
        }
    }

    fn visible_approval_ids(&self) -> Vec<String> {
        self.approval
            .visible_approvals(self.chat.active_thread_id(), self.current_workspace_id())
            .iter()
            .map(|approval| approval.approval_id.clone())
            .collect()
    }

    pub(crate) fn step_approval_selection(&mut self, delta: i32) {
        let visible = self.visible_approval_ids();
        if visible.is_empty() {
            return;
        }
        let current = self
            .approval
            .selected_approval_id()
            .and_then(|approval_id| visible.iter().position(|id| id == approval_id))
            .unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, visible.len().saturating_sub(1) as i32) as usize;
        self.approval
            .reduce(crate::state::ApprovalAction::SelectApproval(
                visible[next].clone(),
            ));
    }

    pub(crate) fn select_approval_center_row(&mut self, index: usize) {
        let visible = self.visible_approval_ids();
        if let Some(approval_id) = visible.get(index) {
            self.approval
                .reduce(crate::state::ApprovalAction::SelectApproval(
                    approval_id.clone(),
                ));
        }
    }

    pub(crate) fn select_approval_center_rule_row(&mut self, index: usize) {
        if let Some(rule_id) = self
            .approval
            .saved_rules()
            .get(index)
            .map(|rule| rule.id.clone())
        {
            self.approval
                .reduce(crate::state::ApprovalAction::SelectRule(rule_id));
        }
    }

    pub(crate) fn create_task_approval_rule(&mut self, approval_id: String) {
        self.send_daemon_command(DaemonCommand::CreateTaskApprovalRule {
            approval_id: approval_id.clone(),
        });
        self.resolve_approval(approval_id, "allow_once");
        self.status_line = "Saved always-approve rule".to_string();
    }

    pub(crate) fn revoke_selected_task_approval_rule(&mut self) {
        let Some(rule_id) = self.approval.selected_rule().map(|rule| rule.id.clone()) else {
            return;
        };
        self.approval
            .reduce(crate::state::ApprovalAction::RemoveRule(rule_id.clone()));
        self.send_daemon_command(DaemonCommand::RevokeTaskApprovalRule { rule_id });
        self.status_line = "Revoked always-approve rule".to_string();
    }

    pub(crate) fn resolve_approval(&mut self, approval_id: String, decision: &str) {
        self.approval.reduce(crate::state::ApprovalAction::Resolve {
            approval_id: approval_id.clone(),
            decision: decision.to_string(),
        });
        self.send_daemon_command(DaemonCommand::ResolveTaskApproval {
            approval_id,
            decision: decision.to_string(),
        });
    }

    pub(crate) fn active_goal_approval_context(&self) -> Option<GoalApprovalContext> {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return None;
        };
        let goal_run = self.tasks.goal_run_by_id(goal_run_id)?;
        let approval_id = goal_run.awaiting_approval_id.clone()?;
        self.approval.approval_by_id(&approval_id)?;
        Some(GoalApprovalContext {
            approval_id,
            goal_run_id: goal_run.id.clone(),
            thread_id: goal_run.thread_id.clone(),
            goal_title: goal_run.title.clone(),
            step_title: goal_run.current_step_title.clone(),
        })
    }

    fn active_contextual_approval_id(&self) -> Option<String> {
        self.active_goal_approval_context()
            .map(|context| context.approval_id)
            .or_else(|| self.next_current_thread_approval_id())
    }

    pub(crate) fn sync_contextual_approval_overlay(&mut self) {
        let Some(approval_id) = self.active_contextual_approval_id() else {
            self.modal.reduce(modal::ModalAction::RemoveAll(
                modal::ModalKind::GoalApprovalRejectPrompt,
            ));
            self.modal.reduce(modal::ModalAction::RemoveAll(
                modal::ModalKind::ApprovalOverlay,
            ));
            return;
        };

        self.approval
            .reduce(crate::state::ApprovalAction::SelectApproval(approval_id));
        if self.modal.top() != Some(modal::ModalKind::ApprovalOverlay)
            && self.modal.top() != Some(modal::ModalKind::GoalApprovalRejectPrompt)
        {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::ApprovalOverlay));
        }
    }

    fn open_goal_approval_reject_prompt(&mut self, approval_id: String) -> bool {
        let Some(context) = self.active_goal_approval_context() else {
            return false;
        };
        if context.approval_id != approval_id {
            return false;
        }
        self.approval
            .reduce(crate::state::ApprovalAction::SelectApproval(approval_id));
        if self.modal.top() != Some(modal::ModalKind::GoalApprovalRejectPrompt) {
            self.modal.reduce(modal::ModalAction::Push(
                modal::ModalKind::GoalApprovalRejectPrompt,
            ));
        }
        true
    }

    fn close_goal_approval_decision_modals(&mut self) {
        self.modal.reduce(modal::ModalAction::RemoveAll(
            modal::ModalKind::GoalApprovalRejectPrompt,
        ));
        self.modal.reduce(modal::ModalAction::RemoveAll(
            modal::ModalKind::ApprovalOverlay,
        ));
        self.modal.reduce(modal::ModalAction::RemoveAll(
            modal::ModalKind::ApprovalCenter,
        ));
    }

    pub(crate) fn handle_reject_selected_approval(&mut self, approval_id: String) {
        if !self.open_goal_approval_reject_prompt(approval_id.clone()) {
            self.resolve_approval(approval_id, "reject");
            self.sync_contextual_approval_overlay();
        }
    }

    pub(crate) fn rewrite_active_goal_after_reject(&mut self) {
        let Some(context) = self.active_goal_approval_context() else {
            self.close_goal_approval_decision_modals();
            return;
        };
        let prompt = context
            .step_title
            .as_deref()
            .map(|step_title| {
                format!(
                    "Rewrite the blocked goal step \"{step_title}\" using this operator guidance and continue the goal:\n"
                )
            })
            .unwrap_or_else(|| {
                "Rewrite the blocked goal step using this operator guidance and continue the goal:\n"
                    .to_string()
            });
        let status_target = context
            .step_title
            .clone()
            .unwrap_or_else(|| context.goal_title.clone());

        self.resolve_approval(context.approval_id, "reject");
        self.close_goal_approval_decision_modals();
        self.focus = FocusArea::Input;
        self.set_input_text(&prompt);
        self.show_input_notice(
            "Type rewrite guidance and press Enter to continue the goal",
            InputNoticeKind::Warning,
            160,
            true,
        );
        self.status_line =
            format!("Approval rejected for {status_target}. Provide rewrite guidance.");
    }

    pub(crate) fn stop_active_goal_after_reject(&mut self) {
        let Some(context) = self.active_goal_approval_context() else {
            self.close_goal_approval_decision_modals();
            return;
        };

        self.resolve_approval(context.approval_id, "reject");
        self.send_daemon_command(DaemonCommand::ControlGoalRun {
            goal_run_id: context.goal_run_id,
            action: "stop".to_string(),
            step_index: None,
            payload_json: None,
        });
        self.close_goal_approval_decision_modals();
        self.status_line = "Goal approval rejected. Stopping goal run...".to_string();
    }

    pub(crate) fn goal_approval_reject_prompt_body(&self) -> String {
        let Some(context) = self.active_goal_approval_context() else {
            return "No active goal approval is available.\n\nPress Esc to return.".to_string();
        };

        let step_label = context.step_title.as_deref().unwrap_or("current goal step");
        format!(
            "Approval for \"{}\" was rejected.\n\nChoose what to do next for {}:\n\n[R] Rewrite with guidance\nType guidance in the input box on the next step, then press Enter.\n\n[S] Stop goal\nImmediately stop this goal run after rejecting the approval.\n\n[Esc] Back to approval",
            context.goal_title, step_label
        )
    }

    pub(crate) fn next_current_thread_approval_id(&self) -> Option<String> {
        let current_thread_id = self.chat.active_thread_id()?;
        self.approval
            .pending_approvals()
            .iter()
            .find(|approval| approval.thread_id.as_deref() == Some(current_thread_id))
            .map(|approval| approval.approval_id.clone())
    }

    pub(crate) fn current_unix_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0)
    }

    fn upsert_notification_local(&mut self, notification: zorai_protocol::InboxNotification) {
        self.notifications
            .reduce(crate::state::NotificationsAction::Upsert(
                notification.clone(),
            ));
        self.send_daemon_command(DaemonCommand::UpsertNotification(notification));
    }

    pub(crate) fn mark_notification_read(&mut self, notification_id: &str) {
        let Some(mut notification) = self
            .notifications
            .all_items()
            .iter()
            .find(|item| item.id == notification_id)
            .cloned()
        else {
            return;
        };
        if notification.read_at.is_some() {
            return;
        }
        let now = Self::current_unix_ms();
        notification.read_at = Some(now);
        notification.updated_at = now;
        self.upsert_notification_local(notification);
    }

    pub(crate) fn toggle_notification_expand(&mut self, notification_id: String) {
        self.mark_notification_read(&notification_id);
        self.notifications
            .reduce(crate::state::NotificationsAction::ToggleExpand(
                notification_id,
            ));
    }

    pub(crate) fn archive_notification(&mut self, notification_id: &str) {
        let Some(mut notification) = self
            .notifications
            .all_items()
            .iter()
            .find(|item| item.id == notification_id)
            .cloned()
        else {
            return;
        };
        let now = Self::current_unix_ms();
        notification.read_at.get_or_insert(now);
        notification.archived_at = Some(now);
        notification.updated_at = now;
        self.upsert_notification_local(notification);
    }

    pub(crate) fn delete_notification(&mut self, notification_id: &str) {
        let Some(mut notification) = self
            .notifications
            .all_items()
            .iter()
            .find(|item| item.id == notification_id)
            .cloned()
        else {
            return;
        };
        let now = Self::current_unix_ms();
        notification.read_at.get_or_insert(now);
        notification.deleted_at = Some(now);
        notification.updated_at = now;
        self.upsert_notification_local(notification);
    }
}
