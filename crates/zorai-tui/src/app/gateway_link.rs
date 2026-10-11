use super::*;
use crate::client::GatewayChatVm;

const GATEWAY_LINK_ERROR_PREFIX: &str = "Failed to link thread to gateway chat";
const GATEWAY_LIST_ERROR_PREFIX: &str = "Failed to list gateway chats";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GatewayLinkModalAction {
    Unlink { label: String },
    Link { channel_key: String, label: String },
}

pub(crate) fn gateway_chat_label(chat: &GatewayChatVm) -> String {
    let platform = chat.platform.trim();
    let title = chat.last_thread_title.as_deref().unwrap_or("").trim();
    let name = match title.get(..platform.len()) {
        Some(head) if !platform.is_empty() && head.eq_ignore_ascii_case(platform) => {
            title[platform.len()..].trim()
        }
        _ => title,
    };
    let name = if name.is_empty() {
        chat.channel_id.trim()
    } else {
        name
    };
    format!("{platform} {name}")
}

impl TuiModel {
    fn gateway_link_active_thread_id(&self) -> Option<&str> {
        self.chat
            .active_thread_id()
            .filter(|thread_id| !thread_id.starts_with("local-"))
    }

    pub(crate) fn gateway_link_modal_actions(&self) -> Vec<GatewayLinkModalAction> {
        let active = self.gateway_link_active_thread_id();
        let mut actions = Vec::new();
        if let Some(linked) = self
            .gateway_chats
            .iter()
            .find(|chat| chat.thread_id.is_some() && chat.thread_id.as_deref() == active)
        {
            actions.push(GatewayLinkModalAction::Unlink {
                label: gateway_chat_label(linked),
            });
        }
        actions.extend(
            self.gateway_chats
                .iter()
                .map(|chat| GatewayLinkModalAction::Link {
                    channel_key: chat.channel_key.clone(),
                    label: gateway_chat_label(chat),
                }),
        );
        actions
    }

    pub(crate) fn gateway_link_modal_body(&self) -> String {
        let active = self.gateway_link_active_thread_id();
        let title = self
            .chat
            .active_thread()
            .map(|thread| thread.title.as_str())
            .unwrap_or("current thread");
        let mut body = format!("Continue \"{title}\" on a messaging chat:\n\n");
        let cursor = self.modal.picker_cursor();
        for (index, action) in self.gateway_link_modal_actions().iter().enumerate() {
            let marker = if index == cursor { ">" } else { " " };
            match action {
                GatewayLinkModalAction::Unlink { label } => {
                    body.push_str(&format!("{marker} Unlink from {label}\n"));
                }
                GatewayLinkModalAction::Link { channel_key, label } => {
                    let chat = self
                        .gateway_chats
                        .iter()
                        .find(|chat| &chat.channel_key == channel_key);
                    let state = match chat.and_then(|chat| chat.thread_id.as_deref()) {
                        None => "default".to_string(),
                        Some(thread_id) if Some(thread_id) == active => {
                            "\u{25cf} this thread".to_string()
                        }
                        Some(thread_id) => {
                            let other = self
                                .chat
                                .threads()
                                .iter()
                                .find(|thread| thread.id == thread_id)
                                .map(|thread| thread.title.clone())
                                .or_else(|| chat.and_then(|chat| chat.thread_title.clone()))
                                .unwrap_or_else(|| thread_id.to_string());
                            format!("on: {other}")
                        }
                    };
                    body.push_str(&format!("{marker} {label}  ({state})\n"));
                }
            }
        }
        if self.gateway_chats.is_empty() {
            body.push_str(
                "  No gateway chats yet. Message zorai from Telegram, Discord, Slack or WhatsApp first.\n",
            );
        }
        body.push_str("\n\u{2191}\u{2193} nav  Enter select  Esc close");
        body
    }

    pub(crate) fn gateway_link_modal_scroll(&self) -> usize {
        let total_lines = self.gateway_link_modal_body().lines().count();
        self.cursor_follow_scroll(modal::ModalKind::GatewayLink, 2, total_lines)
    }

    fn sync_gateway_link_modal_item_count(&mut self) {
        self.modal
            .set_picker_item_count(self.gateway_link_modal_actions().len());
    }

