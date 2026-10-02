use super::part1::*;
use super::*;
use zorai_shared::providers::*;
#[test]
fn retry_delay_scales_with_attempt_multiplier() {
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 1), 5_000);
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 2), 10_000);
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 3), 15_000);
}

#[test]
fn retry_delay_caps_at_one_minute() {
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 20), 60_000);
}

#[test]
fn minimax_anthropic_requests_keep_connection_close_without_extra_transport_headers() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://api.minimax.io/anthropic".to_string(),
        model: "MiniMax-M1".to_string(),
        api_key: "secret".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "medium".to_string(),
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
    let request = build_anthropic_request(
        &client,
        "minimax-coding-plan",
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
        false,
    )
    .expect("request should build");

    assert_eq!(
        request
            .headers()
            .get(reqwest::header::CONNECTION)
            .and_then(|value| value.to_str().ok()),
        Some("close")
    );
    assert!(request.headers().get("upgrade").is_none());
    assert_eq!(
        request
            .headers()
            .get("anthropic-beta")
            .and_then(|value| value.to_str().ok()),
        Some("fine-grained-tool-streaming-2025-05-14,interleaved-thinking-2025-05-14")
    );
    let body: serde_json::Value = serde_json::from_slice(
        request
            .body()
            .and_then(|body| body.as_bytes())
            .expect("body bytes"),
    )
    .expect("json body");
    assert_eq!(body["cache_control"]["type"], "ephemeral");
    assert!(body["cache_control"]["ttl"].is_null());
    assert_eq!(
        body["thinking"]["type"], "enabled",
        "minimax requests must enable thinking so reasoning is returned"
    );
    assert_eq!(body["thinking"]["budget_tokens"], 4096);
}

#[test]
fn anthropic_request_sets_tool_choice_auto_when_tools_are_present() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://api.anthropic.com".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        api_key: "secret".to_string(),
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
    let request = build_anthropic_request(
        &client,
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
        &[ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "ping".to_string(),
                description: "check".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
        }],
        false,
    )
    .expect("request should build");

    let body: serde_json::Value = serde_json::from_slice(
        request
            .body()
            .and_then(|body| body.as_bytes())
            .expect("body bytes"),
    )
    .expect("json body");
    assert_eq!(body["tool_choice"]["type"], "auto");
}

#[test]
fn github_copilot_provider_does_not_support_native_assistant_transport() {
    let provider = get_provider_definition(PROVIDER_ID_GITHUB_COPILOT)
        .expect("github copilot provider should exist");

    assert!(provider.native_transport_kind.is_none());
    assert!(!provider
        .supported_transports
        .contains(&ApiTransport::NativeAssistant));
}

#[test]
fn anthropic_request_fingerprint_is_stable_for_identical_requests() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://api.minimax.io/anthropic".to_string(),
        model: "MiniMax-M2.7".to_string(),
        api_key: "secret".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "medium".to_string(),
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
    let messages = vec![ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text("hello".to_string()),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    }];
    let tools = vec![ToolDefinition {
        tool_type: "function".to_string(),
        function: ToolFunctionDef {
            name: "ping".to_string(),
            description: "check".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    }];

    let request_a = build_anthropic_request(
        &client,
        "minimax-coding-plan",
        &config,
        "system",
        &messages,
        &tools,
        false,
    )
    .expect("request_a should build");
    let request_b = build_anthropic_request(
        &client,
        "minimax-coding-plan",
        &config,
        "system",
        &messages,
        &tools,
        false,
    )
    .expect("request_b should build");

    assert_eq!(
        anthropic_request_fingerprint(&request_a),
        anthropic_request_fingerprint(&request_b)
    );
}

#[test]
fn anthropic_request_fingerprint_changes_when_payload_changes() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://api.minimax.io/anthropic".to_string(),
        model: "MiniMax-M2.7".to_string(),
        api_key: "secret".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "medium".to_string(),
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

    let request_a = build_anthropic_request(
        &client,
        "minimax-coding-plan",
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
        false,
    )
    .expect("request_a should build");
    let request_b = build_anthropic_request(
        &client,
        "minimax-coding-plan",
        &config,
        "system",
        &[ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text("hello again".to_string()),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }],
        &[],
        false,
    )
    .expect("request_b should build");

    assert_ne!(
        anthropic_request_fingerprint(&request_a),
        anthropic_request_fingerprint(&request_b)
    );
}

