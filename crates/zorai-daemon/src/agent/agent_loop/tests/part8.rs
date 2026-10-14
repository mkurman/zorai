use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use zorai_shared::providers::{PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENAI};

#[tokio::test]
async fn truncated_stream_with_accumulated_content_still_emits_done_event() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind truncated stream server");
    let addr = listener.local_addr().expect("truncated stream server addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request(&mut socket, "truncated stream request").await;
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: text/event-stream\r\n",
            "cache-control: no-cache\r\n",
            "connection: close\r\n",
            "\r\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Recovered terminal text\"}}]}\n\n"
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write truncated stream response");
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-truncated-stream-done";
    engine.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: None,
            title: "Truncated stream done".to_string(),
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
        .expect("truncated stream should still complete the turn");

    let mut saw_done = false;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::Done {
            thread_id: event_thread_id,
            ..
        } = event
        {
            if event_thread_id == thread_id {
                saw_done = true;
            }
        }
    }

    assert!(saw_done, "truncated stream should still emit a done event");
    let thread = engine
        .get_thread(thread_id)
        .await
        .expect("thread should exist");
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Recovered terminal text")
    );
}

#[tokio::test]
async fn openai_context_window_stream_error_forces_compaction_and_retries_without_error_reply() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_count = Arc::new(AtomicUsize::new(0));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind context overflow retry server");
    let addr = listener
        .local_addr()
        .expect("context overflow retry server addr");

    tokio::spawn({
        let request_count = request_count.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let request_count = request_count.clone();
                tokio::spawn(async move {
                    let attempt = request_count.fetch_add(1, Ordering::SeqCst);
                    let _ = read_http_request(&mut socket, "context overflow retry request").await;
                    let body = if attempt == 0 {
                        concat!(
                            "data: {\"type\":\"error\",\"error\":{\"code\":\"context_length_exceeded\",\"message\":\"Your input exceeds the context window of this model. Please adjust your input and try again.\"}}\n\n"
                        )
                    } else {
                        concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_after_compaction\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Recovered after compaction\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_after_compaction\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":10,\"output_tokens\":4,\"total_tokens\":14},\"error\":null}}\n\n"
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write context overflow retry response");
                });
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.keep_recent_on_compact = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-context-overflow-compacts-and-recovers";
    engine.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: None,
            title: "Context overflow recovery".to_string(),
            messages: vec![
                crate::agent::types::AgentMessage::user("Older user context", 1),
                crate::agent::types::AgentMessage {
                    id: "assistant-older-context".to_string(),
                    role: MessageRole::Assistant,
                    content: "Older assistant context".to_string(),
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
                crate::agent::types::AgentMessage::user("Recent user context", 3),
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

    engine
        .send_message_inner(
            Some(thread_id),
            "Continue with a compacted prompt",
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
        .expect("context overflow should compact and retry");

    assert_eq!(request_count.load(Ordering::SeqCst), 2);
    let thread = engine
        .get_thread(thread_id)
        .await
        .expect("thread should exist");
    assert!(
        thread
            .messages
            .iter()
            .any(|message| message.message_kind == AgentMessageKind::CompactionArtifact),
        "expected forced compaction artifact before retry"
    );
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Recovered after compaction")
    );
    assert!(
        thread
            .messages
            .iter()
            .all(|message| !message.content.contains("context window of this model")),
        "raw provider context-window error must not be persisted as a user-visible reply"
    );
}

#[tokio::test]
async fn continuation_prompt_is_sent_once_not_after_every_tool_round() {
    let root = tempdir().expect("tempdir");
    let recorded = Arc::new(StdMutex::new(Vec::<String>::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let next = Arc::new(AtomicUsize::new(0));
    {
        let recorded = recorded.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let recorded = recorded.clone();
                let next = next.clone();
                tokio::spawn(async move {
                    let request = read_http_request(&mut socket, "continuation").await;
                    recorded.lock().unwrap().push(request_body(&request));
                    let chunk = if next.fetch_add(1, Ordering::SeqCst) == 0 {
                        serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_dt","function":{"name":"get_current_datetime","arguments":"{}"}}]}}]})
                    } else {
                        serde_json::json!({"choices":[{"delta":{"content":"Handled."},"finish_reason":"stop"}]})
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\ndata: {chunk}\n\ndata: [DONE]\n\n"
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                });
            }
        });
    }

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-continuation-once";
    let now = now_millis();
    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: None,
            title: "Continuation".to_string(),
            messages: vec![
                AgentMessage::user("audit the cases", now),
                {
                    let mut message = AgentMessage::user("Delegated to researcher.", now + 1);
                    message.role = MessageRole::Assistant;
                    message
                },
            ],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: now,
            updated_at: now,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );

    let notice = "CHILD-RESEARCHER-BUDGET-NOTICE";
    engine
        .send_message_inner_with_options(
            Some(thread_id),
            notice,
            None,
            None,
            None,
            None,
            Some(notice),
            None,
            None,
            false,
            true,
            false,
            false,
            None,
        )
        .await
        .expect("continuation turn should complete");

    let bodies = recorded.lock().unwrap().clone();
    assert!(bodies.len() >= 2, "expected a tool round and a follow-up request: {bodies:?}");
    for (index, body) in bodies.iter().enumerate() {
        assert_eq!(
            body.matches(notice).count(),
            1,
            "request {index} must carry the continuation prompt exactly once; appending it after every tool round makes the agent think the notice was redelivered: {body}"
        );
    }
    let follow_up: serde_json::Value = serde_json::from_str(&bodies[1]).expect("json body");
    let last = follow_up["messages"].as_array().and_then(|m| m.last()).expect("messages");
    assert_eq!(last["role"], "tool", "the follow-up must end with the tool result, not a repeated notice");
}
