use super::*;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ThreadHandoffSlashAction {
    OpenModal,
    Push {
        target_alias: String,
        reason: Option<String>,
    },
    Return {
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ThreadHandoffModalAction {
    Return,
    Push {
        agent_id: String,
        agent_name: String,
    },
}

fn canonical_handoff_agent_id(agent_id: &str) -> String {
    let canonical = agent_id.trim().to_ascii_lowercase();
    let canonical = canonical
        .strip_suffix("_builtin")
        .unwrap_or(canonical.as_str());
    match canonical {
        "svarog" | "swarog" | "main" => zorai_protocol::AGENT_ID_SWAROG.to_string(),
        "rarog" | "concierge" => zorai_protocol::AGENT_ID_RAROG.to_string(),
        "veles" => "weles".to_string(),
        _ => canonical.to_string(),
    }
}

const LOCAL_THREAD_HANDOFF_ERROR: &str =
    "Send the first message to create the daemon thread before using handoff";

pub(crate) fn parse_thread_handoff_args(args: &str) -> Result<ThreadHandoffSlashAction, String> {
    let args = args.trim();
    if args.is_empty() {
        return Ok(ThreadHandoffSlashAction::OpenModal);
    }

    let mut parts = args.splitn(2, char::is_whitespace);
    let action = parts.next().unwrap_or_default();
    let reason = parts
        .next()
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .map(ToOwned::to_owned);

    if action.eq_ignore_ascii_case("status") && reason.is_none() {
        Ok(ThreadHandoffSlashAction::OpenModal)
    } else if action.eq_ignore_ascii_case("return") {
        Ok(ThreadHandoffSlashAction::Return { reason })
    } else {
        Ok(ThreadHandoffSlashAction::Push {
            target_alias: action.to_string(),
            reason,
        })
    }
}

impl TuiModel {
    pub(super) fn active_daemon_handoff_thread_id(&mut self) -> Option<String> {
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            self.status_line = "Start or load thread first".to_string();
            self.show_input_notice(
                "Start or load thread first",
                InputNoticeKind::Warning,
                90,
                false,
            );
            return None;
        };
        if thread_id.starts_with("local-") {
            self.status_line = LOCAL_THREAD_HANDOFF_ERROR.to_string();
            self.show_input_notice(
                LOCAL_THREAD_HANDOFF_ERROR,
                InputNoticeKind::Warning,
                120,
                false,
            );
            return None;
        }
        Some(thread_id)
    }

    fn resolve_handoff_target_agent_id(&self, agent_alias: &str) -> Option<String> {
        let canonical_alias = canonical_handoff_agent_id(agent_alias);
        if canonical_alias.is_empty() {
            return None;
        }

        if crate::state::subagents::BUILTIN_PERSONA_ROLE_CHOICES
            .iter()
            .any(|choice| canonical_handoff_agent_id(choice.id) == canonical_alias)
        {
            return Some(canonical_alias);
        }

        self.subagents
            .entries
            .iter()
            .filter(|entry| entry.enabled)
            .find(|entry| {
                canonical_handoff_agent_id(&entry.id) == canonical_alias
                    || entry.name.trim().eq_ignore_ascii_case(agent_alias.trim())
            })
            .map(|entry| canonical_handoff_agent_id(&entry.id))
    }

    pub(crate) fn execute_thread_handoff_slash(&mut self, args: &str) {
        match parse_thread_handoff_args(args) {
            Ok(ThreadHandoffSlashAction::OpenModal) => self.open_thread_handoff_modal(),
            Ok(ThreadHandoffSlashAction::Push {
                target_alias,
                reason,
            }) => {
                if self.active_daemon_handoff_thread_id().is_none() {
                    return;
                }
                let Some(target_agent_id) = self.resolve_handoff_target_agent_id(&target_alias)
                else {
                    self.status_line =
                        format!("Unknown or disabled handoff target: {target_alias}");
                    return;
                };
                let target_agent_name = self.participant_display_name(&target_agent_id);
                self.submit_thread_handoff_push(target_agent_id, target_agent_name, reason);
            }
            Ok(ThreadHandoffSlashAction::Return { reason }) => {
                self.submit_thread_handoff_return(reason);
            }
            Err(error) => {
                self.status_line = error;
            }
        }
    }

    pub(crate) fn thread_handoff_modal_actions(&self) -> Vec<ThreadHandoffModalAction> {
        let Some(thread) = self.chat.active_thread() else {
            return Vec::new();
        };
        let current_agent_id = thread
            .thread_handoff_state
            .as_ref()
            .map(|state| state.active_agent_id.as_str())
            .filter(|agent_id| !agent_id.trim().is_empty())
            .or(thread.agent_name.as_deref())
            .map(canonical_handoff_agent_id)
            .unwrap_or_default();
        let has_return = thread
            .thread_handoff_state
            .as_ref()
            .is_some_and(|state| state.responder_stack.len() > 1);
        let mut actions = Vec::new();
        let mut seen = HashSet::new();

        if has_return {
            actions.push(ThreadHandoffModalAction::Return);
        }
        let mut push_target = |agent_id: &str, agent_name: &str| {
            let canonical = canonical_handoff_agent_id(agent_id);
            if canonical.is_empty()
                || canonical == current_agent_id
                || !seen.insert(canonical.clone())
            {
                return;
            }
            actions.push(ThreadHandoffModalAction::Push {
                agent_id: canonical,
                agent_name: agent_name.to_string(),
            });
        };

        push_target(zorai_protocol::AGENT_ID_SWAROG, "Svarog");
        push_target(zorai_protocol::AGENT_ID_RAROG, "Rarog");
        for choice in crate::state::subagents::BUILTIN_PERSONA_ROLE_CHOICES {
            push_target(choice.id, choice.label);
        }
        for entry in self.subagents.entries.iter().filter(|entry| entry.enabled) {
            let label = if entry.name.trim().is_empty() {
                entry.id.as_str()
            } else {
                entry.name.as_str()
            };
            push_target(&entry.id, label);
        }
        actions
    }

    pub(crate) fn thread_handoff_modal_body(&self) -> String {
        let Some(thread) = self.chat.active_thread() else {
            return "No active thread selected.".to_string();
        };
        let (active_agent_id, frames) = if let Some(state) = thread.thread_handoff_state.as_ref() {
            (
                state.active_agent_id.clone(),
                state
                    .responder_stack
                    .iter()
                    .map(|frame| {
                        let raw_id = frame.agent_id.clone();
                        let canonical = canonical_handoff_agent_id(&frame.agent_id);
                        (raw_id, canonical, frame.agent_name.clone())
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            let agent_name = thread.agent_name.as_deref().unwrap_or("Unknown");
            let agent_id = if agent_name.eq_ignore_ascii_case("Svarog") {
                zorai_protocol::AGENT_ID_SWAROG.to_string()
            } else if agent_name.eq_ignore_ascii_case("Rarog") {
                zorai_protocol::AGENT_ID_RAROG.to_string()
            } else {
                canonical_handoff_agent_id(agent_name)
            };
            (
                agent_id.clone(),
                vec![(
                    agent_id.clone(),
                    canonical_handoff_agent_id(&agent_id),
                    agent_name.to_string(),
                )],
            )
        };
        let active_label = crate::agent_aliases::display_agent_name(
            &self.participant_display_name(&active_agent_id),
        );
        let active_canonical = canonical_handoff_agent_id(&active_agent_id);
        let mut body = format!(
            "Thread: {}\nActive responder: {}\nResponder stack ({})\n",
            thread.title,
            active_label,
            frames.len()
        );
        for (index, (raw_id, _canonical, agent_name)) in frames.iter().enumerate() {
            let label = if agent_name.trim().is_empty() {
                crate::agent_aliases::display_agent_name(&self.participant_display_name(raw_id))
            } else {
                crate::agent_aliases::display_agent_name(agent_name)
            };
            let active = raw_id == &active_agent_id
                || canonical_handoff_agent_id(raw_id) == active_canonical;
            body.push_str(&format!(
                "  {}. {} ({}){}\n",
                index + 1,
                label,
                raw_id,
                if active { " [active]" } else { "" }
            ));
        }
        body.push('\n');
        for (index, action) in self.thread_handoff_modal_actions().iter().enumerate() {
            let marker = if index == self.modal.picker_cursor() {
                ">"
            } else {
                " "
            };
            match action {
                ThreadHandoffModalAction::Return => {
                    body.push_str(&format!("{marker} Return to previous responder\n"));
                }
                ThreadHandoffModalAction::Push {
                    agent_id,
                    agent_name,
                } => {
                    body.push_str(&format!(
                        "{marker} Hand off to {} ({agent_id})\n",
                        crate::agent_aliases::display_agent_name(agent_name)
                    ));
                }
            }
        }
        body.push_str("\n↑↓ nav  Enter select  Esc close");
        body
    }

    pub(crate) fn thread_handoff_modal_action_start_line(&self) -> usize {
        let frame_count = self
            .chat
            .active_thread()
            .and_then(|thread| thread.thread_handoff_state.as_ref())
            .map_or(1, |state| state.responder_stack.len());
        4 + frame_count
    }

    pub(crate) fn sync_thread_handoff_modal_item_count(&mut self) {
        self.modal
            .set_picker_item_count(self.thread_handoff_modal_actions().len());
    }

    pub(crate) fn thread_handoff_modal_cursor_scroll(&self) -> usize {
        let total_lines = self.thread_handoff_modal_body().lines().count();
        self.cursor_follow_scroll(
            modal::ModalKind::ThreadHandoff,
            self.thread_handoff_modal_action_start_line(),
            total_lines,
        )
    }

    pub(crate) fn open_thread_handoff_modal(&mut self) {
        if self.active_daemon_handoff_thread_id().is_none() {
            return;
        }
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadHandoff));
        self.sync_thread_handoff_modal_item_count();
        self.status_line = "Viewing thread handoff".to_string();
    }

    pub(crate) fn submit_thread_handoff_modal_action(&mut self) {
        let action = self
            .thread_handoff_modal_actions()
            .get(self.modal.picker_cursor())
            .cloned();
        let Some(action) = action else {
            self.status_line = "No handoff action selected".to_string();
            return;
        };
        self.close_top_modal();
        match action {
            ThreadHandoffModalAction::Return => self.submit_thread_handoff_return(None),
            ThreadHandoffModalAction::Push {
                agent_id,
                agent_name,
            } => self.submit_thread_handoff_push(agent_id, agent_name, None),
        }
    }

    pub(crate) fn submit_thread_handoff_push(
        &mut self,
        target_agent_id: String,
        target_agent_name: String,
        reason: Option<String>,
    ) {
        let Some(thread_id) = self.active_daemon_handoff_thread_id() else {
            return;
        };

        let reason =
            reason.unwrap_or_else(|| format!("Operator requested handoff to {target_agent_name}"));
        let summary = format!("Continue this thread as {target_agent_name}");
        self.send_daemon_command(DaemonCommand::ThreadHandoff {
            thread_id,
            action: "push_handoff".to_string(),
            target_agent_id: Some(target_agent_id),
            reason,
            summary,
        });
        self.status_line = format!("Requesting handoff to {target_agent_name}...");
    }

    pub(crate) fn handle_thread_handoff_result(
        &mut self,
        result: zorai_protocol::ThreadHandoffResult,
    ) {
        if result.ok {
            let active = result
                .active_agent_id
                .as_deref()
                .unwrap_or("updated responder");
            let depth = result.stack_depth.unwrap_or(0);
            self.status_line = format!("Thread handed off to {active} (stack depth {depth})");
            self.show_input_notice(
                self.status_line.clone(),
                InputNoticeKind::Success,
                120,
                false,
            );
            self.modal.reduce(modal::ModalAction::RemoveAll(
                modal::ModalKind::ThreadHandoff,
            ));
            self.request_authoritative_thread_refresh(result.thread_id, false);
        } else {
            let error = result
                .error
                .unwrap_or_else(|| "Thread handoff failed".to_string());
            self.status_line = error.clone();
            self.show_input_notice(error, InputNoticeKind::Warning, 120, false);
        }
    }

    pub(crate) fn submit_thread_handoff_return(&mut self, reason: Option<String>) {
        let Some(thread_id) = self.active_daemon_handoff_thread_id() else {
            return;
        };
        let Some(thread) = self.chat.active_thread() else {
            return;
        };
        if thread
            .thread_handoff_state
            .as_ref()
            .map_or(true, |state| state.responder_stack.len() <= 1)
        {
            self.status_line = "No previous responder to return to".to_string();
            return;
        }
        let reason = reason
            .unwrap_or_else(|| "Operator requested return to the previous responder".to_string());
        self.send_daemon_command(DaemonCommand::ThreadHandoff {
            thread_id,
            action: "return_handoff".to_string(),
            target_agent_id: None,
            reason,
            summary: "Resume this thread as the previous responder".to_string(),
        });
        self.status_line = "Requesting return to the previous responder...".to_string();
    }
}
