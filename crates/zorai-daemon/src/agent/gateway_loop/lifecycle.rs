use super::*;

impl AgentEngine {
    #[cfg(test)]
    pub(crate) async fn set_gateway_init_test_delay(&self, delay: Duration) {
        *self.gateway_init_test_delay.lock().await = Some(delay);
    }

    pub(super) async fn persist_gateway_fast_path_exchange(
        &self,
        channel_key: &str,
        msg: &gateway::IncomingMessage,
        reply_text: &str,
    ) -> Result<String> {
        let existing_thread = self.gateway_threads.read().await.get(channel_key).cloned();
        let (thread_id, is_new_thread) = self
            .get_or_create_thread(existing_thread.as_deref(), &msg.content)
            .await;
        if is_new_thread {
            self.ensure_thread_identity(&thread_id, &gateway_thread_title(msg), false)
                .await;
        }

        {
            let mut threads = self.threads.write().await;
            let thread = threads
                .get_mut(&thread_id)
                .ok_or_else(|| anyhow::anyhow!("gateway thread missing after creation"))?;
            thread
                .messages
                .push(AgentMessage::user(msg.content.trim(), now_millis()));
            thread.updated_at = now_millis();
        }
        self.persist_thread_by_id(&thread_id).await;
        self.record_operator_message(&thread_id, &msg.content, is_new_thread)
            .await?;
        if let Err(error) = self.maybe_sync_thread_to_honcho(&thread_id).await {
            tracing::warn!(thread_id = %thread_id, error = %error, "failed to sync gateway thread to Honcho");
        }

        self.add_assistant_message(
            &thread_id,
            reply_text,
            0,
            0,
            None,
            Some("concierge".to_string()),
            None,
            None,
            None,
        )
        .await;

        self.gateway_threads
            .write()
            .await
            .insert(channel_key.to_string(), thread_id.clone());
        self.history
            .upsert_gateway_thread_binding(channel_key, &thread_id, now_millis())
            .await?;

        Ok(thread_id)
    }

    pub(crate) fn schedule_gateway_startup(self: &Arc<Self>) {
        let engine = self.clone();
        tokio::spawn(async move {
            engine.maybe_spawn_gateway().await;
        });
    }

    pub async fn enqueue_gateway_message(&self, msg: gateway::IncomingMessage) -> Result<()> {
        let enabled = {
            let guard = self.gateway_state.lock().await;
            guard
                .as_ref()
                .map(|state| state.config.enabled)
                .unwrap_or(false)
        };
        if !enabled {
            anyhow::bail!("gateway polling is not initialized");
        }

        let mut queue = self.gateway_injected_messages.lock().await;
        queue.push_back(msg);
        if queue.len() > 500 {
            let excess = queue.len() - 500;
            queue.drain(..excess);
        }
        Ok(())
    }

