use super::*;
use zorai_protocol::AGENT_ID_SWAROG;

/// Gateway turns now run in detached per-channel tasks, so callers that assert
/// on thread state must wait for every in-flight channel to drain first.
async fn await_gateway_idle(engine: &AgentEngine) {
    for _ in 0..1000 {
        if engine.gateway_inflight_channels.lock().await.is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    panic!("gateway channels did not settle");
}

#[test]
fn hydrate_defers_gateway_start_until_after_thread_restore() {
    let persistence_source =
        fs::read_to_string(repo_root().join("crates/zorai-daemon/src/agent/persistence.rs"))
            .expect("read persistence.rs");

    let list_threads_idx = persistence_source
        .find("match self.history.list_threads().await {")
        .expect("hydrate should restore threads");
    let gateway_start_idx = persistence_source
        .find("self.schedule_gateway_startup();")
        .expect("hydrate should schedule gateway startup");

    assert!(
        list_threads_idx < gateway_start_idx,
        "hydrate should defer gateway startup until after thread restore work"
    );
}

#[test]
fn background_runtime_splits_maintenance_work_into_separate_loops() {
    let source = gateway_loop_production_source();

    for required in [
        "engine.run_task_dispatch_loop(rx).await",
        "engine.run_gateway_event_drain_loop(rx).await",
        "engine.run_heartbeat_loop(rx).await",
        "engine.run_anticipatory_loop(rx).await",
        "engine.run_watcher_refresh_loop(rx).await",
        "engine.run_gateway_supervision_loop(rx).await",
        "engine.run_stalled_turn_supervision_loop(rx).await",
        "engine.run_subagent_supervision_loop(rx).await",
    ] {
        assert!(
            source.contains(required),
            "background runtime should spawn dedicated worker loop: {required}"
        );
    }

    assert!(
        !source.contains("let mut supervisor_tick = tokio::time::interval"),
        "background runtime should not multiplex maintenance work through a shared supervisor tick"
    );
}

#[tokio::test]
async fn gateway_init_loads_replay_cursors() {
    let root = make_test_root("gateway-init-loads-replay-cursors");
    let manager = SessionManager::new_test(&root).await;

    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.gateway.telegram_token = "telegram-token".to_string();
    config.gateway.slack_token = "slack-token".to_string();
    config.gateway.slack_channel_filter = "C123".to_string();
    config.gateway.discord_token = "discord-token".to_string();
    config.gateway.discord_channel_filter = "D456".to_string();
    config.gateway.whatsapp_token = "whatsapp-token".to_string();

    let engine = AgentEngine::new_test(manager, config, &root).await;

    engine
        .history
        .save_gateway_replay_cursor("telegram", "other", "99", "update_id")
        .await
        .expect("save other telegram cursor");
    engine
        .history
        .save_gateway_replay_cursor("telegram", "global", "42", "update_id")
        .await
        .expect("save telegram cursor");
    engine
        .history
        .save_gateway_replay_cursor("slack", "C123", "1712345678.000100", "message_ts")
        .await
        .expect("save slack cursor");
    engine
        .history
        .save_gateway_replay_cursor("discord", "D456", "998877665544", "message_id")
        .await
        .expect("save discord cursor");
    engine
        .history
        .save_gateway_replay_cursor("whatsapp", "15551234567", "wamid-1", "message_id")
        .await
        .expect("save whatsapp cursor");

    engine.init_gateway().await;

    let state_guard = engine.gateway_state.lock().await;
    let state = state_guard.as_ref().expect("gateway state should exist");
    assert_eq!(state.telegram_replay_cursor, Some(42));
    assert_eq!(
        state
            .slack_replay_cursors
            .get("C123")
            .map(std::string::String::as_str),
        Some("1712345678.000100")
    );
    assert_eq!(
        state
            .discord_replay_cursors
            .get("D456")
            .map(std::string::String::as_str),
        Some("998877665544")
    );
    assert_eq!(
        state
            .whatsapp_replay_cursors
            .get("15551234567")
            .map(std::string::String::as_str),
        Some("wamid-1")
    );
    assert!(state.replay_cycle_active.is_empty());

    drop(state_guard);
    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_state_updates_survive_gateway_restart() {
    let root = make_test_root("gateway-state-updates-survive-restart");
    let manager = SessionManager::new_test(&root).await;

    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;

    engine
        .history
        .save_gateway_replay_cursor("slack", "C123", "1712345678.000100", "message_ts")
        .await
        .expect("save slack cursor");
    engine
        .history
        .upsert_gateway_thread_binding("Slack:C123", "thread-123", 1111)
        .await
        .expect("save thread binding");
    engine
        .history
        .upsert_gateway_route_mode("Slack:C123", AGENT_ID_SWAROG, 2222)
        .await
        .expect("save route mode");

    engine.init_gateway().await;
    *engine.gateway_state.lock().await = None;
    engine.init_gateway().await;

    let state_guard = engine.gateway_state.lock().await;
    let state = state_guard.as_ref().expect("gateway state should exist");
    assert_eq!(
        state
            .slack_replay_cursors
            .get("C123")
            .map(std::string::String::as_str),
        Some("1712345678.000100")
    );
    drop(state_guard);

    let bindings = engine
        .history
        .list_gateway_thread_bindings()
        .await
        .expect("list thread bindings");
    assert!(bindings
        .iter()
        .any(|(channel_key, thread_id)| channel_key == "Slack:C123" && thread_id == "thread-123"));

    let modes = engine
        .history
        .list_gateway_route_modes()
        .await
        .expect("list route modes");
    assert!(modes
        .iter()
        .any(|(channel_key, route_mode)| channel_key == "Slack:C123"
            && route_mode == AGENT_ID_SWAROG));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_health_snapshots_survive_gateway_restart() {
    let root = make_test_root("gateway-health-snapshots-survive-restart");
    let manager = SessionManager::new_test(&root).await;

    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;

    let snapshot = zorai_protocol::GatewayHealthState {
        platform: "slack".to_string(),
        status: zorai_protocol::GatewayConnectionStatus::Error,
        last_success_at_ms: Some(111),
        last_error_at_ms: Some(222),
        consecutive_failure_count: 3,
        last_error: Some("timeout".to_string()),
        current_backoff_secs: 30,
    };
    engine
        .history
        .upsert_gateway_health_snapshot(&snapshot, 333)
        .await
        .expect("save health snapshot");

    engine.init_gateway().await;
    *engine.gateway_state.lock().await = None;
    engine.init_gateway().await;

    let state_guard = engine.gateway_state.lock().await;
    let state = state_guard.as_ref().expect("gateway state should exist");
    assert_eq!(state.slack_health.status, GatewayConnectionStatus::Error);
    assert_eq!(state.slack_health.last_success_at, Some(111));
    assert_eq!(state.slack_health.last_error_at, Some(222));
    assert_eq!(state.slack_health.consecutive_failure_count, 3);
    assert_eq!(state.slack_health.last_error.as_deref(), Some("timeout"));
    assert_eq!(state.slack_health.current_backoff_secs, 30);

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_auto_send_thread_response_emits_gateway_request_for_latest_assistant_message() {
    let root = make_test_root("gateway-auto-send-thread-response");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let incoming = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Can you check the release notes?".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-incoming-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &incoming, "Initial reply")
        .await
        .expect("persist fast-path exchange");

    {
        let mut threads = engine.threads.write().await;
        let thread = threads
            .get_mut(&thread_id)
            .expect("gateway thread should exist");
        thread.messages.push(AgentMessage {
            reasoning_tokens: None,
            thinking_ms: None,
            id: "assistant-latest".to_string(),
            role: MessageRole::Assistant,
            content: "Intermittent update from the agent".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider: None,
            model: None,
            api_transport: None,
            response_id: None,
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: None,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: now_millis(),
            feedback: None,
        });
        thread.updated_at = now_millis();
    }
    engine.persist_thread_by_id(&thread_id).await;

    let helper_engine = engine.clone();
    let helper_thread_id = thread_id.clone();
    let send_task = tokio::spawn(async move {
        helper_engine
            .maybe_auto_send_gateway_thread_response(&helper_thread_id)
            .await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway send request should be emitted")
        .expect("gateway send request should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert_eq!(request.content, "Intermittent update from the agent");

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    send_task.await.expect("auto-send task should join");
    fs::remove_dir_all(&root).expect("cleanup test root");
}

async fn spawn_stub_gateway_assistant_server(response_text: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub gateway assistant server");
    let addr = listener.local_addr().expect("stub server local addr");
    let response_text = response_text.to_string();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let response_text = response_text.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let _ = socket.read(&mut buffer).await;
                let response = format!(
                    concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n",
                        "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\n",
                        "data: [DONE]\n\n"
                    ),
                    response_text
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}/v1")
}

/// Regression: a gateway turn routed to a sub-agent must own the thread under
/// that sub-agent, carry the gateway title (so it is classified as a gateway
/// thread), and auto-deliver the sub-agent's reply back to the channel.
#[tokio::test]
async fn gateway_subagent_turn_owns_thread_titles_it_and_auto_replies() {
    let root = make_test_root("gateway-subagent-turn");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_stub_gateway_assistant_server("Hey Mariusz, I am glmus.").await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.sub_agents.push(SubAgentDefinition {
        base_url: None,
        id: "subagent-glmus".to_string(),
        name: "Glmus".to_string(),
        provider: zorai_shared::providers::PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-4o-mini".to_string(),
        role: None,
        system_prompt: Some("You are glmus.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        api_transport: None,
        claude_permission_mode: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 1,
    });
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let msg = gateway::IncomingMessage {
        platform: "discord".to_string(),
        sender: "mariuszkurman".to_string(),
        content: "Bro, who are you?".to_string(),
        channel: "user:42".to_string(),
        message_id: Some("discord-incoming-glmus".to_string()),
        thread_context: None,
    };
    let channel_key = gateway::gateway_channel_key(&msg.platform, &msg.channel);

    let helper = engine.clone();
    let task_msg = msg.clone();
    let task_key = channel_key.clone();
    let task = tokio::spawn(async move {
        helper
            .handle_gateway_full_agent_response(
                &task_msg,
                &task_key,
                None,
                None,
                "send_discord_message",
                Some("subagent-glmus"),
            )
            .await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(15), rx.recv())
        .await
        .expect("gateway send request should be emitted")
        .expect("gateway send request should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert!(
        request.content.contains("glmus"),
        "auto-reply should carry the sub-agent response, got {:?}",
        request.content
    );

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: request.channel_id.clone(),
            requested_channel_id: Some(request.channel_id.clone()),
            delivery_id: Some("delivery-glmus".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    task.await.expect("gateway turn task should join");

    let thread_id = engine
        .gateway_threads
        .read()
        .await
        .get(&channel_key)
        .cloned()
        .expect("gateway thread should be bound");
    let threads = engine.threads.read().await;
    let thread = threads.get(&thread_id).expect("thread should exist");
    assert_eq!(
        thread.agent_name.as_deref(),
        Some("Glmus"),
        "gateway sub-agent thread should be owned by the sub-agent, not the main agent"
    );
    assert!(
        thread.title.to_ascii_lowercase().starts_with("discord"),
        "gateway thread should carry the gateway title so it is classified as a gateway thread, got {:?}",
        thread.title
    );
    drop(threads);
    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_resolves_pending_task_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "approval-fast-path-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-fast-path".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve-once".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-fast-path")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.role == MessageRole::User && message.content == "approve-once"));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_rejects_session_backed_reply_from_wrong_channel() {
    let root = make_test_root("gateway-approval-reply-wrong-channel");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-wrong-channel-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let request = zorai_protocol::ManagedCommandRequest {
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        allow_network: true,
        sandbox_enabled: false,
        security_level: zorai_protocol::SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: zorai_protocol::ManagedCommandSource::Agent,
    };

    let approval = match manager
        .execute_managed_command(session_id, request.clone())
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let wrong_reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve-once".to_string(),
        channel: "user:999999999".to_string(),
        message_id: Some("discord-approval-wrong-channel-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(wrong_reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway rejection should be emitted")
        .expect("gateway rejection should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:999999999");
    assert!(request.content.to_ascii_lowercase().contains("stale"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:999999999".to_string(),
            requested_channel_id: Some("user:999999999".to_string()),
            delivery_id: Some("delivery-approval-wrong-channel-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        manager
            .resolve_approval_by_id(
                &pending.approval_id,
                zorai_protocol::ApprovalDecision::ApproveOnce
            )
            .await
            .is_ok(),
        "wrong-channel reply must not resolve the approval"
    );

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_rejects_cross_platform_reply_when_other_thread_is_bound() {
    let root = make_test_root("gateway-approval-reply-cross-platform-bound-thread");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let discord_seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-cross-platform-1".to_string()),
        thread_context: None,
    };
    let discord_thread_id = engine
        .persist_gateway_fast_path_exchange(
            "Discord:user:123456789",
            &discord_seed,
            "Approval pending",
        )
        .await
        .expect("persist discord fast-path exchange");

    let slack_seed = gateway::IncomingMessage {
        platform: "Slack".to_string(),
        sender: "alice".to_string(),
        content: "Other thread".to_string(),
        channel: "C123".to_string(),
        message_id: Some("slack-seed-cross-platform-1".to_string()),
        thread_context: Some(gateway::ThreadContext {
            slack_thread_ts: Some("1712345678.000100".to_string()),
            ..Default::default()
        }),
    };
    let slack_thread_id = engine
        .persist_gateway_fast_path_exchange("Slack:C123", &slack_seed, "Other reply")
        .await
        .expect("persist slack fast-path exchange");

    let approval_id = "approval-cross-platform-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-cross-platform".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(discord_thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Slack".to_string(),
        sender: "alice".to_string(),
        content: "approve-once".to_string(),
        channel: "C123".to_string(),
        message_id: Some("slack-approval-cross-platform-1".to_string()),
        thread_context: Some(gateway::ThreadContext {
            slack_thread_ts: Some("1712345678.000100".to_string()),
            ..Default::default()
        }),
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway stale rejection should be emitted")
        .expect("gateway stale rejection should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "slack");
    assert_eq!(request.channel_id, "C123");
    assert_eq!(request.thread_id.as_deref(), Some("1712345678.000100"));
    assert!(request.content.to_ascii_lowercase().contains("stale"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "slack".to_string(),
            channel_id: "C123".to_string(),
            requested_channel_id: Some("C123".to_string()),
            delivery_id: Some("delivery-approval-cross-platform-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let tasks = engine.tasks.lock().await;
    let discord_task = tasks
        .iter()
        .find(|task| task.id == "approval-task-cross-platform")
        .cloned()
        .expect("discord approval task should exist");
    assert_eq!(discord_task.status, TaskStatus::AwaitingApproval);
    assert_eq!(
        discord_task.awaiting_approval_id.as_deref(),
        Some(approval_id)
    );
    drop(tasks);

    let slack_thread = engine
        .threads
        .read()
        .await
        .get(&slack_thread_id)
        .cloned()
        .expect("slack thread should exist");
    assert!(slack_thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("stale")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn slack_gateway_approval_reply_fast_path_resolves_pending_task_and_notifies_channel() {
    let root = make_test_root("slack-gateway-approval-reply-fast-path");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Slack".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "C123".to_string(),
        message_id: Some("slack-seed-approval-1".to_string()),
        thread_context: Some(gateway::ThreadContext {
            slack_thread_ts: Some("1712345678.000100".to_string()),
            ..Default::default()
        }),
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Slack:C123", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "slack-approval-fast-path-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "slack-approval-task-fast-path".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Slack".to_string(),
        sender: "alice".to_string(),
        content: "approve-once".to_string(),
        channel: "C123".to_string(),
        message_id: Some("slack-approval-reply-1".to_string()),
        thread_context: Some(gateway::ThreadContext {
            slack_thread_ts: Some("1712345678.000100".to_string()),
            ..Default::default()
        }),
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "slack");
    assert_eq!(request.channel_id, "C123");
    assert_eq!(request.thread_id.as_deref(), Some("1712345678.000100"));
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "slack".to_string(),
            channel_id: "C123".to_string(),
            requested_channel_id: Some("C123".to_string()),
            delivery_id: Some("slack-delivery-approval-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "slack-approval-task-fast-path")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.role == MessageRole::User && message.content == "approve-once"));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn telegram_gateway_approval_reply_fast_path_resolves_pending_task_and_notifies_channel() {
    let root = make_test_root("telegram-gateway-approval-reply-fast-path");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Telegram".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "123456".to_string(),
        message_id: Some("telegram-seed-approval-1".to_string()),
        thread_context: Some(gateway::ThreadContext {
            telegram_message_id: Some(777),
            ..Default::default()
        }),
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Telegram:123456", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "telegram-approval-fast-path-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "telegram-approval-task-fast-path".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Telegram".to_string(),
        sender: "alice".to_string(),
        content: "approve-once".to_string(),
        channel: "123456".to_string(),
        message_id: Some("telegram-approval-reply-1".to_string()),
        thread_context: Some(gateway::ThreadContext {
            telegram_message_id: Some(777),
            ..Default::default()
        }),
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "telegram");
    assert_eq!(request.channel_id, "123456");
    assert_eq!(request.thread_id.as_deref(), Some("777"));
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "telegram".to_string(),
            channel_id: "123456".to_string(),
            requested_channel_id: Some("123456".to_string()),
            delivery_id: Some("telegram-delivery-approval-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "telegram-approval-task-fast-path")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.role == MessageRole::User && message.content == "approve-once"));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_approve_once_phrase_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-approve-once-phrase");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-approve-once-phrase-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "approval-fast-path-approve-once-phrase-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-fast-path-approve-once-phrase".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve once".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-approve-once-phrase-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-approve-once-phrase-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-fast-path-approve-once-phrase")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "approve once" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_approve_session_phrase_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-approve-session-phrase");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-approve-session-phrase-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "approval-fast-path-approve-session-phrase-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-fast-path-approve-session-phrase".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve session".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-approve-session-phrase-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved for this session"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-approve-session-phrase-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-fast-path-approve-session-phrase")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content == "approve session"
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved for this session")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_resolves_pending_task_with_session_grant_and_notifies_channel(
) {
    let root = make_test_root("gateway-approval-reply-fast-path-approve-session");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-approve-session-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "approval-fast-path-approve-session-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-fast-path-approve-session".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve-session".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-approve-session-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved for this session"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-approve-session-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-fast-path-approve-session")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());
    assert!(task.blocked_reason.is_none());

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content == "approve-session"
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved for this session")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_allow_session_alias_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-allow-session-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-allow-session-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "approval-fast-path-allow-session-alias-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-fast-path-allow-session-alias".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "allow session".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-allow-session-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved for this session"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-allow-session-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-fast-path-allow-session-alias")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content == "allow session"
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved for this session")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_reject_alias_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-reject-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-reject-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "approval-fast-path-reject-alias-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-fast-path-reject-alias".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "reject".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-reject-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway denial confirmation should be emitted")
        .expect("gateway denial confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.to_ascii_lowercase().contains("denied"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-reject-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-fast-path-reject-alias")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Failed);
    assert!(task.awaiting_approval_id.is_none());
    assert_eq!(
        task.blocked_reason.as_deref(),
        Some("operator denied managed command approval")
    );

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "reject" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("denied")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_allow_once_alias_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-allow-once-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-allow-once-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "approval-fast-path-allow-once-alias-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-fast-path-allow-once-alias".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "allow once".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-allow-once-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-allow-once-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-fast-path-allow-once-alias")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "allow once" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_denied_alias_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-denied-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-denied-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "approval-fast-path-denied-alias-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-fast-path-denied-alias".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "denied".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-denied-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway denial confirmation should be emitted")
        .expect("gateway denial confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.to_ascii_lowercase().contains("denied"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-denied-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-fast-path-denied-alias")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Failed);
    assert!(task.awaiting_approval_id.is_none());
    assert_eq!(
        task.blocked_reason.as_deref(),
        Some("operator denied managed command approval")
    );
    assert_eq!(
        task.error.as_deref(),
        Some("operator denied managed command approval")
    );

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "denied" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("denied")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_denies_pending_task_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-deny");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-deny-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "approval-fast-path-deny-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-fast-path-deny".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(approval_id.to_string()),
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

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "deny".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-deny-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway denial confirmation should be emitted")
        .expect("gateway denial confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.to_ascii_lowercase().contains("denied"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-deny-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-fast-path-deny")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Failed);
    assert!(task.awaiting_approval_id.is_none());
    assert_eq!(
        task.blocked_reason.as_deref(),
        Some("operator denied managed command approval")
    );
    assert_eq!(
        task.error.as_deref(),
        Some("operator denied managed command approval")
    );

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.role == MessageRole::User && message.content == "deny"));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("denied")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_resolves_real_session_backed_approval_and_notifies_channel(
) {
    let root = make_test_root("gateway-approval-reply-fast-path-session-backed");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-session-backed-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let approval = match manager
        .execute_managed_command(
            session_id,
            zorai_protocol::ManagedCommandRequest {
                command: "sudo terraform destroy".to_string(),
                rationale: "apply risky infra change".to_string(),
                allow_network: true,
                sandbox_enabled: false,
                security_level: zorai_protocol::SecurityLevel::Moderate,
                cwd: Some("/tmp".to_string()),
                language_hint: Some("bash".to_string()),
                source: zorai_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve-once".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-session-backed-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-session-backed-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );
    assert!(
        manager
            .resolve_approval_by_id(
                &pending.approval_id,
                zorai_protocol::ApprovalDecision::ApproveOnce
            )
            .await
            .is_err(),
        "session-backed approval should already be resolved"
    );

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.role == MessageRole::User && message.content == "approve-once"));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_real_session_backed_approve_once_phrase_and_notifies_channel(
) {
    let root =
        make_test_root("gateway-approval-reply-fast-path-session-backed-approve-once-phrase");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-session-backed-approve-once-phrase-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let request = zorai_protocol::ManagedCommandRequest {
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        allow_network: true,
        sandbox_enabled: false,
        security_level: zorai_protocol::SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: zorai_protocol::ManagedCommandSource::Agent,
    };

    let approval = match manager
        .execute_managed_command(session_id, request.clone())
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve once".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-session-backed-approve-once-phrase-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-session-backed-approve-once-phrase-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );
    assert!(
        manager
            .resolve_approval_by_id(
                &pending.approval_id,
                zorai_protocol::ApprovalDecision::ApproveOnce
            )
            .await
            .is_err(),
        "session-backed approval should already be resolved"
    );

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "approve once" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_real_session_backed_approve_session_phrase_and_notifies_channel(
) {
    let root =
        make_test_root("gateway-approval-reply-fast-path-session-backed-approve-session-phrase");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-session-backed-approve-session-phrase-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let request = zorai_protocol::ManagedCommandRequest {
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        allow_network: true,
        sandbox_enabled: false,
        security_level: zorai_protocol::SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: zorai_protocol::ManagedCommandSource::Agent,
    };

    let approval = match manager
        .execute_managed_command(session_id, request.clone())
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve session".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-session-backed-approve-session-phrase-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let gateway_request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(gateway_request.platform, "discord");
    assert_eq!(gateway_request.channel_id, "user:123456789");
    assert!(gateway_request
        .content
        .to_ascii_lowercase()
        .contains("approved for this session"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: gateway_request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some(
                "delivery-approval-session-backed-approve-session-phrase-1".to_string(),
            ),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );
    assert!(
        manager
            .resolve_approval_by_id(
                &pending.approval_id,
                zorai_protocol::ApprovalDecision::ApproveSession,
            )
            .await
            .is_err(),
        "session-backed approval should already be resolved"
    );

    let reused = manager
        .execute_managed_command(session_id, request)
        .await
        .expect("matching session grant should allow queueing");
    assert!(matches!(
        reused,
        zorai_protocol::DaemonMessage::ManagedCommandQueued { .. }
    ));

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content == "approve session"
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved for this session")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_resolves_real_session_backed_session_approval_and_notifies_channel(
) {
    let root = make_test_root("gateway-approval-reply-fast-path-session-backed-approve-session");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-session-backed-approve-session-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let request = zorai_protocol::ManagedCommandRequest {
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        allow_network: true,
        sandbox_enabled: false,
        security_level: zorai_protocol::SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: zorai_protocol::ManagedCommandSource::Agent,
    };

    let approval = match manager
        .execute_managed_command(session_id, request.clone())
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve-session".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-session-backed-approve-session-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let gateway_request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(gateway_request.platform, "discord");
    assert_eq!(gateway_request.channel_id, "user:123456789");
    assert!(gateway_request
        .content
        .to_ascii_lowercase()
        .contains("approved for this session"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: gateway_request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-session-backed-approve-session-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );
    assert!(
        manager
            .resolve_approval_by_id(
                &pending.approval_id,
                zorai_protocol::ApprovalDecision::ApproveSession,
            )
            .await
            .is_err(),
        "session-backed approval should already be resolved"
    );

    let reused = manager
        .execute_managed_command(session_id, request)
        .await
        .expect("matching session grant should allow queueing");
    assert!(matches!(
        reused,
        zorai_protocol::DaemonMessage::ManagedCommandQueued { .. }
    ));

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content == "approve-session"
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved for this session")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_real_session_backed_allow_session_alias_and_notifies_channel(
) {
    let root =
        make_test_root("gateway-approval-reply-fast-path-session-backed-allow-session-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-session-backed-allow-session-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let request = zorai_protocol::ManagedCommandRequest {
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        allow_network: true,
        sandbox_enabled: false,
        security_level: zorai_protocol::SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: zorai_protocol::ManagedCommandSource::Agent,
    };

    let approval = match manager
        .execute_managed_command(session_id, request.clone())
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "allow session".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-session-backed-allow-session-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let gateway_request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(gateway_request.platform, "discord");
    assert_eq!(gateway_request.channel_id, "user:123456789");
    assert!(gateway_request
        .content
        .to_ascii_lowercase()
        .contains("approved for this session"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: gateway_request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-session-backed-allow-session-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );
    assert!(
        manager
            .resolve_approval_by_id(
                &pending.approval_id,
                zorai_protocol::ApprovalDecision::ApproveSession,
            )
            .await
            .is_err(),
        "session-backed approval should already be resolved"
    );

    let reused = manager
        .execute_managed_command(session_id, request)
        .await
        .expect("matching session grant should allow queueing");
    assert!(matches!(
        reused,
        zorai_protocol::DaemonMessage::ManagedCommandQueued { .. }
    ));

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content == "allow session"
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved for this session")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_real_session_backed_allow_once_alias_and_notifies_channel(
) {
    let root = make_test_root("gateway-approval-reply-fast-path-session-backed-allow-once-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-session-backed-allow-once-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let request = zorai_protocol::ManagedCommandRequest {
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        allow_network: true,
        sandbox_enabled: false,
        security_level: zorai_protocol::SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: zorai_protocol::ManagedCommandSource::Agent,
    };

    let approval = match manager
        .execute_managed_command(session_id, request.clone())
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "allow once".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-session-backed-allow-once-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let gateway_request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval confirmation should be emitted")
        .expect("gateway approval confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(gateway_request.platform, "discord");
    assert_eq!(gateway_request.channel_id, "user:123456789");
    assert!(gateway_request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: gateway_request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-session-backed-allow-once-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );
    assert!(
        manager
            .resolve_approval_by_id(
                &pending.approval_id,
                zorai_protocol::ApprovalDecision::ApproveOnce,
            )
            .await
            .is_err(),
        "session-backed approval should already be resolved"
    );

    match manager
        .execute_managed_command(session_id, request)
        .await
        .expect("subsequent matching command should require fresh approval after allow-once")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { .. } => {}
        other => panic!("expected fresh ApprovalRequired after allow-once, got {other:?}"),
    }

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "allow once" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_real_session_backed_reject_alias_and_notifies_channel(
) {
    let root = make_test_root("gateway-approval-reply-fast-path-session-backed-reject-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-session-backed-reject-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let approval = match manager
        .execute_managed_command(
            session_id,
            zorai_protocol::ManagedCommandRequest {
                command: "sudo terraform destroy".to_string(),
                rationale: "apply risky infra change".to_string(),
                allow_network: true,
                sandbox_enabled: false,
                security_level: zorai_protocol::SecurityLevel::Moderate,
                cwd: Some("/tmp".to_string()),
                language_hint: Some("bash".to_string()),
                source: zorai_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "reject".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-session-backed-reject-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway denial confirmation should be emitted")
        .expect("gateway denial confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.to_ascii_lowercase().contains("denied"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-session-backed-reject-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after denial"
    );
    assert!(
        manager
            .resolve_approval_by_id(&pending.approval_id, zorai_protocol::ApprovalDecision::Deny)
            .await
            .is_err(),
        "session-backed approval should already be resolved"
    );

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "reject" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("denied")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_real_session_backed_denied_alias_and_notifies_channel(
) {
    let root = make_test_root("gateway-approval-reply-fast-path-session-backed-denied-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-session-backed-denied-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let approval = match manager
        .execute_managed_command(
            session_id,
            zorai_protocol::ManagedCommandRequest {
                command: "sudo terraform destroy".to_string(),
                rationale: "apply risky infra change".to_string(),
                allow_network: true,
                sandbox_enabled: false,
                security_level: zorai_protocol::SecurityLevel::Moderate,
                cwd: Some("/tmp".to_string()),
                language_hint: Some("bash".to_string()),
                source: zorai_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "denied".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-session-backed-denied-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway denial confirmation should be emitted")
        .expect("gateway denial confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.to_ascii_lowercase().contains("denied"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-session-backed-denied-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after denial"
    );
    assert!(
        manager
            .resolve_approval_by_id(&pending.approval_id, zorai_protocol::ApprovalDecision::Deny)
            .await
            .is_err(),
        "session-backed approval should already be resolved"
    );

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "denied" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("denied")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_denies_real_session_backed_approval_and_notifies_channel()
{
    let root = make_test_root("gateway-approval-reply-fast-path-session-backed-deny");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-session-backed-deny-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let (session_id, _rx) = manager
        .spawn(Some("/bin/sh".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn test session");

    let approval = match manager
        .execute_managed_command(
            session_id,
            zorai_protocol::ManagedCommandRequest {
                command: "sudo terraform destroy".to_string(),
                rationale: "apply risky infra change".to_string(),
                allow_network: true,
                sandbox_enabled: false,
                security_level: zorai_protocol::SecurityLevel::Moderate,
                cwd: Some("/tmp".to_string()),
                language_hint: Some("bash".to_string()),
                source: zorai_protocol::ManagedCommandSource::Agent,
            },
        )
        .await
        .expect("managed command should return approval")
    {
        zorai_protocol::DaemonMessage::ApprovalRequired { approval, .. } => approval,
        other => panic!("expected approval required, got {other:?}"),
    };

    let pending = ToolPendingApproval {
        approval_id: approval.approval_id.clone(),
        execution_id: approval.execution_id.clone(),
        command: approval.command.clone(),
        rationale: approval.rationale.clone(),
        risk_level: approval.risk_level.clone(),
        blast_radius: approval.blast_radius.clone(),
        reasons: approval.reasons.clone(),
        session_id: Some(session_id.to_string()),
    };
    engine.remember_pending_approval_command(&pending).await;
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "deny".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-session-backed-deny-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway denial confirmation should be emitted")
        .expect("gateway denial confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.to_ascii_lowercase().contains("denied"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-session-backed-deny-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after denial"
    );
    assert!(
        manager
            .resolve_approval_by_id(&pending.approval_id, zorai_protocol::ApprovalDecision::Deny)
            .await
            .is_err(),
        "session-backed approval should already be resolved"
    );

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.role == MessageRole::User && message.content == "deny"));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("denied")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_resolves_pending_no_task_approval_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-no-task");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.gateway.enabled = true;
    config.critique.enabled = true;
    config.critique.mode = crate::agent::types::CritiqueMode::Deterministic;
    config.extra.insert(
        "test_force_critique_decision".to_string(),
        serde_json::Value::String("proceed_with_modifications".to_string()),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-no-task-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-gateway-critique".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let initial = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async {
            {
                let mut model = engine.operator_model.write().await;
                model.risk_fingerprint.risk_tolerance =
                    crate::agent::operator_model::RiskTolerance::Moderate;
                model.operator_satisfaction.label = "strained".to_string();
                model.operator_satisfaction.score = 0.21;
            }
            execute_tool(
                &tool_call,
                &engine,
                &thread_id,
                None,
                &manager,
                None,
                &engine.event_tx,
                root.as_path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(
        !initial.is_error,
        "first pass should pause for approval, not hard fail"
    );
    let pending = initial
        .pending_approval
        .clone()
        .expect("critique confirmation should produce pending approval");
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");

    assert!(
        engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should exist before gateway approval reply"
    );
    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve-once".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-no-task-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("gateway approval confirmation should be emitted")
            .expect("gateway approval confirmation should exist");
        match message {
            zorai_protocol::DaemonMessage::GatewaySendRequest { request } => break request,
            zorai_protocol::DaemonMessage::GatewayReloadCommand { .. } => continue,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        }
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-no-task-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should be resumed and removed"
    );
    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4");

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.role == MessageRole::User && message.content == "approve-once"));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_no_task_approve_once_phrase_and_notifies_channel()
{
    let root = make_test_root("gateway-approval-reply-fast-path-no-task-approve-once-phrase");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.gateway.enabled = true;
    config.critique.enabled = true;
    config.critique.mode = crate::agent::types::CritiqueMode::Deterministic;
    config.extra.insert(
        "test_force_critique_decision".to_string(),
        serde_json::Value::String("proceed_with_modifications".to_string()),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-no-task-approve-once-phrase-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-gateway-critique-approve-once-phrase".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let initial = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async {
            {
                let mut model = engine.operator_model.write().await;
                model.risk_fingerprint.risk_tolerance =
                    crate::agent::operator_model::RiskTolerance::Moderate;
                model.operator_satisfaction.label = "strained".to_string();
                model.operator_satisfaction.score = 0.21;
            }
            execute_tool(
                &tool_call,
                &engine,
                &thread_id,
                None,
                &manager,
                None,
                &engine.event_tx,
                root.as_path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(
        !initial.is_error,
        "first pass should pause for approval, not hard fail"
    );
    let pending = initial
        .pending_approval
        .clone()
        .expect("critique confirmation should produce pending approval");
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");

    assert!(
        engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should exist before gateway approval reply"
    );
    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve once".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-no-task-approve-once-phrase-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("gateway approval confirmation should be emitted")
            .expect("gateway approval confirmation should exist");
        match message {
            zorai_protocol::DaemonMessage::GatewaySendRequest { request } => break request,
            zorai_protocol::DaemonMessage::GatewayReloadCommand { .. } => continue,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        }
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-no-task-approve-once-phrase-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should be resumed and removed"
    );
    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4");

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "approve once" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_resolves_pending_no_task_session_approval_and_notifies_channel(
) {
    let root = make_test_root("gateway-approval-reply-fast-path-no-task-approve-session");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.gateway.enabled = true;
    config.critique.enabled = true;
    config.critique.mode = crate::agent::types::CritiqueMode::Deterministic;
    config.extra.insert(
        "test_force_critique_decision".to_string(),
        serde_json::Value::String("proceed_with_modifications".to_string()),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-no-task-approve-session-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-gateway-critique-approve-session".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let initial = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async {
            {
                let mut model = engine.operator_model.write().await;
                model.risk_fingerprint.risk_tolerance =
                    crate::agent::operator_model::RiskTolerance::Moderate;
                model.operator_satisfaction.label = "strained".to_string();
                model.operator_satisfaction.score = 0.21;
            }
            execute_tool(
                &tool_call,
                &engine,
                &thread_id,
                None,
                &manager,
                None,
                &engine.event_tx,
                root.as_path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(
        !initial.is_error,
        "first pass should pause for approval, not hard fail"
    );
    let pending = initial
        .pending_approval
        .clone()
        .expect("critique confirmation should produce pending approval");
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");

    assert!(
        engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should exist before gateway approval reply"
    );
    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve-session".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-no-task-approve-session-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("gateway approval confirmation should be emitted")
            .expect("gateway approval confirmation should exist");
        match message {
            zorai_protocol::DaemonMessage::GatewaySendRequest { request } => break request,
            zorai_protocol::DaemonMessage::GatewayReloadCommand { .. } => continue,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        }
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved for this session"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-no-task-approve-session-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should be resumed and removed"
    );
    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4");

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content == "approve-session"
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved for this session")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_no_task_approve_session_phrase_and_notifies_channel(
) {
    let root = make_test_root("gateway-approval-reply-fast-path-no-task-approve-session-phrase");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.gateway.enabled = true;
    config.critique.enabled = true;
    config.critique.mode = crate::agent::types::CritiqueMode::Deterministic;
    config.extra.insert(
        "test_force_critique_decision".to_string(),
        serde_json::Value::String("proceed_with_modifications".to_string()),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-no-task-approve-session-phrase-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-gateway-critique-approve-session-phrase".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let initial = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async {
            {
                let mut model = engine.operator_model.write().await;
                model.risk_fingerprint.risk_tolerance =
                    crate::agent::operator_model::RiskTolerance::Moderate;
                model.operator_satisfaction.label = "strained".to_string();
                model.operator_satisfaction.score = 0.21;
            }
            execute_tool(
                &tool_call,
                &engine,
                &thread_id,
                None,
                &manager,
                None,
                &engine.event_tx,
                root.as_path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(
        !initial.is_error,
        "first pass should pause for approval, not hard fail"
    );
    let pending = initial
        .pending_approval
        .clone()
        .expect("critique confirmation should produce pending approval");
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");

    assert!(
        engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should exist before gateway approval reply"
    );
    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "approve session".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-no-task-approve-session-phrase-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("gateway approval confirmation should be emitted")
            .expect("gateway approval confirmation should exist");
        match message {
            zorai_protocol::DaemonMessage::GatewaySendRequest { request } => break request,
            zorai_protocol::DaemonMessage::GatewayReloadCommand { .. } => continue,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        }
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved for this session"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-no-task-approve-session-phrase-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should be resumed and removed"
    );
    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4");

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content == "approve session"
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved for this session")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_no_task_allow_session_alias_and_notifies_channel()
{
    let root = make_test_root("gateway-approval-reply-fast-path-no-task-allow-session-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.gateway.enabled = true;
    config.critique.enabled = true;
    config.critique.mode = crate::agent::types::CritiqueMode::Deterministic;
    config.extra.insert(
        "test_force_critique_decision".to_string(),
        serde_json::Value::String("proceed_with_modifications".to_string()),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-no-task-allow-session-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-gateway-critique-allow-session-alias".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let initial = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async {
            {
                let mut model = engine.operator_model.write().await;
                model.risk_fingerprint.risk_tolerance =
                    crate::agent::operator_model::RiskTolerance::Moderate;
                model.operator_satisfaction.label = "strained".to_string();
                model.operator_satisfaction.score = 0.21;
            }
            execute_tool(
                &tool_call,
                &engine,
                &thread_id,
                None,
                &manager,
                None,
                &engine.event_tx,
                root.as_path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(
        !initial.is_error,
        "first pass should pause for approval, not hard fail"
    );
    let pending = initial
        .pending_approval
        .clone()
        .expect("critique confirmation should produce pending approval");
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");

    assert!(
        engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should exist before gateway approval reply"
    );
    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "allow session".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-no-task-allow-session-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("gateway approval confirmation should be emitted")
            .expect("gateway approval confirmation should exist");
        match message {
            zorai_protocol::DaemonMessage::GatewaySendRequest { request } => break request,
            zorai_protocol::DaemonMessage::GatewayReloadCommand { .. } => continue,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        }
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved for this session"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-no-task-allow-session-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should be resumed and removed"
    );
    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4");

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::User && message.content == "allow session"
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved for this session")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_no_task_allow_once_alias_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-no-task-allow-once-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.gateway.enabled = true;
    config.critique.enabled = true;
    config.critique.mode = crate::agent::types::CritiqueMode::Deterministic;
    config.extra.insert(
        "test_force_critique_decision".to_string(),
        serde_json::Value::String("proceed_with_modifications".to_string()),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-no-task-allow-once-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-gateway-critique-allow-once-alias".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let initial = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async {
            {
                let mut model = engine.operator_model.write().await;
                model.risk_fingerprint.risk_tolerance =
                    crate::agent::operator_model::RiskTolerance::Moderate;
                model.operator_satisfaction.label = "strained".to_string();
                model.operator_satisfaction.score = 0.21;
            }
            execute_tool(
                &tool_call,
                &engine,
                &thread_id,
                None,
                &manager,
                None,
                &engine.event_tx,
                root.as_path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(
        !initial.is_error,
        "first pass should pause for approval, not hard fail"
    );
    let pending = initial
        .pending_approval
        .clone()
        .expect("critique confirmation should produce pending approval");
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");

    assert!(
        engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should exist before gateway approval reply"
    );
    assert!(
        engine
            .tasks
            .lock()
            .await
            .iter()
            .all(|task| task.awaiting_approval_id.as_deref() != Some(pending.approval_id.as_str())),
        "no task should own this approval id"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "allow once".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-no-task-allow-once-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("gateway approval confirmation should be emitted")
            .expect("gateway approval confirmation should exist");
        match message {
            zorai_protocol::DaemonMessage::GatewaySendRequest { request } => break request,
            zorai_protocol::DaemonMessage::GatewayReloadCommand { .. } => continue,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        }
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request
        .content
        .to_ascii_lowercase()
        .contains("approved once"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-no-task-allow-once-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should be resumed and removed"
    );
    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after resolution"
    );

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4");

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "allow once" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .to_ascii_lowercase()
                .contains("approved once")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_no_task_reject_alias_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-no-task-reject-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.gateway.enabled = true;
    config.critique.enabled = true;
    config.critique.mode = crate::agent::types::CritiqueMode::Deterministic;
    config.extra.insert(
        "test_force_critique_decision".to_string(),
        serde_json::Value::String("proceed_with_modifications".to_string()),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-no-task-reject-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-gateway-critique-reject-alias".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let initial = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async {
            {
                let mut model = engine.operator_model.write().await;
                model.risk_fingerprint.risk_tolerance =
                    crate::agent::operator_model::RiskTolerance::Moderate;
                model.operator_satisfaction.label = "strained".to_string();
                model.operator_satisfaction.score = 0.21;
            }
            execute_tool(
                &tool_call,
                &engine,
                &thread_id,
                None,
                &manager,
                None,
                &engine.event_tx,
                root.as_path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(
        !initial.is_error,
        "first pass should pause for approval, not hard fail"
    );
    let pending = initial
        .pending_approval
        .clone()
        .expect("critique confirmation should produce pending approval");
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");

    assert!(
        engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should exist before gateway denial reply"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "reject".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-no-task-reject-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway denial confirmation should be emitted")
        .expect("gateway denial confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.to_ascii_lowercase().contains("denied"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-no-task-reject-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should be removed after denial"
    );
    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after denial"
    );

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4-mini");

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.role == MessageRole::User && message.content == "reject"));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("denied")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_accepts_no_task_denied_alias_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-no-task-denied-alias");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.gateway.enabled = true;
    config.critique.enabled = true;
    config.critique.mode = crate::agent::types::CritiqueMode::Deterministic;
    config.extra.insert(
        "test_force_critique_decision".to_string(),
        serde_json::Value::String("proceed_with_modifications".to_string()),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-no-task-denied-alias-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-gateway-critique-denied-alias".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let initial = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async {
            {
                let mut model = engine.operator_model.write().await;
                model.risk_fingerprint.risk_tolerance =
                    crate::agent::operator_model::RiskTolerance::Moderate;
                model.operator_satisfaction.label = "strained".to_string();
                model.operator_satisfaction.score = 0.21;
            }
            execute_tool(
                &tool_call,
                &engine,
                &thread_id,
                None,
                &manager,
                None,
                &engine.event_tx,
                root.as_path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(
        !initial.is_error,
        "first pass should pause for approval, not hard fail"
    );
    let pending = initial
        .pending_approval
        .clone()
        .expect("critique confirmation should produce pending approval");
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");

    assert!(
        engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should exist before gateway denial reply"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "denied".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-no-task-denied-alias-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway denial confirmation should be emitted")
        .expect("gateway denial confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.to_ascii_lowercase().contains("denied"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-no-task-denied-alias-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should be removed after denial"
    );
    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after denial"
    );

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4-mini");

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| { message.role == MessageRole::User && message.content == "denied" }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("denied")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_approval_reply_fast_path_denies_pending_no_task_approval_and_notifies_channel() {
    let root = make_test_root("gateway-approval-reply-fast-path-no-task-deny");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.gateway.enabled = true;
    config.critique.enabled = true;
    config.critique.mode = crate::agent::types::CritiqueMode::Deterministic;
    config.extra.insert(
        "test_force_critique_decision".to_string(),
        serde_json::Value::String("proceed_with_modifications".to_string()),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-no-task-deny-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-gateway-critique-deny".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let initial = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async {
            {
                let mut model = engine.operator_model.write().await;
                model.risk_fingerprint.risk_tolerance =
                    crate::agent::operator_model::RiskTolerance::Moderate;
                model.operator_satisfaction.label = "strained".to_string();
                model.operator_satisfaction.score = 0.21;
            }
            execute_tool(
                &tool_call,
                &engine,
                &thread_id,
                None,
                &manager,
                None,
                &engine.event_tx,
                root.as_path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(
        !initial.is_error,
        "first pass should pause for approval, not hard fail"
    );
    let pending = initial
        .pending_approval
        .clone()
        .expect("critique confirmation should produce pending approval");
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");

    assert!(
        engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should exist before gateway denial reply"
    );

    let reply = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "deny".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-approval-no-task-deny-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(reply)
            .await
            .expect("enqueue reply should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway denial confirmation should be emitted")
        .expect("gateway denial confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.to_ascii_lowercase().contains("denied"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-approval-no-task-deny-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    assert!(
        !engine
            .critique_approval_continuations
            .lock()
            .await
            .contains_key(&pending.approval_id),
        "continuation should be removed after denial"
    );
    assert!(
        !engine
            .pending_operator_approvals
            .read()
            .await
            .contains_key(&pending.approval_id),
        "pending operator approval should be cleared after denial"
    );

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4-mini");

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.role == MessageRole::User && message.content == "deny"));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.to_ascii_lowercase().contains("denied")
    }));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_pending_approval_lookup_finds_critique_continuation_for_thread_without_task_binding(
) {
    let root = make_test_root("gateway-pending-approval-lookup-critique-continuation");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-lookup-critique-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let approval_id = "critique-continuation-approval-1".to_string();
    engine.critique_approval_continuations.lock().await.insert(
        approval_id.clone(),
        CritiqueApprovalContinuation {
            tool_call: ToolCall::with_default_weles_review(
                "tool-switch-model-lookup-critique".to_string(),
                ToolFunction {
                    name: "switch_model".to_string(),
                    arguments: serde_json::json!({
                        "agent": "svarog",
                        "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                        "model": "gpt-5.4"
                    })
                    .to_string(),
                },
            ),
            thread_id: thread_id.clone(),
            agent_data_dir: root.clone(),
        },
    );

    let resolved = engine.gateway_pending_approval_for_thread(&thread_id).await;
    assert_eq!(resolved.as_deref(), Some(approval_id.as_str()));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_pending_approval_lookup_finds_message_history_approval_for_thread_without_task_or_continuation(
) {
    let root = make_test_root("gateway-pending-approval-lookup-message-history");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-lookup-history-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let pending = ToolPendingApproval {
        approval_id: "approval-history-lookup-1".to_string(),
        execution_id: "exec-history-lookup-1".to_string(),
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        risk_level: "high".to_string(),
        blast_radius: "workspace".to_string(),
        reasons: vec!["destructive command".to_string()],
        session_id: None,
    };
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let resolved = engine.gateway_pending_approval_for_thread(&thread_id).await;
    assert_eq!(resolved.as_deref(), Some(pending.approval_id.as_str()));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_pending_approval_lookup_skips_stale_message_history_ids_and_returns_older_pending_one(
) {
    let root = make_test_root("gateway-pending-approval-lookup-stale-history-fallback");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-lookup-stale-history-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let valid_pending = ToolPendingApproval {
        approval_id: "approval-stale-history-valid-1".to_string(),
        execution_id: "exec-stale-history-valid-1".to_string(),
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        risk_level: "high".to_string(),
        blast_radius: "workspace".to_string(),
        reasons: vec!["destructive command".to_string()],
        session_id: None,
    };
    engine
        .record_operator_approval_requested(&valid_pending)
        .await
        .expect("record valid pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                valid_pending.approval_id,
                valid_pending.risk_level,
                valid_pending.blast_radius,
                valid_pending.command,
                valid_pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    engine
        .add_assistant_message(
            &thread_id,
            "Managed command requires approval before execution. Approval ID: approval-stale-history-stale-1\nRisk: high\nBlast radius: workspace\nCommand: sudo rm -rf /tmp/demo\nReasons:\n- destructive command",
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let resolved = engine.gateway_pending_approval_for_thread(&thread_id).await;
    assert_eq!(
        resolved.as_deref(),
        Some(valid_pending.approval_id.as_str())
    );

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_pending_approval_lookup_prefers_critique_continuation_over_message_history_for_thread(
) {
    let root = make_test_root("gateway-pending-approval-lookup-critique-vs-history");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-lookup-critique-vs-history-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let continuation_approval_id = "approval-critique-vs-history-continuation-1".to_string();
    engine.critique_approval_continuations.lock().await.insert(
        continuation_approval_id.clone(),
        CritiqueApprovalContinuation {
            tool_call: ToolCall::with_default_weles_review(
                "tool-switch-model-critique-vs-history".to_string(),
                ToolFunction {
                    name: "switch_model".to_string(),
                    arguments: serde_json::json!({
                        "agent": "svarog",
                        "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                        "model": "gpt-5.4"
                    })
                    .to_string(),
                },
            ),
            thread_id: thread_id.clone(),
            agent_data_dir: root.clone(),
        },
    );

    let pending = ToolPendingApproval {
        approval_id: "approval-critique-vs-history-message-1".to_string(),
        execution_id: "exec-critique-vs-history-message-1".to_string(),
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        risk_level: "high".to_string(),
        blast_radius: "workspace".to_string(),
        reasons: vec!["destructive command".to_string()],
        session_id: None,
    };
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let resolved = engine.gateway_pending_approval_for_thread(&thread_id).await;
    assert_eq!(resolved.as_deref(), Some(continuation_approval_id.as_str()));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_pending_approval_lookup_prefers_task_owned_approval_for_thread() {
    let root = make_test_root("gateway-pending-approval-lookup-task-owned");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-lookup-task-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    let task_approval_id = "approval-task-owned-lookup-1";
    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-owned-lookup".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
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
        awaiting_approval_id: Some(task_approval_id.to_string()),
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

    let continuation_approval_id = "approval-task-owned-lookup-continuation".to_string();
    engine.critique_approval_continuations.lock().await.insert(
        continuation_approval_id,
        CritiqueApprovalContinuation {
            tool_call: ToolCall::with_default_weles_review(
                "tool-switch-model-task-owned-lookup".to_string(),
                ToolFunction {
                    name: "switch_model".to_string(),
                    arguments: serde_json::json!({
                        "agent": "svarog",
                        "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                        "model": "gpt-5.4"
                    })
                    .to_string(),
                },
            ),
            thread_id: thread_id.clone(),
            agent_data_dir: root.clone(),
        },
    );

    let pending = ToolPendingApproval {
        approval_id: "approval-task-owned-lookup-history".to_string(),
        execution_id: "exec-task-owned-lookup-history".to_string(),
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        risk_level: "high".to_string(),
        blast_radius: "workspace".to_string(),
        reasons: vec!["destructive command".to_string()],
        session_id: None,
    };
    engine
        .record_operator_approval_requested(&pending)
        .await
        .expect("record pending operator approval");
    engine
        .add_assistant_message(
            &thread_id,
            &format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}\nReasons:\n- {}",
                pending.approval_id,
                pending.risk_level,
                pending.blast_radius,
                pending.command,
                pending.reasons.join("\n- ")
            ),
            0,
            0,
            None,
            Some("gateway".to_string()),
            None,
            None,
            None,
        )
        .await;

    let resolved = engine.gateway_pending_approval_for_thread(&thread_id).await;
    assert_eq!(resolved.as_deref(), Some(task_approval_id));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn mark_task_awaiting_approval_emits_gateway_prompt_for_same_channel() {
    let root = make_test_root("gateway-pending-approval-prompt");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need approval".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-prompt-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Approval pending")
        .await
        .expect("persist fast-path exchange");

    engine.tasks.lock().await.push_back(AgentTask {
        id: "approval-task-prompt".to_string(),
        title: "approval task".to_string(),
        description: "awaiting approval".to_string(),
        status: TaskStatus::Queued,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now_millis(),
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.clone()),
        source: "managed_command".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: Some("rm -rf /tmp/demo".to_string()),
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
        blocked_reason: None,
        awaiting_approval_id: None,
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

    let pending_approval = ToolPendingApproval {
        approval_id: "approval-prompt-1".to_string(),
        execution_id: "exec-prompt-1".to_string(),
        command: "rm -rf /tmp/demo".to_string(),
        rationale: "demo risky action".to_string(),
        risk_level: "high".to_string(),
        blast_radius: "workspace".to_string(),
        reasons: vec!["destructive command".to_string()],
        session_id: None,
    };

    let helper_engine = engine.clone();
    let helper_thread_id = thread_id.clone();
    let helper_pending = pending_approval.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .mark_task_awaiting_approval("approval-task-prompt", &helper_thread_id, &helper_pending)
            .await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway approval prompt should be emitted")
        .expect("gateway approval prompt should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert!(request.content.contains("approval-prompt-1"));
    assert!(request.content.contains("approve-once"));
    assert!(request.content.contains("approve-session"));
    assert!(request.content.contains("deny"));
    assert!(request.content.contains("rm -rf /tmp/demo"));

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-prompt-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == "approval-task-prompt")
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::AwaitingApproval);
    assert_eq!(
        task.awaiting_approval_id.as_deref(),
        Some("approval-prompt-1")
    );

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[cfg(unix)]
#[tokio::test]
async fn daemon_respawns_gateway_process_when_enabled() {
    let root = make_test_root("daemon-respawns-gateway-process");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let gateway_path = make_gateway_test_binary(&root, "zorai-gateway-test");

    engine
        .maybe_spawn_gateway_with_path(&gateway_path)
        .await
        .expect("spawn gateway");

    let proc_guard = engine.gateway_process.lock().await;
    assert!(proc_guard.is_some(), "gateway process should be running");
    drop(proc_guard);

    let attempts = engine.gateway_restart_attempts().await;
    assert_eq!(attempts, 0);

    engine.stop_gateway().await;
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn daemon_gateway_restart_backoff_applies_after_ipc_loss() {
    let root = make_test_root("daemon-gateway-restart-backoff");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let gateway_path = make_gateway_test_binary(&root, "zorai-gateway-test");

    engine
        .maybe_spawn_gateway_with_path(&gateway_path)
        .await
        .expect("spawn gateway");
    engine.record_gateway_ipc_loss("ipc lost").await;

    assert!(engine.gateway_process.lock().await.is_none());
    let deadline = engine.gateway_restart_not_before_ms().await;
    assert!(deadline.is_some(), "restart deadline should be scheduled");

    engine.stop_gateway().await;
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn daemon_gateway_reload_requests_clean_restart() {
    let root = make_test_root("daemon-gateway-reload");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let gateway_path = make_gateway_test_binary(&root, "zorai-gateway-test");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    engine
        .reinit_gateway_with_path(&gateway_path)
        .await
        .expect("reinit gateway");

    let msg = rx.recv().await.expect("expected reload command");
    assert!(matches!(
        msg,
        zorai_protocol::DaemonMessage::GatewayReloadCommand { .. }
    ));
    assert!(engine.gateway_process.lock().await.is_some());

    engine.stop_gateway().await;
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn gateway_config_reload_uses_spawn_restart_path_not_init_gateway() {
    let root = make_test_root("gateway-config-reload");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.gateway.slack_token = "slack-token".to_string();
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let gateway_path = make_gateway_test_binary(&root, "zorai-gateway-test");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    engine
        .reinit_gateway_with_path(&gateway_path)
        .await
        .expect("reinit gateway");

    let msg = rx.recv().await.expect("expected reload command");
    assert!(matches!(
        msg,
        zorai_protocol::DaemonMessage::GatewayReloadCommand { .. }
    ));
    assert!(engine.gateway_state.lock().await.is_some());
    assert!(engine.gateway_process.lock().await.is_some());

    engine.stop_gateway().await;
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn gateway_fast_path_reply_creates_thread_binding_and_history() {
    let root = make_test_root("gateway-fast-path-persists-history");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let msg = super::gateway::IncomingMessage {
        platform: "Discord".into(),
        sender: "alice".into(),
        content: "switch to svarog".into(),
        channel: "D123".into(),
        message_id: Some("discord:msg-1".into()),
        thread_context: Some(super::gateway::ThreadContext {
            discord_message_id: Some("discord:msg-1".into()),
            ..Default::default()
        }),
    };
    let thread_id = engine
        .persist_gateway_fast_path_exchange(
            "Discord:D123",
            &msg,
            &gateway_route_confirmation(gateway::GatewayRouteMode::Swarog),
        )
        .await
        .expect("persist fast-path exchange");

    let messages = engine
        .history
        .list_recent_messages(&thread_id, 10)
        .await
        .expect("list thread messages");
    assert!(messages
        .iter()
        .any(|msg| msg.role == "user" && msg.content == "switch to svarog"));
    assert!(messages.iter().any(|msg| {
        msg.role == "assistant"
            && msg.content == gateway_route_confirmation(gateway::GatewayRouteMode::Swarog)
    }));
    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn inbound_gateway_messages_reuse_same_thread_binding_for_same_slack_channel() {
    let root = make_test_root("gateway-slack-thread-binding-continuity");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let first = super::gateway::IncomingMessage {
        platform: "slack".into(),
        sender: "alice".into(),
        content: "first message".into(),
        channel: "C123".into(),
        message_id: Some("slack:msg-1".into()),
        thread_context: Some(super::gateway::ThreadContext {
            slack_thread_ts: Some("1712345678.000100".into()),
            ..Default::default()
        }),
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Slack:C123", &first, "first reply")
        .await
        .expect("persist first fast-path exchange");

    engine
        .gateway_threads
        .write()
        .await
        .insert("Slack:C123".to_string(), thread_id.clone());

    let second = super::gateway::IncomingMessage {
        platform: "Slack".into(),
        sender: "alice".into(),
        content: "second message".into(),
        channel: "C123".into(),
        message_id: Some("slack:msg-2".into()),
        thread_context: Some(super::gateway::ThreadContext {
            slack_thread_ts: Some("1712345678.000200".into()),
            ..Default::default()
        }),
    };

    engine
        .enqueue_gateway_message(second)
        .await
        .expect("enqueue second inbound message");
    engine.process_gateway_messages().await;
    await_gateway_idle(&engine).await;

    let bound_thread_id = engine
        .gateway_threads
        .read()
        .await
        .get("Slack:C123")
        .cloned()
        .expect("binding should exist");
    assert_eq!(bound_thread_id, thread_id);

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    let user_messages = thread
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert!(user_messages.contains(&"first message"));
    assert!(user_messages.contains(&"second message"));

    let reply_context = engine
        .gateway_state
        .lock()
        .await
        .as_ref()
        .and_then(|state| state.reply_contexts.get("Slack:C123").cloned())
        .expect("reply context should be retained for channel");
    assert_eq!(
        reply_context.slack_thread_ts.as_deref(),
        Some("1712345678.000200")
    );

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn inbound_gateway_messages_reuse_same_thread_binding_for_same_telegram_chat() {
    let root = make_test_root("gateway-telegram-thread-binding-continuity");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let first = super::gateway::IncomingMessage {
        platform: "Telegram".into(),
        sender: "alice".into(),
        content: "first telegram".into(),
        channel: "123456".into(),
        message_id: Some("tg:1".into()),
        thread_context: Some(super::gateway::ThreadContext {
            telegram_message_id: Some(777),
            ..Default::default()
        }),
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Telegram:123456", &first, "first reply")
        .await
        .expect("persist first fast-path exchange");

    engine
        .gateway_threads
        .write()
        .await
        .insert("Telegram:123456".to_string(), thread_id.clone());

    let second = super::gateway::IncomingMessage {
        platform: "telegram".into(),
        sender: "alice".into(),
        content: "second telegram".into(),
        channel: "123456".into(),
        message_id: Some("tg:2".into()),
        thread_context: Some(super::gateway::ThreadContext {
            telegram_message_id: Some(778),
            ..Default::default()
        }),
    };

    engine
        .enqueue_gateway_message(second)
        .await
        .expect("enqueue second inbound message");
    engine.process_gateway_messages().await;
    await_gateway_idle(&engine).await;

    let bound_thread_id = engine
        .gateway_threads
        .read()
        .await
        .get("Telegram:123456")
        .cloned()
        .expect("binding should exist");
    assert_eq!(bound_thread_id, thread_id);

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    let user_messages = thread
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert!(user_messages.contains(&"first telegram"));
    assert!(user_messages.contains(&"second telegram"));

    let reply_context = engine
        .gateway_state
        .lock()
        .await
        .as_ref()
        .and_then(|state| state.reply_contexts.get("Telegram:123456").cloned())
        .expect("reply context should be retained for chat");
    assert_eq!(reply_context.telegram_message_id, Some(778));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn inbound_gateway_messages_reuse_same_thread_binding_for_same_discord_dm_alias() {
    let root = make_test_root("gateway-discord-thread-binding-continuity");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let first = super::gateway::IncomingMessage {
        platform: "Discord".into(),
        sender: "alice".into(),
        content: "first discord".into(),
        channel: "user:123456789".into(),
        message_id: Some("discord:1".into()),
        thread_context: Some(super::gateway::ThreadContext {
            discord_message_id: Some("111".into()),
            ..Default::default()
        }),
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &first, "first reply")
        .await
        .expect("persist first fast-path exchange");

    engine
        .gateway_threads
        .write()
        .await
        .insert("Discord:user:123456789".to_string(), thread_id.clone());

    let second = super::gateway::IncomingMessage {
        platform: "discord".into(),
        sender: "alice".into(),
        content: "second discord".into(),
        channel: "user:123456789".into(),
        message_id: Some("discord:2".into()),
        thread_context: Some(super::gateway::ThreadContext {
            discord_message_id: Some("222".into()),
            ..Default::default()
        }),
    };

    engine
        .enqueue_gateway_message(second)
        .await
        .expect("enqueue second inbound message");
    engine.process_gateway_messages().await;
    await_gateway_idle(&engine).await;

    let bound_thread_id = engine
        .gateway_threads
        .read()
        .await
        .get("Discord:user:123456789")
        .cloned()
        .expect("binding should exist");
    assert_eq!(bound_thread_id, thread_id);

    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    let user_messages = thread
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert!(user_messages.contains(&"first discord"));
    assert!(user_messages.contains(&"second discord"));

    let reply_context = engine
        .gateway_state
        .lock()
        .await
        .as_ref()
        .and_then(|state| state.reply_contexts.get("Discord:user:123456789").cloned())
        .expect("reply context should be retained for dm alias");
    assert_eq!(reply_context.discord_message_id.as_deref(), Some("222"));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_status_command_reports_workspace_thread_task_state() {
    let root = make_test_root("gateway-status-command-workspace-thread-task");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = super::gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need status".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-status-thread-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Ready")
        .await
        .expect("persist fast-path exchange");

    let created = engine
        .create_workspace_task(
            zorai_protocol::WorkspaceTaskCreate {
                workspace_id: "main".to_string(),
                title: "Remote status thread task".to_string(),
                task_type: zorai_protocol::WorkspaceTaskType::Thread,
                description: "Check remote status command".to_string(),
                definition_of_done: None,
                priority: Some(zorai_protocol::WorkspacePriority::High),
                assignee: Some(zorai_protocol::WorkspaceActor::Agent(
                    zorai_protocol::AGENT_ID_SWAROG.to_string(),
                )),
                reviewer: Some(zorai_protocol::WorkspaceActor::User),
            },
            zorai_protocol::WorkspaceActor::User,
        )
        .await
        .expect("create workspace task");

    let mut task = engine
        .get_workspace_task(&created.id)
        .await
        .expect("read workspace task")
        .expect("workspace task should exist");
    task.thread_id = Some(thread_id.clone());
    task.status = zorai_protocol::WorkspaceTaskStatus::InProgress;
    task.started_at = Some(now_millis());
    task.updated_at = now_millis();
    engine
        .history
        .upsert_workspace_task(&task)
        .await
        .expect("persist workspace task");

    let status_msg = super::gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "!status".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-status-thread-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(status_msg)
            .await
            .expect("enqueue status should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway status reply should be emitted")
        .expect("gateway status reply should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    let lower = request.content.to_ascii_lowercase();
    assert!(
        lower.contains("remote status thread task"),
        "{}",
        request.content
    );
    assert!(lower.contains("in progress"), "{}", request.content);
    assert!(
        lower.contains(&created.id.to_ascii_lowercase()),
        "{}",
        request.content
    );

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-status-thread-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");
    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_status_command_reports_goal_run_state_for_thread() {
    let root = make_test_root("gateway-status-command-goal-run");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = super::gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need goal status".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-status-goal-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Ready")
        .await
        .expect("persist fast-path exchange");

    engine.goal_runs.lock().await.push_back(GoalRun {
        id: "goal-status-1".to_string(),
        title: "Remote status goal".to_string(),
        goal: "Test remote status goal command".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Paused,
        priority: TaskPriority::High,
        created_at: now_millis(),
        updated_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        thread_id: Some(thread_id.clone()),
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("step-1".to_string()),
        current_step_kind: Some(GoalRunStepKind::Command),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("Remote status plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        step_failure_history: Vec::new(),
        stopped_reason: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        pending_review_report: None,
        duration_ms: None,
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: super::super::autonomy::AutonomyLevel::Supervised,
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        supervision_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
        steps: Vec::new(),
        events: Vec::new(),
    });
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();

    let status_msg = super::gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "!status".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-status-goal-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(status_msg)
            .await
            .expect("enqueue status should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway status reply should be emitted")
        .expect("gateway status reply should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    let lower = request.content.to_ascii_lowercase();
    assert!(lower.contains("remote status goal"), "{}", request.content);
    assert!(
        lower.contains("goal paused") || lower.contains("paused"),
        "{}",
        request.content
    );
    assert!(lower.contains("goal-status-1"), "{}", request.content);

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-status-goal-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");
    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_status_command_reports_no_active_work_for_thread() {
    let root = make_test_root("gateway-status-command-no-active-work");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let seed = super::gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Need idle status".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-seed-status-idle-1".to_string()),
        thread_context: None,
    };

    engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &seed, "Ready")
        .await
        .expect("persist fast-path exchange");

    let status_msg = super::gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "!status".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-status-idle-1".to_string()),
        thread_context: None,
    };

    let helper_engine = engine.clone();
    let helper_task = tokio::spawn(async move {
        helper_engine
            .enqueue_gateway_message(status_msg)
            .await
            .expect("enqueue status should succeed");
        helper_engine.process_gateway_messages().await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway status reply should be emitted")
        .expect("gateway status reply should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    let lower = request.content.to_ascii_lowercase();
    assert!(
        lower.contains("no active") || lower.contains("nothing active"),
        "{}",
        request.content
    );

    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-status-idle-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    helper_task.await.expect("helper task should join");
    fs::remove_dir_all(&root).expect("cleanup test root");
}
