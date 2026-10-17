use super::*;

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct GatewayChatInfo {
    pub channel_key: String,
    pub platform: String,
    pub channel_id: String,
    pub thread_id: Option<String>,
    pub thread_title: Option<String>,
    pub last_thread_id: String,
    pub last_thread_title: Option<String>,
    pub updated_at: u64,
}

pub(super) fn gateway_link_note(title: &str, agent_name: &str, last_reply: Option<&str>) -> String {
    let mut note = format!("Continuing zorai thread \u{201c}{title}\u{201d} here with {agent_name}.");
    if let Some(reply) = last_reply.map(str::trim).filter(|reply| !reply.is_empty()) {
        let snippet: String = reply.chars().take(300).collect();
        let ellipsis = if reply.chars().count() > 300 { "\u{2026}" } else { "" };
        note.push_str(&format!("\nLast reply: {snippet}{ellipsis}"));
    }
    note.push_str("\nSend !new to go back to a fresh conversation.");
    note
}

impl AgentEngine {
    pub async fn list_gateway_chats(&self) -> Result<Vec<GatewayChatInfo>> {
        let rows = self.history.list_gateway_chat_history().await?;
        let bindings = self.gateway_threads.read().await.clone();
        let mut chats = Vec::with_capacity(rows.len());
        for (channel_key, last_thread_id, updated_at, _) in rows {
            let Some((platform, channel_id)) = channel_key.split_once(':') else {
                continue;
            };
            let thread_id = bindings.get(&channel_key).cloned();
            let thread_title = match thread_id.as_deref() {
                Some(id) => self.gateway_chat_thread_title(id).await,
                None => None,
            };
            let last_thread_title = self.gateway_chat_thread_title(&last_thread_id).await;
            chats.push(GatewayChatInfo {
                platform: platform.to_string(),
                channel_id: channel_id.to_string(),
                channel_key,
                thread_id,
                thread_title,
                last_thread_id,
                last_thread_title,
                updated_at,
            });
        }
        Ok(chats)
    }

    async fn gateway_chat_thread_title(&self, thread_id: &str) -> Option<String> {
        if let Some(thread) = self.threads.read().await.get(thread_id) {
            return Some(thread.title.clone());
        }
        self.history
            .get_thread(thread_id)
            .await
            .ok()
            .flatten()
            .map(|thread| thread.title)
    }

    pub async fn link_thread_to_gateway_chat(
        &self,
        thread_id: &str,
        channel_key: Option<&str>,
    ) -> Result<()> {
        let thread = self
            .get_thread(thread_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("thread {thread_id} not found"))?;
        if let Some(channel_key) = channel_key {
            let known = self
                .history
                .list_gateway_chat_history()
                .await?
                .into_iter()
                .any(|(key, _, _, _)| key == channel_key);
            if !known {
                anyhow::bail!("gateway chat {channel_key} has not messaged zorai yet");
            }
        }

        let previous_keys: Vec<String> = self
            .gateway_threads
            .read()
            .await
            .iter()
            .filter(|(key, mapped)| mapped.as_str() == thread_id && Some(key.as_str()) != channel_key)
            .map(|(key, _)| key.clone())
            .collect();
        for key in &previous_keys {
            self.gateway_threads.write().await.remove(key);
            self.gateway_route_modes.write().await.remove(key);
            self.history.delete_gateway_thread_binding(key).await?;
            self.history.delete_gateway_route_mode(key).await?;
        }

        let Some(channel_key) = channel_key else {
            return Ok(());
        };
        let (platform, channel) = channel_key
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid gateway chat key {channel_key}"))?;

        let owner = self
            .active_agent_id_for_thread(thread_id)
            .await
            .unwrap_or_else(|| crate::agent::agent_identity::MAIN_AGENT_ID.to_string());
        let route_mode = if crate::agent::agent_identity::canonical_agent_id(&owner)
            == crate::agent::agent_identity::CONCIERGE_AGENT_ID
        {
            gateway::GatewayRouteMode::Rarog
        } else {
            gateway::GatewayRouteMode::Swarog
        };
        let now = now_millis();
        self.gateway_threads
            .write()
            .await
            .insert(channel_key.to_string(), thread_id.to_string());
        self.history
            .upsert_gateway_thread_binding(channel_key, thread_id, now)
            .await?;
        self.gateway_route_modes
            .write()
            .await
            .insert(channel_key.to_string(), route_mode);
        self.history
            .upsert_gateway_route_mode(channel_key, route_mode.as_str(), now)
            .await?;

        let last_reply = thread
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::Assistant && !message.content.trim().is_empty())
            .map(|message| message.content.clone());
        let note = gateway_link_note(
            &thread.title,
            &crate::agent::agent_identity::agent_display_name(&owner),
            last_reply.as_deref(),
        );
        let target = gateway::IncomingMessage {
            platform: platform.to_string(),
            sender: String::new(),
            content: String::new(),
            channel: channel.to_string(),
            message_id: None,
            thread_context: None,
        };
        let (_, reply_tool_name) = gateway_reply_tool(platform, channel);
        let result = self
            .send_gateway_platform_tool(thread_id, "gateway_link", reply_tool_name, &target, &note)
            .await;
        if result.is_error {
            anyhow::bail!(
                "linked thread to {channel_key}, but the handoff note failed to send: {}",
                result.content
            );
        }
        Ok(())
    }
}