    pub(crate) async fn init_gateway(&self) {
        let _init_guard = self.gateway_init_lock.lock().await;
        if self.gateway_state.lock().await.is_some() {
            return;
        }

        #[cfg(test)]
        if let Some(delay) = *self.gateway_init_test_delay.lock().await {
            tokio::time::sleep(delay).await;
        }

        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        if !gw.enabled {
            tracing::info!("gateway: disabled in config, skipping initialization");
            return;
        }

        let slack_token = gw.slack_token.clone();
        let telegram_token = gw.telegram_token.clone();
        let discord_token = gw.discord_token.clone();
        let discord_channel_filter = gw.discord_channel_filter.clone();
        let slack_channel_filter = gw.slack_channel_filter.clone();

        let has_poll_tokens = !slack_token.is_empty()
            || !telegram_token.is_empty()
            || !discord_token.is_empty()
            || !gw.whatsapp_token.is_empty();
        if !has_poll_tokens {
            tracing::info!(
                "gateway: no platform poll tokens configured; enabling injected-only gateway mode"
            );
        }

        self.gateway_injected_messages.lock().await.clear();

        if !discord_channel_filter.is_empty() {
            tracing::info!(discord_filter = %discord_channel_filter, "gateway: discordChannelFilter");
            let channels: Vec<String> = discord_channel_filter
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            *self.gateway_discord_channels.write().await = channels;
        }
        if !slack_channel_filter.is_empty() {
            let channels: Vec<String> = slack_channel_filter
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            *self.gateway_slack_channels.write().await = channels;
        }

        let gw_config = GatewayConfig {
            enabled: true,
            slack_token,
            slack_channel_filter,
            telegram_token,
            telegram_allowed_chats: gw.telegram_allowed_chats.clone(),
            discord_token,
            discord_channel_filter,
            discord_allowed_users: gw.discord_allowed_users.clone(),
            whatsapp_allowed_contacts: gw.whatsapp_allowed_contacts.clone(),
            whatsapp_token: gw.whatsapp_token.clone(),
            whatsapp_phone_id: gw.whatsapp_phone_id.clone(),
            command_prefix: gw.command_prefix.clone(),
            default_agent: gw.default_agent.clone(),
            slack_default_agent: gw.slack_default_agent.clone(),
            discord_default_agent: gw.discord_default_agent.clone(),
            telegram_default_agent: gw.telegram_default_agent.clone(),
            whatsapp_default_agent: gw.whatsapp_default_agent.clone(),
            gateway_electron_bridges_enabled: gw.gateway_electron_bridges_enabled,
            whatsapp_link_fallback_electron: gw.whatsapp_link_fallback_electron,
        };

        let dc = self.gateway_discord_channels.read().await.clone();
        let sc = self.gateway_slack_channels.read().await.clone();

        tracing::info!(
            has_slack = !gw_config.slack_token.is_empty(),
            has_telegram = !gw_config.telegram_token.is_empty(),
            has_discord = !gw_config.discord_token.is_empty(),
            discord_channels = ?dc,
            slack_channels = ?sc,
            "gateway: config loaded"
        );

        let telegram_replay_cursor = match self
            .history
            .load_gateway_replay_cursor("telegram", "global")
            .await
        {
            Ok(Some(row)) => match row.cursor_value.parse::<i64>() {
                Ok(cursor) => Some(cursor),
                Err(error) => {
                    tracing::warn!(
                        channel_id = %row.channel_id,
                        cursor_value = %row.cursor_value,
                        %error,
                        "gateway: ignoring invalid telegram replay cursor"
                    );
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(%error, "gateway: failed to load telegram replay cursor");
                None
            }
        };
        let slack_replay_cursors = match self.history.load_gateway_replay_cursors("slack").await {
            Ok(rows) => rows
                .into_iter()
                .map(|row| (row.channel_id, row.cursor_value))
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "gateway: failed to load slack replay cursors");
                HashMap::new()
            }
        };
        let discord_replay_cursors = match self.history.load_gateway_replay_cursors("discord").await
        {
            Ok(rows) => rows
                .into_iter()
                .map(|row| (row.channel_id, row.cursor_value))
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "gateway: failed to load discord replay cursors");
                HashMap::new()
            }
        };
        let whatsapp_replay_cursors =
            match self.history.load_gateway_replay_cursors("whatsapp").await {
                Ok(rows) => rows
                    .into_iter()
                    .map(|row| (row.channel_id, row.cursor_value))
                    .collect(),
                Err(error) => {
                    tracing::warn!(%error, "gateway: failed to load whatsapp replay cursors");
                    HashMap::new()
                }
            };
        let health_snapshots = match self.history.list_gateway_health_snapshots().await {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "gateway: failed to load health snapshots");
                Vec::new()
            }
        };

        let mut gateway_state = gateway::GatewayState::new(gw_config, self.http_client.clone());
        gateway_state.telegram_replay_cursor = telegram_replay_cursor;
        gateway_state.slack_replay_cursors = slack_replay_cursors;
        gateway_state.discord_replay_cursors = discord_replay_cursors;
        gateway_state.whatsapp_replay_cursors = whatsapp_replay_cursors;
        for row in health_snapshots {
            match serde_json::from_str::<zorai_protocol::GatewayHealthState>(&row.state_json) {
                Ok(snapshot) => apply_health_snapshot(&mut gateway_state, &snapshot),
                Err(error) => {
                    tracing::warn!(
                        platform = %row.platform,
                        %error,
                        "gateway: ignoring invalid health snapshot"
                    );
                }
            }
        }

        *self.gateway_state.lock().await = Some(gateway_state);
        tracing::info!("gateway: runtime state initialized in daemon");
    }

    pub async fn reinit_gateway(&self) -> Option<String> {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        if !gw.enabled {
            tracing::info!("gateway: disabled by config, clearing state");
            *self.gateway_state.lock().await = None;
            *self.gateway_discord_channels.write().await = Vec::new();
            *self.gateway_slack_channels.write().await = Vec::new();
            self.stop_gateway().await;
            return None;
        }

        *self.gateway_state.lock().await = None;
        *self.gateway_discord_channels.write().await = Vec::new();
        *self.gateway_slack_channels.write().await = Vec::new();

        let mut degraded_reason = None;

        match self
            .request_gateway_reload(Some("config reloaded".to_string()))
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                degraded_reason = Some(
                    "gateway reload command not delivered because runtime is not connected"
                        .to_string(),
                );
            }
            Err(error) => {
                tracing::debug!(error = %error, "gateway: reload command not delivered");
                degraded_reason = Some(format!("gateway reload command not delivered: {error}"));
            }
        }

        self.maybe_spawn_gateway().await;
        degraded_reason
    }

    pub(crate) async fn set_gateway_ipc_sender(
        &self,
        sender: Option<mpsc::UnboundedSender<zorai_protocol::DaemonMessage>>,
    ) {
        *self.gateway_ipc_sender.lock().await = sender;
    }

    pub(crate) async fn clear_gateway_ipc_sender(&self) {
        *self.gateway_ipc_sender.lock().await = None;
        self.gateway_pending_send_results.lock().await.clear();
    }

    pub(crate) async fn request_gateway_send(
        &self,
        request: zorai_protocol::GatewaySendRequest,
    ) -> Result<zorai_protocol::GatewaySendResult> {
        let correlation_id = request.correlation_id.clone();
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        let sender = {
            let Some(sender) = self.gateway_ipc_sender.lock().await.clone() else {
                return Err(anyhow::anyhow!(
                    "standalone gateway runtime is not connected"
                ));
            };
            let mut pending = self.gateway_pending_send_results.lock().await;
            if pending.contains_key(&correlation_id) {
                return Err(anyhow::anyhow!(
                    "duplicate gateway send correlation id: {correlation_id}"
                ));
            }
            pending.insert(correlation_id.clone(), result_tx);
            sender
        };

        if let Err(error) =
            sender.send(zorai_protocol::DaemonMessage::GatewaySendRequest { request })
        {
            self.gateway_pending_send_results
                .lock()
                .await
                .remove(&correlation_id);
            return Err(anyhow::anyhow!(
                "failed to deliver gateway send request: {error}"
            ));
        }

        match tokio::time::timeout(
            std::time::Duration::from_secs(GATEWAY_SEND_RESULT_TIMEOUT_SECS),
            result_rx,
        )
        .await
        {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => Err(anyhow::anyhow!(
                "gateway send request was interrupted before a result arrived"
            )),
            Err(_) => {
                self.gateway_pending_send_results
                    .lock()
                    .await
                    .remove(&correlation_id);
                Err(anyhow::anyhow!("timed out waiting for gateway send result"))
            }
        }
    }

    pub(crate) async fn complete_gateway_send_result(
        &self,
        result: zorai_protocol::GatewaySendResult,
    ) -> bool {
        let waiter = self
            .gateway_pending_send_results
            .lock()
            .await
            .remove(&result.correlation_id);
        if let Some(waiter) = waiter {
            let _ = waiter.send(result);
            true
        } else {
            false
        }
    }

    pub(crate) async fn gateway_health_snapshots(&self) -> Vec<zorai_protocol::GatewayHealthState> {
        let gw = self.gateway_state.lock().await;
        let Some(state) = gw.as_ref() else {
            return Vec::new();
        };

        vec![
            snapshot_from_platform_health("slack", &state.slack_health),
            snapshot_from_platform_health("discord", &state.discord_health),
            snapshot_from_platform_health("telegram", &state.telegram_health),
        ]
    }
}
