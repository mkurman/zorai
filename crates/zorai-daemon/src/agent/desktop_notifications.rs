use super::*;

const DESKTOP_NOTIFICATION_APP_NAME: &str = "Zorai";
const DESKTOP_NOTIFICATION_BODY_MAX_CHARS: usize = 240;
const NON_OPERATOR_THREAD_PREFIXES: &[&str] = &[
    "dm:",
    "handoff:",
    "goal:",
    "playground:",
    "workspace-thread:",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::agent) struct DesktopNotification {
    pub title: String,
    pub body: String,
}

impl AgentEngine {
    pub(in crate::agent) async fn run_desktop_notification_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut events = self.event_tx.subscribe();
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(event) => {
                        if !self.config.read().await.desktop_notifications {
                            continue;
                        }
                        if let Some(notification) = self.desktop_notification_for_event(&event).await {
                            show_desktop_notification(notification);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "desktop notification listener lagged behind agent events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                _ = shutdown.changed() => break,
            }
        }
    }

    pub(in crate::agent) async fn desktop_notification_for_event(
        &self,
        event: &AgentEvent,
    ) -> Option<DesktopNotification> {
        match event {
            AgentEvent::ApprovalRequired {
                thread_id,
                command,
                risk_level,
                ..
            } => Some(DesktopNotification {
                title: format!(
                    "Approval needed ({risk_level}) · {}",
                    self.desktop_notification_thread_title(thread_id).await
                ),
                body: truncate_notification_body(command),
            }),
            AgentEvent::OperatorQuestion {
                content, thread_id, ..
            } => {
                let title = match thread_id {
                    Some(thread_id) => format!(
                        "Question · {}",
                        self.desktop_notification_thread_title(thread_id).await
                    ),
                    None => "Question from an agent".to_string(),
                };
                Some(DesktopNotification {
                    title,
                    body: truncate_notification_body(content),
                })
            }
            AgentEvent::Done {
                thread_id,
                message_id,
                ..
            } => {
                self.desktop_notification_for_reply(thread_id, message_id.as_deref())
                    .await
            }
            _ => None,
        }
    }

    async fn desktop_notification_for_reply(
        &self,
        thread_id: &str,
        message_id: Option<&str>,
    ) -> Option<DesktopNotification> {
        let lowered = thread_id.to_ascii_lowercase();
        if NON_OPERATOR_THREAD_PREFIXES
            .iter()
            .any(|prefix| lowered.starts_with(prefix))
        {
            return None;
        }
        if self
            .tasks
            .lock()
            .await
            .iter()
            .any(|task| task.thread_id.as_deref() == Some(thread_id))
        {
            return None;
        }
        let threads = self.threads.read().await;
        let thread = threads.get(thread_id)?;
        if crate::agent::concierge::is_heartbeat_thread(thread)
            || thread
                .agent_name
                .as_deref()
                .is_some_and(crate::agent::agent_identity::is_weles_agent_scope)
        {
            return None;
        }
        let reply = match message_id {
            Some(message_id) => thread
                .messages
                .iter()
                .rev()
                .find(|message| message.id == message_id)?,
            None => thread
                .messages
                .iter()
                .rev()
                .find(|message| message.role == MessageRole::Assistant)?,
        };
        if reply.role != MessageRole::Assistant || reply.content.trim().is_empty() {
            return None;
        }
        let author = reply
            .author_agent_name
            .as_deref()
            .or(thread.agent_name.as_deref())
            .unwrap_or("Agent");
        Some(DesktopNotification {
            title: format!("{author} · {}", thread.title),
            body: truncate_notification_body(&reply.content),
        })
    }

    async fn desktop_notification_thread_title(&self, thread_id: &str) -> String {
        self.threads
            .read()
            .await
            .get(thread_id)
            .map(|thread| thread.title.clone())
            .unwrap_or_else(|| thread_id.to_string())
    }
}

fn truncate_notification_body(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= DESKTOP_NOTIFICATION_BODY_MAX_CHARS {
        return collapsed;
    }
    let mut truncated = collapsed
        .chars()
        .take(DESKTOP_NOTIFICATION_BODY_MAX_CHARS)
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn show_desktop_notification(notification: DesktopNotification) {
    tokio::task::spawn_blocking(move || {
        if let Err(error) = notify_rust::Notification::new()
            .appname(DESKTOP_NOTIFICATION_APP_NAME)
            .summary(&notification.title)
            .body(&notification.body)
            .show()
        {
            tracing::warn!(error = %error, title = %notification.title, "desktop notification failed");
        }
    });
}

#[cfg(test)]
#[path = "tests/desktop_notifications.rs"]
mod tests;
