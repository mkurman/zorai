use super::*;
#[tokio::test]
async fn anthropic_batch_results_parse_tool_references_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_tool_refs/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-tool-refs\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_tool_refs\",\"content\":[{\"type\":\"tool_search_result\",\"tool_references\":[{\"type\":\"tool_reference\",\"tool_name\":\"web_search\"}]}]}}}\n"
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
        "msgbatch_tool_refs",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 1);
            let result_block = &message.content[0];
            assert_eq!(result_block.block_type(), "tool_search_result");
            let tool_references = result_block
                .tool_references
                .as_ref()
                .expect("typed tool references");
            assert_eq!(tool_references.items.len(), 1);
            let tool_reference = &tool_references.items[0];
            assert_eq!(tool_reference.reference_type, "tool_reference");
            assert_eq!(tool_reference.tool_name.as_deref(), Some("web_search"));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_code_execution_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_exec/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-exec\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_exec\",\"content\":[{\"type\":\"code_execution_result\",\"content\":\"done\",\"return_code\":0,\"stderr\":\"\",\"is_file_update\":false}]}}}\n"
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
        "msgbatch_exec",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 1);
            let exec = &message.content[0];
            assert_eq!(exec.block_type(), "code_execution_result");
            assert_eq!(
                exec.content.as_ref().and_then(|value| value.as_text()),
                Some("done")
            );
            assert_eq!(exec.return_code, Some(0));
            assert_eq!(exec.stderr.as_deref(), Some(""));
            assert_eq!(exec.is_file_update, Some(false));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_batch_results_parse_error_content_blocks() {
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
            request.starts_with("GET /v1/messages/batches/msgbatch_error_block/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-error-block\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_error_block\",\"content\":[{\"type\":\"tool_search_tool_result_error\",\"error_code\":\"invalid_tool_input\",\"error_message\":\"bad query\"}]}}}\n"
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
        "msgbatch_error_block",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            assert_eq!(message.content.len(), 1);
            let error_block = &message.content[0];
            assert_eq!(error_block.block_type(), "tool_search_tool_result_error");
            assert_eq!(
                error_block.error_code.as_deref(),
                Some("invalid_tool_input")
            );
            assert_eq!(error_block.error_message.as_deref(), Some("bad query"));
        }
        other => panic!("expected succeeded batch result, got {other:?}"),
    }
}
