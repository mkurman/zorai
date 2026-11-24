use super::*;
#[tokio::test]
async fn anthropic_batch_results_parse_extended_usage_fields() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_usage/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-usage\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_usage\",\"content\":[{\"type\":\"text\",\"text\":\"hello world\"}],\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"cache_creation\":{\"ephemeral_1h_input_tokens\":13,\"ephemeral_5m_input_tokens\":17},\"cache_creation_input_tokens\":3,\"cache_read_input_tokens\":5,\"server_tool_use\":{\"web_search_requests\":2}}}}}\n"
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
        "msgbatch_usage",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            let usage = message.usage.as_ref().expect("usage should be present");
            assert_eq!(usage.input_tokens, 11);
            assert_eq!(usage.output_tokens, 7);
            assert_eq!(
                usage
                    .cache_creation
                    .as_ref()
                    .and_then(|value| value.ephemeral_1h_input_tokens),
                Some(13)
            );
            assert_eq!(
                usage
                    .cache_creation
                    .as_ref()
                    .and_then(|value| value.ephemeral_5m_input_tokens),
                Some(17)
            );
            assert_eq!(usage.cache_creation_input_tokens, Some(3));
            assert_eq!(usage.cache_read_input_tokens, Some(5));
            assert_eq!(
                usage
                    .server_tool_use
                    .as_ref()
                    .and_then(|value| value.web_search_requests),
                Some(2)
            );
            assert!(!usage.extra.contains_key("server_tool_use"));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_tool_use_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_blocks/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-blocks\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_blocks\",\"content\":[{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"web_search\",\"input\":{\"query\":\"cats\"}},{\"type\":\"server_tool_use\",\"id\":\"toolu_server\",\"name\":\"web_fetch\",\"caller\":{\"type\":\"server_tool_caller\",\"tool_id\":\"srv_1\"},\"input\":{\"url\":\"https://example.com\"}}]}}}\n"
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
        "msgbatch_blocks",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 2);

            let tool_use = &message.content[0];
            assert_eq!(tool_use.block_type(), "tool_use");
            assert_eq!(tool_use.id.as_deref(), Some("toolu_1"));
            assert_eq!(tool_use.name.as_deref(), Some("web_search"));
            assert_eq!(tool_use.input, Some(serde_json::json!({ "query": "cats" })));

            let server_tool_use = &message.content[1];
            assert_eq!(server_tool_use.block_type(), "server_tool_use");
            assert_eq!(server_tool_use.id.as_deref(), Some("toolu_server"));
            assert_eq!(server_tool_use.name.as_deref(), Some("web_fetch"));
            assert_eq!(
                server_tool_use
                    .caller
                    .as_ref()
                    .map(|value| (value.caller_type.clone(), value.tool_id.clone())),
                Some(("server_tool_caller".to_string(), Some("srv_1".to_string()),))
            );
            assert_eq!(
                server_tool_use.input,
                Some(serde_json::json!({ "url": "https://example.com" }))
            );
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_thinking_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_thinking/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-thinking\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_thinking\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"step by step\",\"signature\":\"sig_1\"},{\"type\":\"redacted_thinking\",\"data\":\"opaque\"}]}}}\n"
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
        "msgbatch_thinking",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 2);

            let thinking = &message.content[0];
            assert_eq!(thinking.block_type(), "thinking");
            assert_eq!(thinking.thinking.as_deref(), Some("step by step"));
            assert_eq!(thinking.signature.as_deref(), Some("sig_1"));

            let redacted = &message.content[1];
            assert_eq!(redacted.block_type(), "redacted_thinking");
            assert_eq!(redacted.data.as_deref(), Some("opaque"));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_web_fetch_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_fetch/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-fetch\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_fetch\",\"content\":[{\"type\":\"web_fetch\",\"url\":\"https://example.com\",\"retrieved_at\":\"2026-04-05T13:00:00Z\",\"content\":\"hello from cache\"}]}}}\n"
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
        "msgbatch_fetch",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 1);

            let web_fetch = &message.content[0];
            assert_eq!(web_fetch.block_type(), "web_fetch");
            assert_eq!(web_fetch.url.as_deref(), Some("https://example.com"));
            assert_eq!(
                web_fetch.retrieved_at.as_deref(),
                Some("2026-04-05T13:00:00Z")
            );
            assert_eq!(
                web_fetch.content.as_ref().and_then(|value| value.as_text()),
                Some("hello from cache")
            );
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}
