use super::tests_part2_support::*;
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn concierge_welcome_request_does_not_block_ping() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake llm listener");
    let addr = listener.local_addr().expect("listener addr");
    let accept_task = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("accept concierge request");
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = format!("http://{addr}");
    config.api_key = "test-key".to_string();
    config.model = "gpt-5.4".to_string();
    config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;
    config.tier.onboarding_completed = true;

    let mut conn = spawn_test_connection_with_config(config).await;

    conn.framed
        .send(ClientMessage::AgentSubscribe)
        .await
        .expect("subscribe to agent events");
    conn.framed
        .send(ClientMessage::AgentRequestConciergeWelcome)
        .await
        .expect("request concierge welcome");
    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while concierge work is active");

    let pong_received = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv().await {
                DaemonMessage::Pong => return true,
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "concierge_welcome");
                    continue;
                }
                DaemonMessage::AgentEvent { .. } => continue,
                other => {
                    panic!("expected Pong while concierge work runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind concierge welcome generation"
    );

    accept_task.abort();
    conn.shutdown().await;
}

#[tokio::test]
async fn concierge_welcome_streams_partial_content_before_completion() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake llm listener");
    let addr = listener.local_addr().expect("listener addr");
    let accept_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept concierge request");
        let mut request_buf = [0u8; 4096];
        let _ = stream.read(&mut request_buf).await;

        let chunk_one =
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_partial\"}}\n\n";
        let chunk_two =
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Partial welcome\"}\n\n";
        let chunk_three = "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_partial\",\"usage\":{\"input_tokens\":7,\"output_tokens\":3}}}\n\n";
        let done = "data: [DONE]\n\n";
        let body_len = chunk_one.len() + chunk_two.len() + chunk_three.len() + done.len();

        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {body_len}\r\nconnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .expect("write response head");
        stream
            .write_all(chunk_one.as_bytes())
            .await
            .expect("write created event");
        stream
            .write_all(chunk_two.as_bytes())
            .await
            .expect("write partial delta");
        stream.flush().await.expect("flush partial delta");

        tokio::time::sleep(Duration::from_millis(800)).await;

        stream
            .write_all(chunk_three.as_bytes())
            .await
            .expect("write final chunk");
        stream
            .write_all(done.as_bytes())
            .await
            .expect("write stream terminator");
        stream.flush().await.expect("flush final stream");
    });

    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = format!("http://{addr}");
    config.api_key = "test-key".to_string();
    config.model = "gpt-5.4".to_string();
    config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;
    config.tier.onboarding_completed = true;

    let mut conn = spawn_test_connection_with_config(config).await;
    conn.framed
        .send(ClientMessage::AgentSubscribe)
        .await
        .expect("subscribe to agent events");
    conn.framed
        .send(ClientMessage::AgentRequestConciergeWelcome)
        .await
        .expect("request concierge welcome");

    let partial_event = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(250)).await {
                DaemonMessage::AgentEvent { event_json } => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&event_json).expect("parse concierge event");
                    if parsed.get("type").and_then(|value| value.as_str())
                        == Some("concierge_welcome")
                    {
                        return parsed;
                    }
                }
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "concierge_welcome");
                }
                other => panic!("expected concierge partial event, got {other:?}"),
            }
        }
    })
    .await
    .expect("partial concierge welcome should arrive before completion");

    assert_eq!(
        partial_event
            .get("content")
            .and_then(|value| value.as_str()),
        Some("Partial welcome")
    );
    assert_eq!(
        partial_event
            .get("actions")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(0)
    );

    accept_task.abort();
    conn.shutdown().await;
}

