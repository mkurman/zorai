use super::*;
use zorai_protocol::AGENT_NAME_SWAROG;

/// Max messages held per channel while its turn is in flight before the oldest
/// are dropped (matches the bounded-queue policy on `gateway_injected_messages`).
const GATEWAY_FOLLOWUP_CAP: usize = 200;

impl AgentEngine {
    async fn process_single_gateway_message(self: &Arc<Self>, msg: gateway::IncomingMessage) {
        if let Some(ref mid) = msg.message_id {
            let mut seen = self.gateway_seen_ids.lock().await;
            if seen.contains(mid) {
                tracing::debug!(
                    message_id = %mid,
                    platform = %msg.platform,
                    "gateway: skipping duplicate message"
                );
                return;
            }
            seen.push(mid.clone());
            if seen.len() > 200 {
                let excess = seen.len() - 200;
                seen.drain(..excess);
            }
        }

        let channel_key = gateway::gateway_channel_key(&msg.platform, &msg.channel);
        {
            // Lock order: follow-ups before in-flight, matching the drain at
            // the end of the spawned turn loop, so a message that arrives just
            // as a turn finishes is never orphaned.
            let mut followups = self.gateway_pending_followups.lock().await;
            let mut inflight = self.gateway_inflight_channels.lock().await;
            if inflight.contains(&channel_key)
                && is_gateway_reset_command(&msg.content.trim().to_lowercase())
            {
                drop(inflight);
                drop(followups);
                self.handle_gateway_reset_command(&msg, &channel_key).await;
                return;
            }
            if inflight.contains(&channel_key) {
                let queue = followups.entry(channel_key.clone()).or_default();
                queue.push_back(msg);
                if queue.len() > GATEWAY_FOLLOWUP_CAP {
                    let excess = queue.len() - GATEWAY_FOLLOWUP_CAP;
                    queue.drain(..excess);
                    tracing::warn!(
                        channel_key = %channel_key,
                        excess,
                        "gateway: follow-up queue exceeded cap, dropped oldest pending messages"
                    );
                } else {
                    tracing::info!(
                        channel_key = %channel_key,
                        "gateway: channel busy, queued message to inject after current turn"
                    );
                }
                return;
            }
            inflight.insert(channel_key.clone());
        }

        let engine = Arc::clone(self);
        tokio::spawn(async move {
            let mut current = msg;
            loop {
                engine
                    .run_gateway_message_pipeline(current, &channel_key)
                    .await;
                let next = {
                    let mut followups = engine.gateway_pending_followups.lock().await;
                    let mut inflight = engine.gateway_inflight_channels.lock().await;
                    match followups
                        .get_mut(&channel_key)
                        .and_then(VecDeque::pop_front)
                    {
                        Some(next) => Some(next),
                        None => {
                            followups.remove(&channel_key);
                            inflight.remove(&channel_key);
                            None
                        }
                    }
                };
                match next {
                    Some(next) => current = next,
                    None => break,
                }
            }
        });
    }

    async fn run_gateway_message_pipeline(&self, msg: gateway::IncomingMessage, channel_key: &str) {
        let channel_key = channel_key.to_string();
        tracing::info!(
            platform = %msg.platform,
            sender = %msg.sender,
            channel = %msg.channel,
            content = %msg.content,
            message_id = ?msg.message_id,
            "gateway: incoming message"
        );

        if self.handle_gateway_reset_command(&msg, &channel_key).await {
            return;
        }

        let (_reply_tool, reply_tool_name) = gateway_reply_tool(&msg.platform, &msg.channel);
        let route_request = classify_gateway_route_request(&msg.content);
        if let Some(request) = route_request {
            self.persist_gateway_route_request(&channel_key, request)
                .await;
            if self
                .handle_gateway_route_switch_ack(&msg, &channel_key, request, reply_tool_name)
                .await
            {
                return;
            }
        }

        let existing_thread = self.gateway_threads.read().await.get(&channel_key).cloned();
        if let Some(command) = parse_gateway_control_command(&msg.content) {
            if Box::pin(self.handle_gateway_control_command(
                command,
                &msg,
                &channel_key,
                existing_thread.as_deref(),
                reply_tool_name,
            ))
            .await
            {
                return;
            }
        }

        if Box::pin(self.handle_gateway_task_approval_reply(
            &msg,
            &channel_key,
            existing_thread.as_deref(),
            reply_tool_name,
        ))
        .await
        {
            return;
        }

        let _ = self.event_tx.send(AgentEvent::GatewayIncoming {
            platform: msg.platform.clone(),
            sender: msg.sender.clone(),
            content: msg.content.clone(),
            channel: msg.channel.clone(),
            thread_id: existing_thread.clone(),
        });
        let (run_concierge_triage, full_agent_scope): (bool, Option<String>) =
            if let Some(request) = route_request {
                (request.mode == gateway::GatewayRouteMode::Rarog, None)
            } else if let Some(mode) = self
                .gateway_route_modes
                .read()
                .await
                .get(&channel_key)
                .copied()
            {
                (mode == gateway::GatewayRouteMode::Rarog, None)
            } else {
                self.resolve_gateway_default_responder(&msg.platform).await
            };
        let history_window: Option<String> = self
            .load_gateway_history_window(&channel_key, existing_thread.as_deref())
            .await;

        if run_concierge_triage {
            match self
                .triage_gateway_message(&msg, history_window.as_deref(), existing_thread.as_deref())
                .await
            {
                concierge::GatewayTriage::Simple(response_text) => {
                    if self
                        .handle_concierge_simple_response(
                            &msg,
                            &channel_key,
                            reply_tool_name,
                            &response_text,
                        )
                        .await
                    {
                        return;
                    }
                }
                concierge::GatewayTriage::Complex => {}
            }
        } else {
            tracing::info!(
                platform = %msg.platform,
                channel = %msg.channel,
                responder = full_agent_scope.as_deref().unwrap_or(AGENT_NAME_SWAROG),
                "gateway: bypassing concierge triage; routing to configured responder",
            );
        }

        self.handle_gateway_full_agent_response(
            &msg,
            &channel_key,
            existing_thread.as_deref(),
            history_window.as_deref(),
            reply_tool_name,
            full_agent_scope.as_deref(),
        )
        .await;
    }

    pub(super) async fn process_gateway_messages(self: &Arc<Self>) {
        let now_ms = now_millis();
        let incoming = {
            let mut gw_guard = self.gateway_state.lock().await;
            let Some(gw) = gw_guard.as_mut() else {
                return;
            };

            let mut queue = self.gateway_injected_messages.lock().await;
            let incoming: Vec<gateway::IncomingMessage> = queue.drain(..).collect();
            if incoming.is_empty() {
                return;
            }

            for msg in &incoming {
                let key = gateway::gateway_channel_key(&msg.platform, &msg.channel);
                gw.last_incoming_at.insert(key.clone(), now_ms);
                if let Some(ref tc) = msg.thread_context {
                    gw.reply_contexts.insert(key, tc.clone());
                }
            }
            incoming
        };

        for msg in incoming {
            self.process_single_gateway_message(msg).await;
        }
    }
}