#[test]
fn minimax_attempt_target_uses_anthropic_messages_endpoint() {
    let config = ProviderConfig {
        base_url: "https://api.minimax.io/anthropic".to_string(),
        model: "MiniMax-M1".to_string(),
        api_key: "secret".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "medium".to_string(),
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

    let target = effective_attempt_target(
        "minimax-coding-plan",
        &config,
        ApiTransport::ChatCompletions,
    );

    assert_eq!(target.api_type, ApiType::Anthropic);
    assert_eq!(target.branch, "anthropic");
    assert!(
        target.url.ends_with("/anthropic/v1/messages"),
        "expected anthropic messages endpoint, got {}",
        target.url
    );
}

#[test]
fn retry_failure_analysis_marks_invalid_http_version_as_transport_retry() {
    let error = anyhow::anyhow!(
        "error sending request for url (https://api.minimax.io/anthropic/v1/messages): client error (SendRequest): invalid HTTP version parsed"
    );

    let analysis = analyze_retry_failure(&error);

    assert_eq!(analysis.failure_class, "transport");
    assert!(analysis.is_transient_transport);
    assert!(!analysis.is_rate_limited);
    assert!(!analysis.is_temporary_upstream);
    assert!(analysis.retry_after_ms.is_none());
}

#[test]
fn exhausted_usage_limit_is_not_retried_as_a_rate_limit() {
    let error: anyhow::Error = UpstreamFailureError::new(
        UpstreamFailureClass::RateLimit,
        "openai API returned 429: The usage limit has been reached",
        serde_json::json!({
            "raw_message": "The usage limit has been reached",
        }),
    )
    .into();

    let analysis = analyze_retry_failure(&error);

    assert!(!analysis.is_rate_limited);
    assert_eq!(analysis.failure_class, "non_retryable");
}

#[tokio::test]
async fn minimax_anthropic_retry_recovers_after_malformed_http_response() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind malformed-http server");
    let addr = listener.local_addr().expect("malformed-http server addr");

    tokio::spawn(async move {
        let mut request_count = 0usize;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            request_count += 1;
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let _ = socket
                    .read(&mut buffer)
                    .await
                    .expect("read malformed-http request");
                if request_count == 1 {
                    socket
                        .write_all(b"HTTP/9.9 200 OK\r\ncontent-length: 0\r\n\r\n")
                        .await
                        .expect("write malformed-http response");
                    return;
                }
                let body = concat!(
                    "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1}}}\n\n",
                    "data: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"text\"}}\n\n",
                    "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"recovered\"}}\n\n",
                    "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":1}}\n\n",
                    "data: {\"type\":\"message_stop\"}\n\n"
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write valid anthropic response");
            });
        }
    });

    let stream = send_completion_request(
        &reqwest::Client::new(),
        "minimax-coding-plan",
        &ProviderConfig {
            base_url: format!("http://{addr}/anthropic"),
            model: "MiniMax-M1".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "medium".to_string(),
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
        ApiTransport::ChatCompletions,
        None,
        None,
        RetryStrategy::Bounded {
            max_retries: 1,
            retry_delay_ms: 1,
        },
    );

    let chunks = collect_chunks(stream).await;
    assert!(
        chunks
            .iter()
            .any(|chunk| matches!(chunk, CompletionChunk::Retry { failure_class, .. } if failure_class == "transport")),
        "expected a transport retry before recovery"
    );
    assert!(
        chunks.iter().any(
            |chunk| matches!(chunk, CompletionChunk::Done { content, .. } if content == "recovered")
        ),
        "expected retry path to recover and finish the anthropic stream"
    );
}