#[tokio::test]
async fn operation_status_query_survives_reconnect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake llm listener");
    let addr = listener.local_addr().expect("listener addr");
    let accept_task = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("accept concierge request");
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = format!("http://{addr}");
    config.api_key = "test-key".to_string();
    config.model = "gpt-5.4".to_string();
    config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;

    let mut conn = spawn_test_connection_with_config(config.clone()).await;
    declare_async_command_capability(&mut conn).await;
    conn.framed
        .send(ClientMessage::AgentRequestConciergeWelcome)
        .await
        .expect("request concierge welcome");

    let operation_id = match conn.recv().await {
        DaemonMessage::OperationAccepted { operation_id, .. } => operation_id,
        other => panic!("expected operation acceptance, got {other:?}"),
    };

    let mut reconnect = spawn_test_connection_with_config(config).await;
    declare_async_command_capability(&mut reconnect).await;
    reconnect
        .framed
        .send(ClientMessage::AgentGetOperationStatus {
            operation_id: operation_id.clone(),
        })
        .await
        .expect("query operation status after reconnect");

    match reconnect.recv().await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.operation_id, operation_id);
            assert_eq!(snapshot.kind, "concierge_welcome");
        }
        other => panic!("expected operation status snapshot after reconnect, got {other:?}"),
    }

    accept_task.abort();
    reconnect.shutdown().await;
    conn.shutdown().await;
}

#[tokio::test]
async fn second_client_can_query_accepted_operation_before_concierge_work_completes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake llm listener");
    let addr = listener.local_addr().expect("listener addr");
    let accept_task = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("accept concierge request");
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = format!("http://{addr}");
    config.api_key = "test-key".to_string();
    config.model = "gpt-5.4".to_string();
    config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;

    let mut primary = spawn_test_connection_with_config(config.clone()).await;
    declare_async_command_capability(&mut primary).await;
    primary
        .framed
        .send(ClientMessage::AgentRequestConciergeWelcome)
        .await
        .expect("request concierge welcome");

    let operation_id = match primary.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "concierge_welcome");
            operation_id
        }
        other => panic!("expected operation acceptance, got {other:?}"),
    };

    let mut second_client = spawn_test_connection_with_config(config).await;
    declare_async_command_capability(&mut second_client).await;
    second_client
        .framed
        .send(ClientMessage::AgentGetOperationStatus {
            operation_id: operation_id.clone(),
        })
        .await
        .expect("query accepted concierge operation from second client");

    match second_client
        .recv_with_timeout(Duration::from_millis(250))
        .await
    {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.operation_id, operation_id);
            assert_eq!(snapshot.kind, "concierge_welcome");
            assert!(matches!(
                snapshot.state,
                zorai_protocol::OperationLifecycleState::Accepted
                    | zorai_protocol::OperationLifecycleState::Started
            ));
        }
        other => panic!("expected immediate operation status snapshot, got {other:?}"),
    }

    accept_task.abort();
    second_client.shutdown().await;
    primary.shutdown().await;
}

#[test]
fn direct_message_response_includes_provider_final_result_json() {
    let message = DaemonMessage::AgentDirectMessageResponse {
        target: "main".to_string(),
        thread_id: "thread-1".to_string(),
        response: "protocol reply".to_string(),
        session_id: None,
        provider_final_result_json: Some(
            r#"{"provider":"open_ai_responses","id":"resp_dm_protocol"}"#.to_string(),
        ),
    };

    match message {
        DaemonMessage::AgentDirectMessageResponse {
            target,
            thread_id,
            response,
            provider_final_result_json,
            ..
        } => {
            assert_eq!(target, "main");
            assert_eq!(thread_id, "thread-1");
            assert_eq!(response, "protocol reply");
            let json = provider_final_result_json.expect("expected provider-native final result");
            let value: serde_json::Value =
                serde_json::from_str(&json).expect("parse final result json");
            assert_eq!(
                value.get("provider").and_then(|v| v.as_str()),
                Some("open_ai_responses")
            );
            assert_eq!(
                value.get("id").and_then(|v| v.as_str()),
                Some("resp_dm_protocol")
            );
        }
        other => panic!("expected direct message response, got {other:?}"),
    }
}

