use super::*;
#[tokio::test]
async fn anthropic_batch_results_parse_document_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_document/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-document\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_document\",\"content\":[{\"type\":\"document\",\"title\":\"Spec\",\"source\":{\"type\":\"text\",\"media_type\":\"text/plain\",\"data\":\"hello\"},\"citations\":[{\"type\":\"char_location\",\"start_char_index\":0,\"end_char_index\":5,\"document_index\":0}]}]}}}\n"
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
        "msgbatch_document",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 1);

            let document = &message.content[0];
            assert_eq!(document.block_type(), "document");
            assert_eq!(document.title.as_deref(), Some("Spec"));

            let source = document.source.as_ref().expect("document source");
            assert_eq!(source.source_type, "text");
            assert_eq!(source.media_type.as_deref(), Some("text/plain"));
            assert_eq!(source.data.as_deref(), Some("hello"));

            let citations = document.citations.as_ref().expect("document citations");
            assert_eq!(citations.items.len(), 1);
            let citation = &citations.items[0];
            assert_eq!(citation.citation_type, "char_location");
            assert_eq!(citation.start_char_index, Some(0));
            assert_eq!(citation.end_char_index, Some(5));
            assert_eq!(citation.document_index, Some(0));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_tool_result_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_tool_result/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-tool-result\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_tool_result\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"toolu_1\",\"is_error\":true,\"content\":\"network failed\"}]}}}\n"
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
        "msgbatch_tool_result",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 1);

            let tool_result = &message.content[0];
            assert_eq!(tool_result.block_type(), "tool_result");
            assert_eq!(tool_result.tool_use_id.as_deref(), Some("toolu_1"));
            assert_eq!(tool_result.is_error, Some(true));
            assert_eq!(
                tool_result
                    .content
                    .as_ref()
                    .and_then(|value| value.as_text()),
                Some("network failed")
            );
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_nested_tool_result_content_blocks() {
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
            request.starts_with(
                "GET /v1/messages/batches/msgbatch_nested_tool_result/results HTTP/1.1"
            ),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-nested-tool-result\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_nested_tool_result\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"toolu_1\",\"content\":[{\"type\":\"text\",\"text\":\"nested output\"}]}]}}}\n"
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
        "msgbatch_nested_tool_result",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            let tool_result = &message.content[0];
            let nested_blocks = tool_result
                .content
                .as_ref()
                .and_then(|value| value.as_blocks())
                .expect("nested content blocks");
            assert_eq!(nested_blocks.len(), 1);
            assert_eq!(nested_blocks[0].block_type(), "text");
            assert_eq!(nested_blocks[0].text(), Some("nested output"));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_web_search_result_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_search/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-search\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_search\",\"content\":[{\"type\":\"web_search_result\",\"title\":\"Example\",\"url\":\"https://example.com\",\"page_age\":\"3d\",\"encrypted_content\":\"cipher\"}]}}}\n"
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
        "msgbatch_search",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 1);

            let search_result = &message.content[0];
            assert_eq!(search_result.block_type(), "web_search_result");
            assert_eq!(search_result.title.as_deref(), Some("Example"));
            assert_eq!(search_result.url.as_deref(), Some("https://example.com"));
            assert_eq!(search_result.page_age.as_deref(), Some("3d"));
            assert_eq!(search_result.encrypted_content.as_deref(), Some("cipher"));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_file_backed_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_file/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-file\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_file\",\"content\":[{\"type\":\"container_upload\",\"file_id\":\"file_1\"}]}}}\n"
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
        "msgbatch_file",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 1);
            let file_block = &message.content[0];
            assert_eq!(file_block.block_type(), "container_upload");
            assert_eq!(file_block.file_id.as_deref(), Some("file_1"));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_tool_reference_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_reference/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-reference\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_reference\",\"content\":[{\"type\":\"tool_reference\",\"tool_name\":\"web_search\"}]}}}\n"
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
        "msgbatch_reference",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 1);
            let tool_reference = &message.content[0];
            assert_eq!(tool_reference.block_type(), "tool_reference");
            assert_eq!(tool_reference.tool_name.as_deref(), Some("web_search"));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}
