use super::*;
#[tokio::test]
async fn anthropic_create_message_batch_posts_requests_and_parses_batch() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind batch create server");
    let addr = listener.local_addr().expect("batch create addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buffer = vec![0u8; 65536];
        let read = socket.read(&mut buffer).await.expect("read request");
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        assert!(
            request.starts_with("POST /v1/messages/batches HTTP/1.1"),
            "unexpected request line: {request}"
        );
        assert!(
            request.contains("\"custom_id\":\"req-1\""),
            "missing custom_id: {request}"
        );
        assert!(
            request.contains("\"max_tokens\":256"),
            "missing max_tokens: {request}"
        );

        let body = serde_json::json!({
            "id": "msgbatch_123",
            "created_at": "2024-08-20T18:37:24.100435Z",
            "processing_status": "in_progress",
            "request_counts": {
                "canceled": 0,
                "errored": 0,
                "expired": 0,
                "processing": 1,
                "succeeded": 0
            },
            "results_url": "https://api.anthropic.com/v1/messages/batches/msgbatch_123/results",
            "type": "message_batch"
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nrequest-id: req_batch_create_123\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    });

    let config = ProviderConfig {
        presence_penalty: None,
        base_url: format!("http://{addr}"),
        model: "claude-sonnet-4-6".to_string(),
        api_key: "test-key".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "off".to_string(),
        context_window_tokens: 0,
        response_schema: None,
        stop_sequences: None,
        temperature: None,
        top_p: None,
        top_k: None,
        metadata: None,
        service_tier: None,
        container: None,
        inference_geo: None,
        cache_control: None,
        max_tokens: None,
        anthropic_tool_choice: None,
        output_effort: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        openrouter_response_cache_enabled: false,
        huggingface_provider: None,
    };
    let params = build_anthropic_message_batch_params(
        "anthropic",
        &config,
        "system",
        &[ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text("hello".to_string()),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }],
        &[],
        256,
    );

    let batch = create_message_batch(
        &reqwest::Client::new(),
        "anthropic",
        &config,
        &[AnthropicBatchCreateRequest {
            custom_id: "req-1".to_string(),
            params,
        }],
    )
    .await
    .expect("create batch should succeed");

    assert_eq!(batch.id, "msgbatch_123");
    assert_eq!(batch.request_id.as_deref(), Some("req_batch_create_123"));
    assert_eq!(batch.processing_status, "in_progress");
    assert_eq!(batch.request_counts.processing, 1);
}

