use super::*;
use zorai_shared::providers::{PROVIDER_ID_ANTHROPIC, PROVIDER_ID_OPENAI};

#[tokio::test]
async fn send_message_request_uses_spawned_persona_identity_in_continuity_summary() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_recording_assistant_server(recorded_bodies.clone()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-spawned-runtime-continuity";
    let spawned_persona_id = crate::agent::agent_identity::RADOGOST_AGENT_ID;
    let spawned_persona_name = crate::agent::agent_identity::RADOGOST_AGENT_NAME;
    let override_prompt = format!(
        "{} {}\n{} {}\nYou are {} ({}) operating as a spawned zorai agent.",
        crate::agent::agent_identity::PERSONA_MARKER,
        spawned_persona_name,
        crate::agent::agent_identity::PERSONA_ID_MARKER,
        spawned_persona_id,
        spawned_persona_name,
        spawned_persona_id,
    );

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Spawned runtime continuity thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Investigate the failure",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(crate::agent::types::AgentTask {
            id: "task-spawned-runtime-continuity".to_string(),
            title: "Investigate failure".to_string(),
            description: "Inspect the failing path".to_string(),
            status: TaskStatus::InProgress,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: Some(1),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: Some("goal-spawned-runtime-1".to_string()),
            goal_run_title: Some("Spawned goal".to_string()),
            goal_step_id: Some("step-spawned-runtime-1".to_string()),
            goal_step_title: Some("Investigate failure".to_string()),
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 1,
            max_retries: 2,
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
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: Some(override_prompt),
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        });
    }

    {
        let mut stores = engine.episodic_store.write().await;
        let store = stores.entry(spawned_persona_id.to_string()).or_default();
        store.counter_who.current_focus = Some("Tool: read_file".to_string());
    }

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Investigate the failure",
            None,
            Some("task-spawned-runtime-continuity"),
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    assert!(!outcome.interrupted_for_approval);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant bodies");
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Working Continuity")),
        "expected the execution prompt to include a continuity summary for spawned personas",
    );
    assert!(
        recorded.iter().any(|body| body.contains(&format!(
            "I am carrying this forward as {spawned_persona_name}."
        ))),
        "expected continuity summary to use the spawned persona name",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("comparing tradeoffs")),
        "expected continuity summary to include the spawned persona guidance",
    );
    assert!(
        recorded.iter().all(
            |body| !body.contains(&format!("I am carrying this forward as {MAIN_AGENT_NAME}."))
        ),
        "spawned persona continuity should not fall back to the main agent name",
    );
}

#[tokio::test]
async fn auto_compaction_forces_connection_close_on_next_llm_request() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_recording_request_server(recorded_requests.clone()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-compaction-connection-close";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Compaction transport reset thread".to_string(),
                messages: vec![
                    crate::agent::types::AgentMessage::user("First request", 1),
                    crate::agent::types::AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-1".to_string(),
                        role: MessageRole::Assistant,
                        content: "Observed earlier state".to_string(),
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
                        timestamp: 2,
                        feedback: None,
                    },
                    crate::agent::types::AgentMessage::user("Need a fresh request boundary", 3),
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Continue after compaction",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    assert!(!outcome.interrupted_for_approval);

    let recorded = recorded_requests.lock().expect("lock recorded requests");
    let request = recorded
        .back()
        .expect("expected one recorded request")
        .to_ascii_lowercase();
    assert!(
        request.contains("connection: close"),
        "expected compaction-boundary request to disable keep-alive, got: {request}"
    );
}

#[tokio::test]
async fn anthropic_send_message_persists_upstream_message_on_assistant_turn() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind anthropic upstream message server");
    let addr = listener
        .local_addr()
        .expect("anthropic upstream message server addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request(&mut socket, "anthropic upstream message request").await;
        let response_body = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_upstream_persisted\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":2}}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello from Claude.\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write anthropic upstream message response");
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_ANTHROPIC.to_string();
    config.base_url = format!("http://{addr}/anthropic");
    config.model = "claude-sonnet-4-20250514".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-anthropic-upstream-persisted";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Anthropic upstream persisted".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "hello",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    assert!(!outcome.interrupted_for_approval);

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    let assistant = thread
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .expect("assistant message should be stored");
    let upstream = assistant
        .upstream_message
        .as_ref()
        .expect("upstream message should be preserved");

    assert_eq!(
        assistant.response_id.as_deref(),
        Some("msg_upstream_persisted")
    );
    assert_eq!(upstream.id.as_deref(), Some("msg_upstream_persisted"));
    assert_eq!(upstream.model.as_deref(), Some("claude-sonnet-4-20250514"));
    assert_eq!(upstream.content_blocks.len(), 1);
    assert_eq!(
        upstream.content_blocks[0].text.as_deref(),
        Some("Hello from Claude.")
    );
}

#[tokio::test]
async fn anthropic_send_message_outcome_exposes_upstream_message() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind anthropic upstream outcome server");
    let addr = listener
        .local_addr()
        .expect("anthropic upstream outcome server addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request(&mut socket, "anthropic upstream outcome request").await;
        let response_body = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_upstream_outcome\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":3}}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Outcome surface\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write anthropic upstream outcome response");
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_ANTHROPIC.to_string();
    config.base_url = format!("http://{addr}/anthropic");
    config.model = "claude-sonnet-4-20250514".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-anthropic-upstream-outcome";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Anthropic upstream outcome".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "hello",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let upstream = outcome
        .upstream_message
        .as_ref()
        .expect("outcome should expose upstream message");
    assert_eq!(upstream.id.as_deref(), Some("msg_upstream_outcome"));
    assert_eq!(upstream.content_blocks.len(), 1);
    assert_eq!(
        upstream.content_blocks[0].text.as_deref(),
        Some("Outcome surface")
    );
}

#[tokio::test]
async fn anthropic_done_event_exposes_upstream_message() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind anthropic upstream event server");
    let addr = listener
        .local_addr()
        .expect("anthropic upstream event server addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request(&mut socket, "anthropic upstream event request").await;
        let response_body = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_upstream_event\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":4}}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Event surface\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":6}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write anthropic upstream event response");
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_ANTHROPIC.to_string();
    config.base_url = format!("http://{addr}/anthropic");
    config.model = "claude-sonnet-4-20250514".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-anthropic-upstream-event";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Anthropic upstream event".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    engine
        .send_message_inner(
            Some(thread_id),
            "hello",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let mut done_upstream = None;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::Done {
            upstream_message, ..
        } = event
        {
            done_upstream = upstream_message;
        }
    }

    let upstream = done_upstream.expect("done event should expose upstream message");
    assert_eq!(upstream.id.as_deref(), Some("msg_upstream_event"));
    assert_eq!(upstream.content_blocks.len(), 1);
    assert_eq!(
        upstream.content_blocks[0].text.as_deref(),
        Some("Event surface")
    );
}