#[tokio::test]
async fn concierge_welcome_does_not_delay_provider_validation_acceptance() {
    let concierge_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake concierge listener");
    let concierge_addr = concierge_listener
        .local_addr()
        .expect("concierge listener addr");
    let concierge_accept_task = tokio::spawn(async move {
        let (_stream, _) = concierge_listener
            .accept()
            .await
            .expect("accept concierge request");
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let provider_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake provider listener");
    let provider_addr = provider_listener
        .local_addr()
        .expect("provider listener addr");
    let provider_accept_task = tokio::spawn(async move {
        let (_stream, _) = provider_listener
            .accept()
            .await
            .expect("accept provider request");
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = format!("http://{concierge_addr}");
    config.api_key = "test-key".to_string();
    config.model = "gpt-5.4".to_string();
    config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;

    let mut conn = spawn_test_connection_with_config(config).await;
    declare_async_command_capability(&mut conn).await;

    conn.framed
        .send(ClientMessage::AgentSubscribe)
        .await
        .expect("subscribe to agent events");
    conn.framed
        .send(ClientMessage::AgentRequestConciergeWelcome)
        .await
        .expect("request concierge welcome");

    let concierge_accepted = timeout(Duration::from_secs(2), async {
        loop {
            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::OperationAccepted { kind, .. } if kind == "concierge_welcome" => {
                    return true;
                }
                DaemonMessage::AgentEvent { .. } => continue,
                other => panic!("expected concierge acceptance, got {other:?}"),
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(concierge_accepted, "concierge welcome should be accepted");

    conn.framed
        .send(ClientMessage::AgentValidateProvider {
            provider_id: "openai".to_string(),
            base_url: format!("http://{provider_addr}"),
            api_key: "test-key".to_string(),
            auth_source: "api_key".to_string(),
        })
        .await
        .expect("request provider validation while concierge is active");

    let provider_accepted = timeout(Duration::from_millis(500), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(500)).await {
                    DaemonMessage::OperationAccepted { kind, .. } if kind == "provider_validation" => {
                        return true;
                    }
                    DaemonMessage::AgentEvent { .. } => continue,
                    other => panic!("expected provider validation acceptance while concierge is active, got {other:?}"),
                }
            }
        })
        .await
        .unwrap_or(false);

    assert!(
        provider_accepted,
        "provider validation should be accepted while concierge is active"
    );

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while concierge and provider work are active");

    let pong_received = timeout(Duration::from_millis(500), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(500)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::AgentEvent { .. } => continue,
                other => panic!(
                    "expected Pong while concierge and provider work are active, got {other:?}"
                ),
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked while concierge and provider work are active"
    );

    concierge_accept_task.abort();
    provider_accept_task.abort();
    conn.shutdown().await;
}

#[tokio::test]
async fn concierge_welcome_request_without_declared_capability_still_returns_operation_acceptance()
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake llm listener");
    let addr = listener.local_addr().expect("listener addr");
    let accept_task = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("accept concierge request");
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = format!("http://{addr}");
    config.api_key = "test-key".to_string();
    config.model = "gpt-5.4".to_string();
    config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;

    let mut conn = spawn_test_connection_with_config(config).await;
    conn.framed
        .send(ClientMessage::AgentSubscribe)
        .await
        .expect("subscribe to agent events");
    conn.framed
        .send(ClientMessage::AgentRequestConciergeWelcome)
        .await
        .expect("request concierge welcome without declared capability");

    let operation_id = timeout(Duration::from_secs(2), async {
        loop {
            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::OperationAccepted {
                    operation_id, kind, ..
                } => {
                    assert_eq!(kind, "concierge_welcome");
                    return operation_id;
                }
                DaemonMessage::AgentEvent { .. } => continue,
                other => panic!("expected concierge acceptance, got {other:?}"),
            }
        }
    })
    .await
    .expect("timed out waiting for concierge acceptance");

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while legacy concierge work is active");

    let pong_received = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(250)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::AgentEvent { .. } => continue,
                other => panic!("expected Pong while concierge runs in background, got {other:?}"),
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind concierge welcome without declared capability"
    );

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query concierge operation status");

    match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, "concierge_welcome");
        }
        other => panic!("expected concierge status snapshot, got {other:?}"),
    }

    accept_task.abort();
    conn.shutdown().await;
}

