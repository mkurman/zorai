use super::*;

impl AgentEngine {
    fn workspace_task_status_label(status: zorai_protocol::WorkspaceTaskStatus) -> &'static str {
        match status {
            zorai_protocol::WorkspaceTaskStatus::Todo => "todo",
            zorai_protocol::WorkspaceTaskStatus::InProgress => "in progress",
            zorai_protocol::WorkspaceTaskStatus::InReview => "in review",
            zorai_protocol::WorkspaceTaskStatus::Done => "done",
        }
    }

    fn goal_run_matches_thread(goal_run: &GoalRun, thread_id: &str) -> bool {
        goal_run.thread_id.as_deref() == Some(thread_id)
            || goal_run.root_thread_id.as_deref() == Some(thread_id)
            || goal_run.active_thread_id.as_deref() == Some(thread_id)
            || goal_run
                .execution_thread_ids
                .iter()
                .any(|id| id == thread_id)
    }

    fn goal_run_status_message_for_status(status: GoalRunStatus) -> &'static str {
        match status {
            GoalRunStatus::Queued => "Goal queued",
            GoalRunStatus::Planning => "Goal planning",
            GoalRunStatus::Running => "Goal running",
            GoalRunStatus::AwaitingApproval => "Goal awaiting approval",
            GoalRunStatus::AwaitingReview => "Goal awaiting supervisor review",
            GoalRunStatus::Paused => "Goal paused",
            GoalRunStatus::Blocked => "Goal blocked",
            GoalRunStatus::Completed => "Goal completed",
            GoalRunStatus::Failed => "Goal failed",
            GoalRunStatus::Cancelled => "Goal cancelled",
            GoalRunStatus::Contained => "Goal contained for review",
            GoalRunStatus::Compensated => "Goal compensated",
            GoalRunStatus::PartiallyCompensated => "Goal partially compensated",
            GoalRunStatus::BreakGlass => "Goal completed under break-glass override",
        }
    }

    fn extract_gateway_approval_id_from_message(content: &str) -> Option<String> {
        let (_, remainder) = content.split_once("Approval ID:")?;
        remainder
            .trim()
            .split_whitespace()
            .next()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    fn gateway_approval_request_text(&self, pending_approval: &ToolPendingApproval) -> String {
        format!(
            "Approval required for a pending task.\nApproval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}\n\nReply with one of: approve-once | approve-session | deny",
            pending_approval.approval_id,
            pending_approval.risk_level,
            pending_approval.blast_radius,
            pending_approval.command,
            pending_approval.reasons.join("\n- "),
        )
    }

    fn gateway_stale_approval_reply_text(&self) -> &'static str {
        "That approval reply is stale or from the wrong chat/thread. Please reply from the same channel/thread where the approval prompt was sent, or ask me to resend the approval request."
    }

    async fn send_gateway_stale_approval_reply(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        reply_tool_name: &str,
    ) -> bool {
        let response_text = self.gateway_stale_approval_reply_text();
        let thread_id = match self
            .persist_gateway_fast_path_exchange(channel_key, msg, response_text)
            .await
        {
            Ok(thread_id) => thread_id,
            Err(error) => {
                tracing::error!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    %error,
                    "gateway: failed to persist stale approval reply exchange"
                );
                return true;
            }
        };

        let tool_result = self
            .send_gateway_platform_tool(
                &thread_id,
                "gateway_approval_stale",
                reply_tool_name,
                msg,
                response_text,
            )
            .await;
        if tool_result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send stale approval rejection"
            );
        }
        true
    }

    pub(in crate::agent) async fn maybe_send_gateway_thread_approval_request(
        &self,
        thread_id: &str,
        pending_approval: &ToolPendingApproval,
    ) -> bool {
        let Some(msg) = self.gateway_message_target_for_thread(thread_id).await else {
            return false;
        };

        let (_, reply_tool_name) = gateway_reply_tool(&msg.platform, &msg.channel);
        let response_text = self.gateway_approval_request_text(pending_approval);
        self.add_assistant_message(
            thread_id,
            &response_text,
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;
        self.persist_thread_by_id(thread_id).await;

        let tool_result = self
            .send_gateway_platform_tool(
                thread_id,
                "gateway_approval_prompt",
                reply_tool_name,
                &msg,
                &response_text,
            )
            .await;

        if tool_result.is_error {
            tracing::error!(
                thread_id = %thread_id,
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send approval prompt"
            );
            return false;
        }

        true
    }

    pub(super) async fn gateway_pending_approval_for_thread(
        &self,
        thread_id: &str,
    ) -> Option<String> {
        match self
            .history
            .latest_agent_task_approval_id_for_thread(thread_id)
            .await
        {
            Ok(Some(approval_id)) => return Some(approval_id),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "gateway: failed to query pending task approval id"
                );
            }
        }

        {
            let tasks = self.tasks.lock().await;
            if let Some(approval_id) = tasks
                .iter()
                .filter(|task| task.thread_id.as_deref() == Some(thread_id))
                .find_map(|task| task.awaiting_approval_id.clone())
            {
                return Some(approval_id);
            }
        }

        if let Some(approval_id) = self
            .critique_approval_continuations
            .lock()
            .await
            .iter()
            .find(|(_, continuation)| continuation.thread_id == thread_id)
            .map(|(approval_id, _)| approval_id.clone())
        {
            return Some(approval_id);
        }

        let approval_ids = match self
            .history
            .gateway_approval_ids_for_thread(thread_id)
            .await
        {
            Ok(approval_ids) if !approval_ids.is_empty() => approval_ids,
            other => {
                if let Err(error) = &other {
                    tracing::warn!(
                        thread_id = %thread_id,
                        %error,
                        "gateway: failed to query persisted approval ids; falling back to live thread messages"
                    );
                }
                let threads = self.threads.read().await;
                threads
                    .get(thread_id)
                    .map(|thread| {
                        thread
                            .messages
                            .iter()
                            .rev()
                            .filter_map(|message| {
                                Self::extract_gateway_approval_id_from_message(&message.content)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            }
        };
        if approval_ids.is_empty() {
            return None;
        }

        let pending = self.pending_operator_approvals.read().await;
        approval_ids
            .into_iter()
            .find(|approval_id| pending.contains_key(approval_id))
    }

    async fn gateway_has_pending_approval_anywhere(&self) -> bool {
        match self.history.has_agent_task_pending_approval().await {
            Ok(true) => return true,
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "gateway: failed to query task-owned pending approvals"
                );
            }
        }

        // Always also consult the in-memory task queue, not only on the
        // history-query Err branch. The live queue is the source of truth
        // for approvals that arrived this session — history catches up
        // via background persistence and may lag. Without this in-memory
        // check, a freshly-created awaiting-approval task is invisible to
        // cross-platform/cross-thread stale-reply rejection until the
        // next persist cycle.
        {
            let tasks = self.tasks.lock().await;
            if tasks.iter().any(|task| task.awaiting_approval_id.is_some()) {
                return true;
            }
        }

        if !self.critique_approval_continuations.lock().await.is_empty() {
            return true;
        }

        !self.pending_operator_approvals.read().await.is_empty()
    }

    async fn gateway_resolve_thread_approval(
        &self,
        approval_id: &str,
        decision: zorai_protocol::ApprovalDecision,
    ) -> bool {
        if self
            .session_manager
            .resolve_approval_by_id(approval_id, decision)
            .await
            .is_ok()
        {
            let _ = self
                .record_operator_approval_resolution(approval_id, decision)
                .await;
            return true;
        }

        if self
            .handle_task_approval_resolution(approval_id, decision)
            .await
        {
            let _ = self
                .record_operator_approval_resolution(approval_id, decision)
                .await;
            return true;
        }

        self.resume_critique_approval_continuation(
            approval_id,
            decision,
            &self.session_manager,
            &self.event_tx,
            &self.data_dir,
            &self.http_client,
        )
        .await
        .is_ok()
    }

    fn gateway_approval_confirmation_text(
        &self,
        decision: zorai_protocol::ApprovalDecision,
    ) -> &'static str {
        match decision {
            zorai_protocol::ApprovalDecision::ApproveOnce => {
                "Approved once. I resumed the pending task."
            }
            zorai_protocol::ApprovalDecision::ApproveSession => {
                "Approved for this session. I resumed the pending task."
            }
            zorai_protocol::ApprovalDecision::Deny => "Denied. I rejected the pending task.",
        }
    }

    async fn gateway_status_reply_text(&self, thread_id: Option<&str>) -> String {
        let Some(thread_id) = thread_id else {
            return "No active work is bound to this chat yet.".to_string();
        };

        if let Ok(Some(task)) = self
            .history
            .get_workspace_task_by_thread_id(thread_id)
            .await
        {
            let task = match self.sync_workspace_task_runtime_state(task).await {
                Ok(task) => task,
                Err(_) => match self
                    .history
                    .get_workspace_task_by_thread_id(thread_id)
                    .await
                {
                    Ok(Some(task)) => task,
                    _ => {
                        return "No active work is bound to this chat right now.".to_string();
                    }
                },
            };

            if task.status != zorai_protocol::WorkspaceTaskStatus::Done {
                return format!(
                    "Workspace task {} ({}) is {}.",
                    task.title,
                    task.id,
                    Self::workspace_task_status_label(task.status)
                );
            }
        }

        let mut goal_runs = match self
            .history
            .latest_goal_run_status_reply_ref_for_thread_ids(&[thread_id.to_string()])
            .await
        {
            Ok(Some(goal_run)) => vec![goal_run],
            Ok(None) => Vec::new(),
            Err(error) => {
                tracing::warn!(
                    thread_id,
                    "gateway: failed to query latest persisted goal run status reply fields: {error}"
                );
                Vec::new()
            }
        };
        let mut seen_goal_ids = goal_runs
            .iter()
            .map(|goal_run| goal_run.id.clone())
            .collect::<std::collections::HashSet<_>>();
        {
            let live_goal_runs = self.goal_runs.lock().await;
            for goal_run in live_goal_runs
                .iter()
                .filter(|goal_run| Self::goal_run_matches_thread(goal_run, thread_id))
            {
                if seen_goal_ids.insert(goal_run.id.clone()) {
                    goal_runs.push(crate::history::GoalRunStatusReplyRef::from(goal_run));
                }
            }
        }
        let goal_run = goal_runs
            .into_iter()
            .max_by_key(|goal_run| goal_run.updated_at);

        if let Some(goal_run) = goal_run {
            let mut response = format!(
                "{} ({}) — {}.",
                goal_run.title,
                goal_run.id,
                Self::goal_run_status_message_for_status(goal_run.status)
            );

            if let Some(step_title) = goal_run
                .current_step_title
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                response.push_str(&format!(" Current step: {step_title}."));
            } else if let Some(summary) = goal_run
                .plan_summary
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                response.push_str(&format!(" Summary: {summary}."));
            }

            return response;
        }

        "No active work is bound to this chat right now.".to_string()
    }

    pub(super) async fn handle_gateway_control_command(
        &self,
        command: GatewayControlCommand,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        existing_thread: Option<&str>,
        reply_tool_name: &str,
    ) -> bool {
        let response_text = match command {
            GatewayControlCommand::Status => self.gateway_status_reply_text(existing_thread).await,
            GatewayControlCommand::Pause => {
                "Pause is not available from chat for this thread yet.".to_string()
            }
            GatewayControlCommand::Resume => {
                "Resume is not available from chat for this thread yet.".to_string()
            }
            GatewayControlCommand::Rerun => {
                "Rerun is not available from chat for this thread yet.".to_string()
            }
        };

        let thread_id = match self
            .persist_gateway_fast_path_exchange(channel_key, msg, &response_text)
            .await
        {
            Ok(thread_id) => thread_id,
            Err(error) => {
                tracing::error!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    %error,
                    "gateway: failed to persist control-command exchange"
                );
                return true;
            }
        };

        let tool_result = self
            .send_gateway_platform_tool(
                &thread_id,
                "gateway_control",
                reply_tool_name,
                msg,
                &response_text,
            )
            .await;
        if tool_result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send control-command response"
            );
        }

        true
    }

    pub(in crate::agent) async fn handle_gateway_task_approval_reply(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        existing_thread: Option<&str>,
        reply_tool_name: &str,
    ) -> bool {
        let Some(decision) = parse_gateway_approval_decision(&msg.content) else {
            return false;
        };

        let Some(thread_id) = existing_thread else {
            return self
                .send_gateway_stale_approval_reply(msg, channel_key, reply_tool_name)
                .await;
        };

        let approval_target_mismatch = {
            let gateway_threads = self.gateway_threads.read().await;
            gateway_threads
                .get(channel_key)
                .map(|mapped| mapped != thread_id)
                .unwrap_or(true)
        };
        if approval_target_mismatch {
            return self
                .send_gateway_stale_approval_reply(msg, channel_key, reply_tool_name)
                .await;
        }

        let Some(approval_id) = self.gateway_pending_approval_for_thread(thread_id).await else {
            if self.gateway_has_pending_approval_anywhere().await {
                return self
                    .send_gateway_stale_approval_reply(msg, channel_key, reply_tool_name)
                    .await;
            }
            return false;
        };

        if !self
            .gateway_resolve_thread_approval(&approval_id, decision)
            .await
        {
            return false;
        }

        let response_text = self.gateway_approval_confirmation_text(decision);
        let thread_id = match self
            .persist_gateway_fast_path_exchange(channel_key, msg, response_text)
            .await
        {
            Ok(thread_id) => thread_id,
            Err(error) => {
                tracing::error!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    %error,
                    "gateway: failed to persist approval reply exchange"
                );
                return true;
            }
        };

        let tool_result = self
            .send_gateway_platform_tool(
                &thread_id,
                "gateway_approval",
                reply_tool_name,
                msg,
                response_text,
            )
            .await;
        if tool_result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send approval confirmation"
            );
        }
        true
    }

    pub(super) async fn send_gateway_platform_tool(
        &self,
        _thread_id: &str,
        tool_id_prefix: &str,
        reply_tool_name: &str,
        msg: &gateway::IncomingMessage,
        response_text: &str,
    ) -> ToolResult {
        let tool_call_id = format!("{tool_id_prefix}_{}", uuid::Uuid::new_v4());
        let args = gateway_reply_args(&msg.platform, &msg.channel, response_text);
        match crate::agent::tool_executor::execute_gateway_message(
            reply_tool_name,
            &args,
            self,
            &self.http_client,
        )
        .await
        {
            Ok(content) => ToolResult {
                tool_call_id,
                name: reply_tool_name.to_string(),
                content,
                is_error: false,
                weles_review: None,
                pending_approval: None,
            },
            Err(error) => ToolResult {
                tool_call_id,
                name: reply_tool_name.to_string(),
                content: error.to_string(),
                is_error: true,
                weles_review: None,
                pending_approval: None,
            },
        }
    }

    async fn gateway_message_target_for_thread(
        &self,
        thread_id: &str,
    ) -> Option<gateway::IncomingMessage> {
        let channel_key = {
            let gateway_threads = self.gateway_threads.read().await;
            gateway_threads
                .iter()
                .find_map(|(channel_key, mapped_thread_id)| {
                    (mapped_thread_id == thread_id).then(|| channel_key.clone())
                })
        }?;

        let (platform, channel) = channel_key.split_once(':')?;
        Some(gateway::IncomingMessage {
            platform: platform.to_string(),
            sender: String::new(),
            content: String::new(),
            channel: channel.to_string(),
            message_id: None,
            thread_context: None,
        })
    }

    pub(in crate::agent) async fn maybe_auto_send_gateway_thread_response(&self, thread_id: &str) {
        let Some(msg) = self.gateway_message_target_for_thread(thread_id).await else {
            return;
        };
        let (_, reply_tool_name) = gateway_reply_tool(&msg.platform, &msg.channel);
        self.auto_send_gateway_response(thread_id, &msg, reply_tool_name)
            .await;
    }

    pub(super) async fn handle_gateway_reset_command(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
    ) -> bool {
        let trimmed = msg.content.trim().to_lowercase();
        if !is_gateway_reset_command(&trimmed) {
            return false;
        }

        self.gateway_threads.write().await.remove(channel_key);
        self.gateway_route_modes.write().await.remove(channel_key);
        if let Err(error) = self
            .history
            .delete_gateway_thread_binding(channel_key)
            .await
        {
            tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist reset binding");
        }
        if let Err(error) = self.history.delete_gateway_route_mode(channel_key).await {
            tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist reset route mode");
        }
        tracing::info!(channel_key = %channel_key, "gateway: conversation reset");

        let (_, reply_tool_name) = gateway_reply_tool(&msg.platform, &msg.channel);
        let confirmation =
            "Conversation reset. Starting fresh — send your next message whenever you're ready.";
        let result = self
            .send_gateway_platform_tool("", "gateway_reset", reply_tool_name, msg, confirmation)
            .await;
        if result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %result.content,
                "gateway: failed to send reset confirmation"
            );
        }
        true
    }

    pub(super) async fn persist_gateway_route_request(
        &self,
        channel_key: &str,
        request: GatewayRouteRequest,
    ) {
        self.gateway_route_modes
            .write()
            .await
            .insert(channel_key.to_string(), request.mode);
        if let Err(error) = self
            .history
            .upsert_gateway_route_mode(channel_key, request.mode.as_str(), now_millis())
            .await
        {
            tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist route mode");
        }
    }

    pub(super) async fn handle_gateway_route_switch_ack(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        request: GatewayRouteRequest,
        reply_tool_name: &str,
    ) -> bool {
        if !request.ack_only {
            return false;
        }

        let response_text = gateway_route_confirmation(request.mode);
        let thread_id = match self
            .persist_gateway_fast_path_exchange(channel_key, msg, &response_text)
            .await
        {
            Ok(thread_id) => thread_id,
            Err(error) => {
                tracing::error!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    %error,
                    "gateway: failed to persist route switch exchange"
                );
                return true;
            }
        };
        let tool_result = self
            .send_gateway_platform_tool(
                &thread_id,
                "gateway_route",
                reply_tool_name,
                msg,
                &response_text,
            )
            .await;
        if tool_result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send route switch confirmation"
            );
        }
        true
    }

    pub(super) async fn load_gateway_history_window(
        &self,
        channel_key: &str,
        thread_id: Option<&str>,
    ) -> Option<String> {
        let tid = thread_id?;
        match self.history.list_recent_messages(tid, 10).await {
            Ok(messages) if !messages.is_empty() => {
                let mut lines = Vec::with_capacity(messages.len() + 1);
                for m in messages {
                    let role = m.role;
                    let content = m
                        .content
                        .replace('\n', " ")
                        .chars()
                        .take(240)
                        .collect::<String>();
                    lines.push(format!("- {role}: {content}"));
                }
                Some(lines.join("\n"))
            }
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to load recent history");
                None
            }
        }
    }

    pub(super) async fn triage_gateway_message(
        &self,
        msg: &gateway::IncomingMessage,
        history_window: Option<&str>,
        existing_thread: Option<&str>,
    ) -> concierge::GatewayTriage {
        let triage = match Box::pin(tokio::time::timeout(
            std::time::Duration::from_secs(GATEWAY_TRIAGE_TIMEOUT_SECS),
            self.concierge.triage_gateway_message(
                self,
                &msg.platform,
                &msg.sender,
                &msg.content,
                history_window,
                existing_thread,
                &self.threads,
                &self.tasks,
            ),
        ))
        .await
        {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    timeout_secs = GATEWAY_TRIAGE_TIMEOUT_SECS,
                    "gateway: concierge triage timed out; falling back to full agent loop"
                );
                concierge::GatewayTriage::Complex
            }
        };
        triage
    }

    pub(super) async fn handle_concierge_simple_response(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        reply_tool_name: &str,
        response_text: &str,
    ) -> bool {
        tracing::info!(
            platform = %msg.platform,
            sender = %msg.sender,
            "gateway: concierge handled simple message"
        );
        let thread_id = match self
            .persist_gateway_fast_path_exchange(channel_key, msg, response_text)
            .await
        {
            Ok(thread_id) => thread_id,
            Err(error) => {
                tracing::error!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    %error,
                    "gateway: failed to persist concierge fast-path exchange"
                );
                return true;
            }
        };
        let tool_result = self
            .send_gateway_platform_tool(
                &thread_id,
                "concierge",
                reply_tool_name,
                msg,
                response_text,
            )
            .await;
        if tool_result.is_error {
            tracing::error!(
                platform = %msg.platform,
                channel = %msg.channel,
                error = %tool_result.content,
                "gateway: failed to send concierge simple response"
            );
        }
        true
    }

    pub(super) async fn gateway_stream_timeout(&self) -> std::time::Duration {
        let config = self.config.read().await.clone();
        self.resolve_provider_config(&config)
            .map(|provider| gateway_stream_timeout_for_reasoning(&provider.reasoning_effort))
            .unwrap_or(std::time::Duration::from_secs(GATEWAY_AGENT_TIMEOUT_SECS))
    }

    pub(super) async fn auto_send_gateway_response(
        &self,
        thread_id: &str,
        msg: &gateway::IncomingMessage,
        reply_tool_name: &str,
    ) {
        let last_response = match self
            .history
            .gateway_turn_auto_send_projection(thread_id)
            .await
        {
            Ok(Some(projection)) => {
                if projection.used_send_tool {
                    return;
                }
                projection.latest_assistant_response
            }
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "gateway: failed to query persisted auto-send state; falling back to live thread messages"
                );
                let threads = self.threads.read().await;
                let Some(thread) = threads.get(thread_id) else {
                    return;
                };
                if gateway_turn_used_send_tool(&thread.messages) {
                    return;
                }
                latest_gateway_turn_assistant_response(&thread.messages)
            }
        };

        if let Some(response_text) = last_response {
            tracing::info!(
                platform = %msg.platform,
                "gateway: agent forgot to call send tool, auto-sending response"
            );

            let _ = self
                .send_gateway_platform_tool(thread_id, "auto", reply_tool_name, msg, &response_text)
                .await;
        }
    }

    /// Resolve the configured gateway default responder into a triage decision
    /// and an optional full-agent scope override. Concierge keeps the triage
    /// front-door; the main agent and any other sub-agent run the full agent
    /// directly, scoped to that responder.
    pub(super) async fn resolve_gateway_default_responder(
        &self,
        platform: &str,
    ) -> (bool, Option<String>) {
        let default_agent = gateway_default_agent_for_platform(&self.config.read().await.gateway, platform);
        let trimmed = default_agent.trim();
        if trimmed.is_empty() {
            return (true, None);
        }
        let sub_agents = self.list_sub_agents().await;
        let resolved = crate::agent::agent_identity::resolve_agent_target(trimmed, &sub_agents);
        if resolved.scope_id == crate::agent::agent_identity::CONCIERGE_AGENT_ID {
            (true, None)
        } else if resolved.scope_id == crate::agent::agent_identity::MAIN_AGENT_ID {
            (false, None)
        } else {
            (false, Some(resolved.scope_id))
        }
    }

    pub(super) async fn handle_gateway_full_agent_response(
        &self,
        msg: &gateway::IncomingMessage,
        channel_key: &str,
        existing_thread: Option<&str>,
        history_window: Option<&str>,
        reply_tool_name: &str,
        agent_scope: Option<&str>,
    ) {
        let active_responder_name = match existing_thread {
            Some(thread_id) => self
                .active_agent_id_for_thread(thread_id)
                .await
                .map(|agent_id| canonical_agent_name(&agent_id).to_string()),
            None => None,
        };
        let (thread_id, is_new) = self
            .get_or_create_thread_with_target(existing_thread, &msg.content, agent_scope)
            .await;
        if is_new {
            self.ensure_thread_identity(&thread_id, &gateway_thread_title(msg), false)
                .await;
        }
        self.gateway_threads
            .write()
            .await
            .insert(channel_key.to_string(), thread_id.clone());
        let enriched_prompt = build_gateway_agent_prompt(
            &msg.platform,
            &msg.sender,
            &msg.content,
            history_window,
            reply_tool_name,
            active_responder_name.as_deref(),
        );
        let stream_timeout = self.gateway_stream_timeout().await;
        let send_result = Box::pin(self.send_message_with_agent_scope_override(
            Some(&thread_id),
            &msg.content,
            &enriched_prompt,
            agent_scope,
            stream_timeout,
        ))
        .await;

        match send_result {
            Err(e) => {
                tracing::error!(
                    platform = %msg.platform,
                    error = %e,
                    "gateway: failed to process incoming message"
                );
            }
            Ok(thread_id) => {
                {
                    let mut gateway_threads = self.gateway_threads.write().await;
                    if !gateway_threads.contains_key(channel_key) {
                        return;
                    }
                    gateway_threads.insert(channel_key.to_string(), thread_id.clone());
                }
                if let Err(error) = self
                    .history
                    .upsert_gateway_thread_binding(channel_key, &thread_id, now_millis())
                    .await
                {
                    tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist thread binding");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn gateway_has_pending_approval_anywhere_finds_persisted_task_owned_approval() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine.tasks.lock().await.push_back(AgentTask {
            id: "approval-task-anywhere".to_string(),
            title: "approval task".to_string(),
            description: "awaiting approval anywhere".to_string(),
            status: TaskStatus::Queued,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: now_millis(),
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some("thread-approval-anywhere".to_string()),
            source: "managed_command".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: Some("echo ok".to_string()),
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: Some("awaiting approval".to_string()),
            awaiting_approval_id: Some("approval-task-anywhere-1".to_string()),
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        assert!(engine.gateway_has_pending_approval_anywhere().await);
    }
}