#[tokio::test]
async fn minimax_send_path_never_falls_back_to_chat_completions() {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let request_paths = Arc::new(Mutex::new(VecDeque::new()));
    let recorded_paths = request_paths.clone();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind minimax path server");
    let addr = listener.local_addr().expect("minimax path server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_paths = request_paths.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let read = socket.read(&mut buffer).await.expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or_default()
                    .to_string();
                request_paths
                    .lock()
                    .expect("lock request paths")
                    .push_back(path.clone());

                let response = if path.ends_with("/chat/completions") {
                    concat!(
                        "HTTP/1.1 500 Internal Server Error\r\n",
                        "content-type: application/json\r\n",
                        "content-length: 2\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "{}"
                    )
                    .to_string()
                } else {
                    let body = concat!(
                        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1}}}\n\n",
                        "data: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"text\"}}\n\n",
                        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"anthropic-only\"}}\n\n",
                        "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":1}}\n\n",
                        "data: {\"type\":\"message_stop\"}\n\n"
                    );
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                };
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            });
        }
    });

    let stream = send_completion_request(
        &reqwest::Client::new(),
        "minimax-coding-plan",
        &ProviderConfig {
            base_url: format!("http://{addr}/anthropic"),
            model: "MiniMax-M1".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "medium".to_string(),
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
        ApiTransport::ChatCompletions,
        None,
        None,
        RetryStrategy::Bounded {
            max_retries: 0,
            retry_delay_ms: 0,
        },
    );

    let chunks = collect_chunks(stream).await;
    assert!(
        chunks
            .iter()
            .any(|chunk| matches!(chunk, CompletionChunk::Done { content, .. } if content == "anthropic-only")),
        "expected minimax request to complete through anthropic messages endpoint"
    );

    let request_paths = recorded_paths.lock().expect("lock request paths");
    assert!(
        request_paths
            .iter()
            .any(|path| path.ends_with("/anthropic/v1/messages")),
        "expected anthropic endpoint request, saw {:?}",
        *request_paths
    );
    assert!(
        !request_paths
            .iter()
            .any(|path| path.ends_with("/chat/completions")),
        "minimax send path must not fall back to chat completions: {:?}",
        *request_paths
    );
}

#[tokio::test]
async fn anthropic_stream_error_event_emits_error_chunk() {
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::new();
    let body = concat!(
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_stream_err\",\"usage\":{\"input_tokens\":1}}}\n\n",
        "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Over capacity\"}}\n\n"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let body = body.to_string();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    });

    let response = client
        .get(format!("http://{addr}"))
        .send()
        .await
        .expect("send test request");

    let (tx, mut rx) = mpsc::channel(8);
    parse_anthropic_sse(response, None, &tx)
        .await
        .expect("parse should succeed");
    drop(tx);

    let mut saw_error = None;
    while let Some(chunk) = rx.recv().await {
        match chunk.expect("chunk") {
            CompletionChunk::Error { message } => {
                saw_error = Some(message);
                break;
            }
            CompletionChunk::Done { .. } => {
                panic!("expected streamed anthropic error to stop with an error chunk")
            }
            _ => {}
        }
    }

    let message = saw_error.expect("expected anthropic stream error chunk");
    assert!(
        message.contains("Anthropic stream error"),
        "unexpected message: {message}"
    );
    assert!(
        message.contains("Over capacity"),
        "unexpected message: {message}"
    );
    server.await.expect("server task");
}

#[tokio::test]
async fn anthropic_message_start_id_is_forwarded_as_response_id() {
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::new();
    let body = concat!(
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_response_id\",\"usage\":{\"input_tokens\":3}}}\n\n",
        "data: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"text\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n",
        "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":5}}\n\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let body = body.to_string();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    });

    let response = client
        .get(format!("http://{addr}"))
        .send()
        .await
        .expect("send test request");

    let (tx, mut rx) = mpsc::channel(8);
    parse_anthropic_sse(response, None, &tx)
        .await
        .expect("parse should succeed");
    drop(tx);

    let mut done_chunk = None;
    while let Some(chunk) = rx.recv().await {
        if let CompletionChunk::Done { response_id, .. } = chunk.expect("chunk") {
            done_chunk = Some(response_id);
            break;
        }
    }

    assert_eq!(done_chunk, Some(Some("msg_response_id".to_string())));
    server.await.expect("server task");
}
