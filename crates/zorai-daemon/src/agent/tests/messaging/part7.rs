use super::*;
use tempfile::TempDir;
use tokio::sync::Notify;
use tokio::time::{timeout, Duration};
use zorai_shared::providers::PROVIDER_ID_OPENAI;

async fn make_runner_test_engine(config: AgentConfig) -> (Arc<AgentEngine>, TempDir) {
    let temp_dir = TempDir::new().expect("temp dir");
    let session_manager = SessionManager::new_test(temp_dir.path()).await;
    let history = HistoryStore::new_test_store(temp_dir.path())
        .await
        .expect("history store");
    let data_dir = temp_dir.path().join("agent");
    std::fs::create_dir_all(&data_dir).expect("create agent data dir");
    let engine = AgentEngine::new_with_storage_and_http_client(
        session_manager,
        config,
        history,
        data_dir,
        reqwest::Client::new(),
    );
    (engine, temp_dir)
}

#[tokio::test]
async fn participant_runner_enqueues_suggestion() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: no\\nMESSAGE: Verify claim X before sending.\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":7},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner".to_string(),
            messages: vec![AgentMessage::user("Check claim X", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    {
        let mut streams = engine.stream_cancellations.lock().await;
        streams.insert(
            thread_id.to_string(),
            StreamCancellationEntry {
                generation: 1,
                token: CancellationToken::new(),
                retry_now: Arc::new(Notify::new()),
                started_at: 1,
                last_progress_at: 1,
                last_progress_kind: StreamProgressKind::Started,
                last_progress_excerpt: String::new(),
            },
        );
    }

    timeout(
        Duration::from_secs(2),
        engine.run_participant_observers(thread_id),
    )
    .await
    .expect("participant observers should complete without stalling")
    .expect("participant observers should run");

    let suggestions = engine.list_thread_participant_suggestions(thread_id).await;
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].instruction, "Verify claim X before sending.");
}

#[tokio::test]
async fn participant_runner_force_send_posts_visible_message_and_continues_thread() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        let mut request_count = 0usize;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _body = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            request_count += 1;
            let response_body = match request_count {
                1 => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_observer\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: yes\\nMESSAGE: Visible participant post.\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_observer\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":7},\"error\":null}}\n\n"
                ),
                2 => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_follow_up\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Swarozyc follow-up.\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_follow_up\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                ),
                _ => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_tail\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_tail\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1},\"error\":null}}\n\n"
                ),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner_force";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner force".to_string(),
            messages: vec![AgentMessage::user("Check claim X", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    timeout(
        Duration::from_secs(2),
        engine.run_participant_observers(thread_id),
    )
    .await
    .expect("participant observers should complete without stalling")
    .expect("participant observers should run");

    let threads = engine.threads.read().await;
    let thread = threads
        .get(thread_id)
        .expect("thread should still exist after participant send");
    let participant_idx = thread
        .messages
        .iter()
        .position(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() == Some("weles")
        })
        .expect("force-send participant post should stay on the visible thread");
    let assistant = &thread.messages[participant_idx];

    assert_eq!(assistant.content, "Visible participant post.");
    assert_eq!(assistant.author_agent_name.as_deref(), Some("Weles"));
    assert_eq!(
        thread
            .messages
            .iter()
            .filter(|message| message.role == MessageRole::User)
            .count(),
        1,
        "force-send participant posts should not add an extra user turn"
    );
    let follow_up = thread.messages[participant_idx + 1..]
        .iter()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() != Some("weles")
        })
        .expect("force-send participant post should trigger a separate active-agent follow-up");
    assert_eq!(follow_up.content, "Swarozyc follow-up.");
    drop(threads);

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        "weles",
    );
    assert!(
        engine.get_thread(&dm_thread_id).await.is_none(),
        "force-send participant posts should not create a hidden DM thread"
    );

    let suggestions = engine.list_thread_participant_suggestions(thread_id).await;
    assert!(
        suggestions.is_empty(),
        "force-send participant posts should not leave queued or failed suggestions behind"
    );
}

