use super::*;
#[tokio::test]
async fn anthropic_batch_results_parse_typed_document_source_and_citations() {
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
            request
                .starts_with("GET /v1/messages/batches/msgbatch_document_typed/results HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = concat!(
            "{\"custom_id\":\"req-document-typed\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_document_typed\",\"content\":[{\"type\":\"document\",\"title\":\"Spec\",\"source\":{\"type\":\"text\",\"media_type\":\"text/plain\",\"data\":\"hello\"},\"citations\":[{\"type\":\"char_location\",\"start_char_index\":0,\"end_char_index\":5,\"document_index\":0}]}]}}}\n"
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
        "msgbatch_document_typed",
    )
    .await
    .expect("batch results should parse");

    match &results[0].result {
        AnthropicMessageBatchResult::Succeeded { message } => {
            let document = &message.content[0];
            let source = document.source.as_ref().expect("typed source");
            assert_eq!(source.source_type, "text");
            assert_eq!(source.media_type.as_deref(), Some("text/plain"));
            assert_eq!(source.data.as_deref(), Some("hello"));

            let citations = document.citations.as_ref().expect("typed citations");
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