#[tokio::test]
async fn anthropic_batch_list_and_lifecycle_endpoints_use_documented_paths() {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let recorded = Arc::new(Mutex::new(VecDeque::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind batch lifecycle server");
    let addr = listener.local_addr().expect("batch lifecycle addr");
    let recorded_paths = recorded.clone();

    tokio::spawn(async move {
        for _ in 0..4 {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = vec![0u8; 65536];
            let read = socket.read(&mut buffer).await.expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let request_line = request.lines().next().unwrap_or_default().to_string();
            recorded_paths
                .lock()
                .expect("lock recorded")
                .push_back(request_line.clone());

            let (request_id, body) = if request_line.starts_with("DELETE ") {
                (
                    "req_batch_delete_123",
                    serde_json::json!({
                        "id": "msgbatch_123",
                        "type": "message_batch_deleted"
                    }),
                )
            } else if request_line.starts_with("GET /v1/messages/batches?") {
                (
                    "req_batch_list_123",
                    serde_json::json!({
                        "data": [{
                            "id": "msgbatch_123",
                            "created_at": "2024-08-20T18:37:24.100435Z",
                            "processing_status": "in_progress",
                            "request_counts": {
                                "canceled": 0,
                                "errored": 0,
                                "expired": 0,
                                "processing": 1,
                                "succeeded": 0
                            },
                            "type": "message_batch"
                        }],
                        "first_id": "msgbatch_123",
                        "has_more": false,
                        "last_id": "msgbatch_123"
                    }),
                )
            } else if request_line.starts_with("GET /v1/messages/batches/msgbatch_123 ") {
                (
                    "req_batch_retrieve_123",
                    serde_json::json!({
                        "id": "msgbatch_123",
                        "created_at": "2024-08-20T18:37:24.100435Z",
                        "processing_status": "in_progress",
                        "request_counts": {
                            "canceled": 0,
                            "errored": 0,
                            "expired": 0,
                            "processing": 1,
                            "succeeded": 0
                        },
                        "type": "message_batch"
                    }),
                )
            } else if request_line.starts_with("POST /v1/messages/batches/msgbatch_123/cancel ") {
                (
                    "req_batch_cancel_123",
                    serde_json::json!({
                        "id": "msgbatch_123",
                        "created_at": "2024-08-20T18:37:24.100435Z",
                        "processing_status": "in_progress",
                        "request_counts": {
                            "canceled": 0,
                            "errored": 0,
                            "expired": 0,
                            "processing": 1,
                            "succeeded": 0
                        },
                        "type": "message_batch"
                    }),
                )
            } else {
                panic!("unexpected lifecycle request line: {request_line}");
            };
            let body = body.to_string();

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nrequest-id: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                request_id,
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
        base_url: format!("http://{addr}"),
        model: "claude-sonnet-4-6".to_string(),
        api_key: "test-key".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "off".to_string(),
        context_window_tokens: 0,
        response_schema: None,
        stop_sequences: None,
        temperature: None,
        top_p: None,
        top_k: None,
        metadata: None,
        service_tier: None,
        container: None,
        inference_geo: None,
        cache_control: None,
        max_tokens: None,
        anthropic_tool_choice: None,
        output_effort: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        openrouter_response_cache_enabled: false,
        huggingface_provider: None,
    };

    let list = list_message_batches(
        &client,
        "anthropic",
        &config,
        &AnthropicMessageBatchListParams {
            after_id: Some("msgbatch_prev".to_string()),
            before_id: None,
            limit: Some(5),
        },
    )
    .await
    .expect("list batches should succeed");
    assert_eq!(list.data.len(), 1);
    assert_eq!(list.request_id.as_deref(), Some("req_batch_list_123"));

    let retrieved = retrieve_message_batch(&client, "anthropic", &config, "msgbatch_123")
        .await
        .expect("retrieve batch should succeed");
    assert_eq!(retrieved.id, "msgbatch_123");
    assert_eq!(
        retrieved.request_id.as_deref(),
        Some("req_batch_retrieve_123")
    );

    let canceled = cancel_message_batch(&client, "anthropic", &config, "msgbatch_123")
        .await
        .expect("cancel batch should succeed");
    assert_eq!(canceled.id, "msgbatch_123");
    assert_eq!(canceled.request_id.as_deref(), Some("req_batch_cancel_123"));

    let deleted = delete_message_batch(&client, "anthropic", &config, "msgbatch_123")
        .await
        .expect("delete batch should succeed");
    assert_eq!(deleted.batch_type, "message_batch_deleted");
    assert_eq!(deleted.request_id.as_deref(), Some("req_batch_delete_123"));

    let recorded = recorded.lock().expect("lock recorded lines");
    assert!(
        recorded[0].starts_with("GET /v1/messages/batches?"),
        "unexpected list request: {}",
        recorded[0]
    );
    assert!(
        recorded[0].contains("after_id=msgbatch_prev"),
        "missing after_id: {}",
        recorded[0]
    );
    assert!(
        recorded[0].contains("limit=5"),
        "missing limit: {}",
        recorded[0]
    );
    assert_eq!(
        recorded[1],
        "GET /v1/messages/batches/msgbatch_123 HTTP/1.1"
    );
    assert_eq!(
        recorded[2],
        "POST /v1/messages/batches/msgbatch_123/cancel HTTP/1.1"
    );
    assert_eq!(
        recorded[3],
        "DELETE /v1/messages/batches/msgbatch_123 HTTP/1.1"
    );
}

#[tokio::test]
async fn anthropic_batch_results_parse_jsonl_lines() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind batch results server");
    let addr = listener.local_addr().expect("batch results addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buffer = vec![0u8; 65536];
        let read = socket.read(&mut buffer).await.expect("read request");
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        assert!(
            request.starts_with("GET /v1/messages/batches/msgbatch_123/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-1\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_1\",\"content\":[{\"type\":\"text\",\"text\":\"hello world\"}],\"container\":{\"id\":\"container_1\",\"expires_at\":\"2025-01-01T00:00:00Z\"},\"usage\":{\"input_tokens\":11,\"output_tokens\":7}}}}\n",
            "{\"custom_id\":\"req-2\",\"result\":{\"type\":\"errored\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"Bad input\"}}}\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/x-jsonl\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    });

    let results = retrieve_message_batch_results(
        &reqwest::Client::new(),
        "anthropic",
        &ProviderConfig {
            presence_penalty: None,
            base_url: format!("http://{addr}"),
            model: "claude-sonnet-4-6".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "off".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        },
        "msgbatch_123",
    )
    .await
    .expect("batch results should parse");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].custom_id, "req-1");
    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.id, "msg_1");
            assert_eq!(message.content.len(), 1);
            assert_eq!(message.content[0].block_type(), "text");
            assert_eq!(message.content[0].text(), Some("hello world"));
            assert_eq!(
                message.container.as_ref().map(|value| value.id.as_str()),
                Some("container_1")
            );
            assert_eq!(
                message.usage.as_ref().map(|value| value.input_tokens),
                Some(11)
            );
            assert_eq!(
                message.usage.as_ref().map(|value| value.output_tokens),
                Some(7)
            );
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
    match &results[1].result {
        AnthropicMessageBatchResult::Errored { error } => {
            assert_eq!(error.error_type, "invalid_request_error");
            assert_eq!(error.message, "Bad input");
        }
        other => panic!("expected errored batch result, got {other:?}"),
    }
}