#[tokio::test]
async fn participant_runner_force_send_persists_across_reload() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        let mut request_count = 0usize;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            request_count += 1;
            let response_body = match request_count {
                1 => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_reload_observer\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: yes\\nMESSAGE: Visible participant post.\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_reload_observer\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":7},\"error\":null}}\n\n"
                ),
                2 => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_reload_follow_up\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Swarozyc follow-up.\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_reload_follow_up\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                ),
                3 => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_reload_later\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Later visible thread progress.\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_reload_later\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":3,\"output_tokens\":3},\"error\":null}}\n\n"
                ),
                _ => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_reload_tail\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_reload_tail\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1},\"error\":null}}\n\n"
                ),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, temp_dir) = make_runner_test_engine(config.clone()).await;
    let thread_id = "thread_participant_runner_force_reload";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner force reload".to_string(),
            messages: vec![AgentMessage::user("Check claim X", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    timeout(
        Duration::from_secs(2),
        engine.run_participant_observers(thread_id),
    )
    .await
    .expect("participant observers should complete without stalling")
    .expect("participant observers should run");

    engine
        .resend_existing_user_message(thread_id, "Check claim X")
        .await
        .expect("later resend should succeed");

    let live_messages = engine
        .get_thread(thread_id)
        .await
        .expect("live thread should exist")
        .messages;

    drop(engine);

    let session_manager = SessionManager::new_test(temp_dir.path()).await;
    let history = HistoryStore::new_test_store(temp_dir.path())
        .await
        .expect("history store");
    let data_dir = temp_dir.path().join("agent");
    let reloaded = AgentEngine::new_with_storage_and_http_client(
        session_manager,
        config,
        history,
        data_dir,
        reqwest::Client::new(),
    );
    reloaded.hydrate().await.expect("hydrate");

    let reloaded_messages = reloaded
        .get_thread(thread_id)
        .await
        .expect("reloaded thread should exist")
        .messages;

    assert_eq!(
        reloaded_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        live_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn participant_runner_uses_single_visible_snapshot_per_cycle() {
    let recorded_bodies = Arc::new(StdMutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn({
        let recorded_bodies = recorded_bodies.clone();
        async move {
            let mut participant_prompt_count = 0usize;

            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let body = read_http_request_body(&mut socket)
                    .await
                    .expect("read request");
                recorded_bodies
                    .lock()
                    .expect("lock request log")
                    .push(body.clone());
                let response_body = if body.contains("Role: participant observer") {
                    participant_prompt_count += 1;
                    match participant_prompt_count {
                        1 => concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_1\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: yes\\nMESSAGE: First participant visible post.\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_1\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":7},\"error\":null}}\n\n"
                        ),
                        2 => concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_2\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: no\\nMESSAGE: Second participant follow-up.\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_2\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":5,\"output_tokens\":3},\"error\":null}}\n\n"
                        ),
                        _ => concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_tail\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_tail\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1},\"error\":null}}\n\n"
                        ),
                    }
                } else if body.contains("First participant visible post.") {
                    concat!(
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_visible\"}}\n\n",
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"First participant visible post.\"}\n\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_visible\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                    )
                } else {
                    concat!(
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_other\"}}\n\n",
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_other\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1},\"error\":null}}\n\n"
                    )
                };

                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner_snapshot";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner snapshot".to_string(),
            messages: vec![AgentMessage::user("Check claim X", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("first participant should register");
    engine
        .upsert_thread_participant(thread_id, "swarog", "verify the second detail")
        .await
        .expect("second participant should register");

    engine
        .run_participant_observers(thread_id)
        .await
        .expect("participant observers should run");

    let visible_snapshot_bodies: Vec<String> = recorded_bodies
        .lock()
        .expect("lock request log")
        .iter()
        .filter(|body| {
            body.contains("Role: participant observer") && body.contains("verify the second detail")
        })
        .cloned()
        .collect();

    assert!(
        !visible_snapshot_bodies.is_empty(),
        "expected a prompt body for the second participant"
    );
    assert!(
        visible_snapshot_bodies
            .iter()
            .all(|body| !body.contains("First participant visible post.")),
        "the second participant should use the original visible-thread snapshot for the same observer cycle"
    );
}

#[tokio::test]
async fn participant_runner_skips_no_suggestion() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_none\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"NO_SUGGESTION\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_none\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner_none";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner none".to_string(),
            messages: vec![AgentMessage::user("noop", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    engine
        .run_participant_observers(thread_id)
        .await
        .expect("participant observers should run");

    assert!(engine
        .list_thread_participant_suggestions(thread_id)
        .await
        .is_empty());
}

#[tokio::test]
async fn participant_runner_skips_structured_no_suggestion() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_structured_none\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: no\\nMESSAGE: NO_SUGGESTION\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_structured_none\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":3},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner_structured_none";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner structured none".to_string(),
            messages: vec![AgentMessage::user("noop", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    engine
        .run_participant_observers(thread_id)
        .await
        .expect("participant observers should run");

    assert!(engine
        .list_thread_participant_suggestions(thread_id)
        .await
        .is_empty());
}

#[tokio::test]
async fn participant_runner_skips_narrated_no_suggestion() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_narrated_none\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"**Observer: Radogost - Status Update**\\n\\nObserved four consecutive NO_SUGGESTION cycles with no forward progress.\\n\\n**Response:** `NO_SUGGESTION`\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_narrated_none\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":9,\"output_tokens\":7},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner_narrated_none";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner narrated none".to_string(),
            messages: vec![AgentMessage::user("noop", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    engine
        .run_participant_observers(thread_id)
        .await
        .expect("participant observers should run");

    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "narrated NO_SUGGESTION observer replies should not queue a participant suggestion"
    );
}

#[tokio::test]
async fn participant_observer_runs_after_thread_owner_assistant_message() {
    let recorded_bodies = Arc::new(StdMutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn({
        let recorded_bodies = recorded_bodies.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let body = read_http_request_body(&mut socket)
                    .await
                    .expect("read request");
                recorded_bodies.lock().expect("lock request log").push(body);
                let response_body = concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_owner_tail\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"NO_SUGGESTION\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_owner_tail\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_owner_tail";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant owner tail".to_string(),
            messages: vec![
                AgentMessage::user("Keep going", 1),
                AgentMessage {
                    reasoning_tokens: None,
                    thinking_ms: None,
                    id: "owner-assistant-tail".to_string(),
                    role: MessageRole::Assistant,
                    content: "I will continue with the next implementation step.".to_string(),
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
                    author_agent_id: Some(crate::agent::agent_identity::MAIN_AGENT_ID.to_string()),
                    author_agent_name: Some(
                        crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                    ),
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
            updated_at: 2,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    engine
        .run_participant_observers(thread_id)
        .await
        .expect("participant observers should run");

    let request_bodies = recorded_bodies.lock().expect("lock request log").clone();
    assert_eq!(request_bodies.len(), 1);
    assert!(request_bodies[0].contains("Role: participant observer"));
}

#[tokio::test]
async fn participant_observer_skips_cycle_when_latest_visible_message_is_participant_authored() {
    let recorded_bodies = Arc::new(StdMutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn({
        let recorded_bodies = recorded_bodies.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let body = read_http_request_body(&mut socket)
                    .await
                    .expect("read request");
                recorded_bodies.lock().expect("lock request log").push(body);
                let response_body = concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_self_tail\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"NO_SUGGESTION\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_self_tail\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_self_tail";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant self tail".to_string(),
            messages: vec![
                AgentMessage::user("Keep going", 1),
                AgentMessage {
                    reasoning_tokens: None,
                    thinking_ms: None,
                    id: "participant-assistant-tail".to_string(),
                    role: MessageRole::Assistant,
                    content: "I checked that claim already.".to_string(),
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
                    author_agent_id: Some("weles".to_string()),
                    author_agent_name: Some("Weles".to_string()),
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
            updated_at: 2,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    engine
        .run_participant_observers(thread_id)
        .await
        .expect("participant observers should run");

    assert!(
        recorded_bodies.lock().expect("lock request log").is_empty(),
        "participant-authored tail messages should not reopen the observer cycle"
    );
}

#[tokio::test]
async fn apply_participant_command_runs_initial_observer_review() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_command_review\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: no\\nMESSAGE: Initial review from current thread.\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_command_review\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":5,\"output_tokens\":7},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_initial_review";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Initial participant review".to_string(),
            messages: vec![AgentMessage::user("Review the current thread", 1)],
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

    engine
        .apply_thread_participant_command(
            thread_id,
            "weles",
            "upsert",
            Some("review from the current thread"),
        )
        .await
        .expect("participant command should succeed");

    let thread_messages = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should still exist after participant command")
            .messages
            .clone()
    };
    assert!(thread_messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.author_agent_id.as_deref() == Some("weles")
            && message.content == "Initial review from current thread."
    }));
    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "initial observer suggestion should be released instead of staying queued"
    );
}

#[tokio::test]
async fn participant_suggestions_not_visible_to_participants() {
    let (engine, _temp_dir) = make_runner_test_engine(AgentConfig::default()).await;
    let thread_id = "thread_participant_prompt_hidden_suggestion";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Prompt visibility".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    {
        let mut streams = engine.stream_cancellations.lock().await;
        streams.insert(
            thread_id.to_string(),
            StreamCancellationEntry {
                generation: 1,
                token: CancellationToken::new(),
                retry_now: Arc::new(Notify::new()),
                started_at: 1,
                last_progress_at: 1,
                last_progress_kind: StreamProgressKind::Started,
                last_progress_excerpt: String::new(),
            },
        );
    }
    engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Hidden", false)
        .await
        .expect("queue suggestion");

    let prompt = engine
        .build_participant_prompt(thread_id, "weles")
        .await
        .expect("prompt should build");
    assert!(!prompt.contains("Hidden"));
}

#[tokio::test]
async fn participant_prompt_excludes_internal_delegation() {
    let (engine, _temp_dir) = make_runner_test_engine(AgentConfig::default()).await;
    let thread_id = "thread_participant_prompt_internal_delegate";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Prompt visibility".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    engine
        .append_internal_delegate_message(thread_id, "secret")
        .await
        .expect("append internal delegate message");

    let prompt = engine
        .build_participant_prompt(thread_id, "weles")
        .await
        .expect("prompt should build");
    assert!(!prompt.contains("secret"));
}

#[tokio::test]
async fn participant_prompt_excludes_tool_messages() {
    let (engine, _temp_dir) = make_runner_test_engine(AgentConfig::default()).await;
    let thread_id = "thread_participant_prompt_tool_message";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Prompt visibility".to_string(),
            messages: vec![
                AgentMessage::user("hello", 1),
                AgentMessage {
                    reasoning_tokens: None,
                    thinking_ms: None,
                    id: "tool-message-1".to_string(),
                    role: MessageRole::Tool,
                    content: "tool output that should stay hidden".to_string(),
                    content_blocks: Vec::new(),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: Some("search_files".to_string()),
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
                    reasoning: Some("reasoning that should stay hidden".to_string()),
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
                AgentMessage {
                    reasoning_tokens: None,
                    thinking_ms: None,
                    id: "assistant-message-1".to_string(),
                    role: MessageRole::Assistant,
                    content: "assistant reply".to_string(),
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
                    reasoning: Some("assistant reasoning that should stay hidden".to_string()),
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    tool_output_preview_path: None,
                    structural_refs: Vec::new(),
                    pinned_for_compaction: false,
                    timestamp: 3,
                    feedback: None,
                },
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
            updated_at: 3,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    let prompt = engine
        .build_participant_prompt(thread_id, "weles")
        .await
        .expect("prompt should build");
    assert!(prompt.contains("- user: hello"));
    assert!(prompt.contains("- assistant: assistant reply"));
    assert!(!prompt.contains("tool output that should stay hidden"));
    assert!(!prompt.contains("reasoning that should stay hidden"));
    assert!(!prompt.contains("assistant reasoning that should stay hidden"));
}

#[tokio::test]
async fn participant_prompt_compacts_older_visible_messages() {
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 1;
    config.keep_recent_on_compact = 1;
    config.context_window_tokens = 4_096;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_prompt_compaction";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Prompt compaction".to_string(),
            messages: vec![
                AgentMessage::user("old user detail that should compact away", 1),
                AgentMessage {
                    reasoning_tokens: None,
                    thinking_ms: None,
                    id: "assistant-old-1".to_string(),
                    role: MessageRole::Assistant,
                    content: "older assistant detail that should compact away".to_string(),
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
                AgentMessage::user("latest user detail should remain", 3),
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
            updated_at: 3,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "review current thread")
        .await
        .expect("participant should register");

    let prompt = engine
        .build_participant_prompt(thread_id, "weles")
        .await
        .expect("prompt should build");
    assert!(prompt.contains("latest user detail should remain"));
    assert!(
        !prompt.contains("old user detail that should compact away"),
        "older visible user content should be compacted out of the observer prompt: {prompt}"
    );
    assert!(
        !prompt.contains("older assistant detail that should compact away"),
        "older visible assistant content should be compacted out of the observer prompt: {prompt}"
    );
}

#[tokio::test]
async fn participant_prompt_applies_bounded_snapshot_cap_under_large_context_budget() {
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 100;
    config.keep_recent_on_compact = 10;
    config.context_window_tokens = 205_000;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_prompt_snapshot_cap";

    let mut messages = Vec::new();
    for index in 0..60 {
        let timestamp = (index + 1) as u64;
        if index % 2 == 0 {
            messages.push(AgentMessage::user(
                &format!("visible-user-{index:02}"),
                timestamp,
            ));
        } else {
            messages.push(AgentMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: format!("assistant-{index:02}"),
                role: MessageRole::Assistant,
                content: format!("visible-assistant-{index:02}"),
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
                timestamp,
                feedback: None,
            });
        }
    }

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Prompt snapshot cap".to_string(),
            messages,
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 60,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "review current thread")
        .await
        .expect("participant should register");

    let prompt = engine
        .build_participant_prompt(thread_id, "weles")
        .await
        .expect("prompt should build");
    assert!(
        prompt.contains("visible-assistant-59"),
        "latest visible content should stay available to participant observers: {prompt}"
    );
    assert!(
        !prompt.contains("visible-user-00"),
        "participant observer prompts should drop the oldest visible content even when the main context budget is large: {prompt}"
    );
}

#[tokio::test]
async fn participant_prompt_requires_continuing_after_latest_assistant_message_until_work_finishes()
{
    let (engine, _temp_dir) = make_runner_test_engine(AgentConfig::default()).await;
    let thread_id = "thread_participant_prompt_requires_assistant_continuation";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Prompt continuation semantics".to_string(),
            messages: vec![
                AgentMessage::user(
                    "Implement the recovery fix and keep going until it's done",
                    1,
                ),
                AgentMessage {
                    reasoning_tokens: None,
                    thinking_ms: None,
                    id: "assistant-latest-action-plan".to_string(),
                    role: MessageRole::Assistant,
                    content: "I will inspect recovery.rs next, then patch the daemon flow."
                        .to_string(),
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
            updated_at: 2,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims and keep task momentum")
        .await
        .expect("participant should register");

    let prompt = engine
        .build_participant_prompt(thread_id, "weles")
        .await
        .expect("prompt should build");
    assert!(
        prompt.contains("Evaluate the thread after the latest visible message, even if that message is from the assistant."),
        "observer prompt should explicitly treat the latest assistant message as actionable context: {prompt}"
    );
    assert!(
        prompt.contains("If the operator asked for autonomous progress and work is still pending, you should suggest the next concrete participant action instead of NO_SUGGESTION."),
        "observer prompt should forbid premature NO_SUGGESTION while autonomous work remains: {prompt}"
    );
    assert!(
        prompt.contains("Return NO_SUGGESTION only when the visible thread is naturally complete, blocked on external input, or the participant truly has nothing useful to add."),
        "observer prompt should only allow NO_SUGGESTION for truly finished or blocked threads: {prompt}"
    );
}

#[tokio::test]
async fn participant_observer_executes_tools_in_hidden_playground_and_queues_only_final_message() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        let mut request_count = 0usize;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let body = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            request_count += 1;
            let response_body = match request_count {
                1 => {
                    assert!(
                        body.contains("Role: participant observer"),
                        "first request should start the hidden participant observer turn"
                    );
                    concat!(
                        "data: {\"id\":\"chatcmpl_participant_playground_tool_1\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_participant_list_threads\",\"function\":{\"name\":\"list_threads\",\"arguments\":\"{}\"}}]}}]}\n\n",
                        "data: {\"id\":\"chatcmpl_participant_playground_tool_1\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    )
                }
                _ => {
                    assert!(
                        body.contains("call_participant_list_threads"),
                        "follow-up request should include the hidden tool call result"
                    );
                    concat!(
                        "data: {\"id\":\"chatcmpl_participant_playground_tool_2\",\"choices\":[{\"delta\":{\"content\":\"FORCE: no\\nMESSAGE: Verified via tool.\"}}]}\n\n",
                        "data: {\"id\":\"chatcmpl_participant_playground_tool_2\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4}}\n\n",
                        "data: [DONE]\n\n"
                    )
                }
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 2;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_playground_tool_queue";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant playground tool queue".to_string(),
            messages: vec![AgentMessage::user("Check claim X", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    {
        let mut streams = engine.stream_cancellations.lock().await;
        streams.insert(
            thread_id.to_string(),
            StreamCancellationEntry {
                generation: 1,
                token: CancellationToken::new(),
                retry_now: Arc::new(Notify::new()),
                started_at: 1,
                last_progress_at: 1,
                last_progress_kind: StreamProgressKind::Started,
                last_progress_excerpt: String::new(),
            },
        );
    }

    timeout(
        Duration::from_secs(2),
        engine.run_participant_observers(thread_id),
    )
    .await
    .expect("participant observer should finish")
    .expect("participant observer should succeed");

    let suggestions = engine.list_thread_participant_suggestions(thread_id).await;
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].instruction, "Verified via tool.");

    let visible_thread = engine
        .get_thread(thread_id)
        .await
        .expect("visible thread should still exist");
    assert_eq!(
        visible_thread
            .messages
            .iter()
            .filter(|message| message.role == MessageRole::Assistant)
            .count(),
        0,
        "only the final queued participant suggestion should be surfaced, not hidden playground chatter"
    );
    assert!(
        visible_thread
            .messages
            .iter()
            .all(|message| !message.content.contains("[TOOL_CALL]")),
        "hidden tool-call markup must not leak into the visible thread"
    );

    let playground_thread_id =
        crate::agent::agent_identity::participant_playground_thread_id(thread_id, "weles");
    let playground_thread = engine
        .get_thread_filtered(&playground_thread_id, true, None, 0)
        .await
        .expect("hidden participant playground thread should exist")
        .thread;
    assert!(
        playground_thread
            .messages
            .iter()
            .any(|message| message.role == MessageRole::Tool),
        "tool execution should stay inside the hidden participant playground"
    );
    assert!(
        playground_thread.messages.iter().any(|message| {
            message.role == MessageRole::Assistant
                && message.content == "FORCE: no\nMESSAGE: Verified via tool."
        }),
        "the final hidden participant response should stay in the playground thread"
    );
}