    pub(crate) fn open_gateway_link_modal(&mut self) {
        if self.active_daemon_handoff_thread_id().is_none() {
            return;
        }
        self.send_daemon_command(DaemonCommand::ListGatewayChats);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::GatewayLink));
        self.sync_gateway_link_modal_item_count();
        self.status_line = "Loading gateway chats...".to_string();
    }

    pub(crate) fn submit_gateway_link_modal(&mut self) {
        let action = self
            .gateway_link_modal_actions()
            .get(self.modal.picker_cursor())
            .cloned();
        let Some(action) = action else {
            self.status_line = "No gateway chat selected".to_string();
            return;
        };
        let Some(thread_id) = self.active_daemon_handoff_thread_id() else {
            return;
        };
        self.close_top_modal();
        let (channel_key, pending, status) = match action {
            GatewayLinkModalAction::Unlink { label } => (
                None,
                format!("Unlinked from {label}"),
                format!("Unlinking from {label}..."),
            ),
            GatewayLinkModalAction::Link { channel_key, label } => (
                Some(channel_key),
                format!("Linked to {label}"),
                format!("Linking to {label}..."),
            ),
        };
        self.send_daemon_command(DaemonCommand::LinkThreadToGatewayChat {
            thread_id,
            channel_key,
        });
        self.gateway_link_pending = Some(pending);
        self.status_line = status;
    }

    pub(crate) fn handle_gateway_chats(&mut self, chats: Vec<GatewayChatVm>) {
        self.chat.set_gateway_linked_thread_ids(
            chats
                .iter()
                .filter_map(|chat| chat.thread_id.clone())
                .collect(),
        );
        self.gateway_chats = chats;
        if self.modal.top() == Some(modal::ModalKind::GatewayLink) {
            self.sync_gateway_link_modal_item_count();
            self.status_line = "Select a gateway chat".to_string();
        }
        if let Some(message) = self.gateway_link_pending.take() {
            self.status_line = message.clone();
            self.show_input_notice(message, InputNoticeKind::Success, 120, false);
        }
    }

    pub(crate) fn handle_gateway_link_error(&mut self, message: &str) -> bool {
        if message.starts_with(GATEWAY_LIST_ERROR_PREFIX) {
            self.status_line = message.to_string();
            return true;
        }
        if self.gateway_link_pending.is_none() || !message.starts_with(GATEWAY_LINK_ERROR_PREFIX) {
            return false;
        }
        self.gateway_link_pending = None;
        self.last_error = Some(message.to_string());
        self.status_line = message.to_string();
        self.show_input_notice(message.to_string(), InputNoticeKind::Warning, 120, false);
        self.send_daemon_command(DaemonCommand::ListGatewayChats);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::chat;

    const CHATS_JSON: &str = r#"[
        {"channel_key":"Telegram:42","platform":"Telegram","channel_id":"42","thread_id":"thread-a","thread_title":"Refactor plan","last_thread_id":"tg-1","last_thread_title":"telegram alice","updated_at":10},
        {"channel_key":"Discord:7","platform":"Discord","channel_id":"7","thread_id":"thread-other","thread_title":"Other work","last_thread_id":"dc-1","last_thread_title":null,"updated_at":5},
        {"channel_key":"Slack:C1","platform":"Slack","channel_id":"C1","thread_id":null,"thread_title":null,"last_thread_id":"sl-1","last_thread_title":"slack bob","updated_at":1}
    ]"#;

    fn test_model() -> (
        TuiModel,
        tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
    ) {
        let (_event_tx, event_rx) = std::sync::mpsc::channel();
        let (daemon_tx, daemon_rx) = tokio::sync::mpsc::unbounded_channel();
        (TuiModel::new(event_rx, daemon_tx), daemon_rx)
    }

    fn chats() -> Vec<GatewayChatVm> {
        serde_json::from_str(CHATS_JSON).expect("daemon chat list shape parses")
    }

    fn select_thread(model: &mut TuiModel, thread_id: &str, title: &str) {
        model
            .chat
            .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
                id: thread_id.to_string(),
                title: title.to_string(),
                ..Default::default()
            }));
        model
            .chat
            .reduce(chat::ChatAction::SelectThread(thread_id.to_string()));
    }

    fn drain(rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>) -> Vec<DaemonCommand> {
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    #[test]
    fn chat_labels_read_like_the_gateway_contact_not_the_raw_key() {
        let chats = chats();
        assert_eq!(chats[0].thread_id.as_deref(), Some("thread-a"));
        assert_eq!(chats[2].thread_id, None);
        assert_eq!(gateway_chat_label(&chats[0]), "Telegram alice");
        assert_eq!(gateway_chat_label(&chats[1]), "Discord 7");
        assert_eq!(gateway_chat_label(&chats[2]), "Slack bob");
    }

    #[test]
    fn modal_offers_unlink_and_marks_where_each_chat_currently_lives() {
        let (mut model, _rx) = test_model();
        select_thread(&mut model, "thread-a", "Refactor plan");
        model.handle_gateway_chats(chats());

        assert_eq!(
            model.gateway_link_modal_actions().first(),
            Some(&GatewayLinkModalAction::Unlink {
                label: "Telegram alice".to_string()
            })
        );
        let body = model.gateway_link_modal_body();
        assert!(body.contains("Telegram alice  (\u{25cf} this thread)"));
        assert!(body.contains("Discord 7  (on: Other work)"));
        assert!(body.contains("Slack bob  (default)"));

        select_thread(&mut model, "thread-b", "Unlinked");
        assert!(matches!(
            model.gateway_link_modal_actions().first(),
            Some(GatewayLinkModalAction::Link { .. })
        ));
    }

    #[test]
    fn enter_links_the_active_daemon_thread_and_reports_success_when_list_returns() {
        let (mut model, mut rx) = test_model();
        select_thread(&mut model, "thread-b", "Unlinked");

        assert!(model.execute_slash_command_line("/gateway"));
        assert_eq!(model.modal.top(), Some(modal::ModalKind::GatewayLink));
        assert!(drain(&mut rx)
            .iter()
            .any(|cmd| matches!(cmd, DaemonCommand::ListGatewayChats)));

        model.handle_gateway_chats(chats());
        model.modal.set_picker_cursor(2);
        model.submit_gateway_link_modal();

        let sent = drain(&mut rx);
        assert!(
            sent.iter().any(|cmd| matches!(
                cmd,
                DaemonCommand::LinkThreadToGatewayChat { thread_id, channel_key }
                    if thread_id == "thread-b" && channel_key.as_deref() == Some("Slack:C1")
            )),
            "link must target the active thread's daemon id: {sent:?}"
        );
        assert_eq!(model.status_line, "Linking to Slack bob...");
        assert_eq!(model.modal.top(), None);

        let mut refreshed = chats();
        refreshed[2].thread_id = Some("thread-b".to_string());
        model.handle_gateway_chats(refreshed);
        assert_eq!(model.status_line, "Linked to Slack bob");
    }

    #[test]
    fn local_threads_cannot_be_linked_because_the_daemon_does_not_know_them() {
        let (mut model, mut rx) = test_model();
        select_thread(&mut model, "local-123", "Draft");

        assert!(model.execute_slash_command_line("/gateway"));

        assert_ne!(model.modal.top(), Some(modal::ModalKind::GatewayLink));
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn link_failure_is_shown_and_list_refreshed_without_disturbing_unrelated_errors() {
        let (mut model, mut rx) = test_model();
        let error = "Failed to link thread to gateway chat: linked thread to Telegram:42, but the handoff note failed to send: timeout";

        assert!(!model.handle_gateway_link_error(error));

        model.gateway_link_pending = Some("Linked to Telegram alice".to_string());
        assert!(!model.handle_gateway_link_error("provider exploded"));
        assert!(model.handle_gateway_link_error(error));
        assert_eq!(model.status_line, error);
        assert_eq!(model.gateway_link_pending, None);
        assert!(drain(&mut rx)
            .iter()
            .any(|cmd| matches!(cmd, DaemonCommand::ListGatewayChats)));
    }

    #[test]
    fn linked_threads_are_gateway_threads_even_without_a_gateway_title() {
        let (mut model, _rx) = test_model();
        select_thread(&mut model, "thread-a", "Refactor plan");
        let thread = model.chat.active_thread().cloned().expect("thread");
        assert!(!crate::widgets::thread_picker::is_gateway_thread(
            &thread,
            &model.chat
        ));

        model.handle_gateway_chats(chats());
        assert!(crate::widgets::thread_picker::is_gateway_thread(
            &thread,
            &model.chat
        ));

        model.handle_gateway_chats(Vec::new());
        assert!(!crate::widgets::thread_picker::is_gateway_thread(
            &thread,
            &model.chat
        ));
    }
}