#[tokio::test]
async fn concierge_welcome_skips_onboarding_when_hydrated_history_exists() {
    let mut config = AgentConfig::default();
    config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::Minimal;
    config.model = "gpt-5".to_string();

    let mut conn = spawn_test_connection_with_config(config).await;
    let now = 1_735_000_000_000i64;

    conn.agent
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: "persisted-thread".to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Persisted Thread".to_string(),
            created_at: now - 10_000,
            updated_at: now - 5_000,
            message_count: 2,
            total_tokens: 42,
            last_preview: "last persisted answer".to_string(),
            metadata_json: None,
        })
        .await
        .expect("persist thread row");
    conn.agent
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "persisted-user".to_string(),
            thread_id: "persisted-thread".to_string(),
            created_at: now - 10_000,
            role: "user".to_string(),
            content: "resume the work".to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("persist user message");
    conn.agent
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "persisted-assistant".to_string(),
            thread_id: "persisted-thread".to_string(),
            created_at: now - 5_000,
            role: "assistant".to_string(),
            content: "last persisted answer".to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("persist assistant message");

    conn.agent
        .hydrate()
        .await
        .expect("hydrate persisted history");

    conn.framed
        .send(ClientMessage::AgentSubscribe)
        .await
        .expect("subscribe to agent events");
    conn.framed
        .send(ClientMessage::AgentRequestConciergeWelcome)
        .await
        .expect("request concierge welcome");

    let event = timeout(Duration::from_secs(2), async {
        loop {
            match conn.recv().await {
                DaemonMessage::AgentEvent { event_json } => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&event_json).expect("parse concierge welcome");
                    if parsed.get("type").and_then(|value| value.as_str())
                        == Some("concierge_welcome")
                    {
                        return parsed;
                    }
                }
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "concierge_welcome");
                }
                other => panic!("expected concierge welcome event, got {other:?}"),
            }
        }
    })
    .await
    .expect("hydrated history should produce concierge welcome");

    let content = event
        .get("content")
        .and_then(|value| value.as_str())
        .expect("concierge welcome content");
    assert!(
        content.contains("Persisted Thread"),
        "welcome should summarize persisted work instead of onboarding, got: {content}"
    );
    assert!(
        !content.contains("first session"),
        "welcome should not describe hydrated history as a first session, got: {content}"
    );

    conn.shutdown().await;
}

#[tokio::test]
async fn concierge_welcome_uses_persisted_history_without_preloading_messages() {
    let mut config = AgentConfig::default();
    config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::Minimal;
    config.model = "gpt-5".to_string();

    let mut conn = spawn_test_connection_with_config(config).await;
    let now = 1_735_000_000_000i64;

    conn.agent
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: "persisted-thread".to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Persisted Thread".to_string(),
            created_at: now - 10_000,
            updated_at: now - 5_000,
            message_count: 2,
            total_tokens: 42,
            last_preview: "last persisted answer".to_string(),
            metadata_json: None,
        })
        .await
        .expect("persist thread row");
    conn.agent
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "persisted-user".to_string(),
            thread_id: "persisted-thread".to_string(),
            created_at: now - 10_000,
            role: "user".to_string(),
            content: "resume the work".to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("persist user message");
    conn.agent
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "persisted-assistant".to_string(),
            thread_id: "persisted-thread".to_string(),
            created_at: now - 5_000,
            role: "assistant".to_string(),
            content: "last persisted answer".to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("persist assistant message");

    conn.framed
        .send(ClientMessage::AgentSubscribe)
        .await
        .expect("subscribe to agent events");
    conn.framed
        .send(ClientMessage::AgentRequestConciergeWelcome)
        .await
        .expect("request concierge welcome");

    let event = timeout(Duration::from_secs(2), async {
        loop {
            match conn.recv().await {
                DaemonMessage::AgentEvent { event_json } => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&event_json).expect("parse concierge welcome");
                    if parsed.get("type").and_then(|value| value.as_str())
                        == Some("concierge_welcome")
                    {
                        return parsed;
                    }
                }
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "concierge_welcome");
                }
                other => panic!("expected concierge welcome event, got {other:?}"),
            }
        }
    })
    .await
    .expect("persisted history should produce concierge welcome");

    let content = event
        .get("content")
        .and_then(|value| value.as_str())
        .expect("concierge welcome content");
    assert!(
        content.contains("Persisted Thread"),
        "welcome should summarize persisted work without preloading messages, got: {content}"
    );
    assert!(
        !content.contains("first session"),
        "welcome should not describe persisted work as a first session, got: {content}"
    );

    conn.shutdown().await;
}
