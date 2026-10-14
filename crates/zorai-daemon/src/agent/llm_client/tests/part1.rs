use super::*;
use crate::agent::types::{AgentMessage, AuthSource, MessageRole, ToolCall, ToolFunction};
use futures::StreamExt;
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use zorai_shared::providers::PROVIDER_ID_KIMI_CODING_PLAN;

const STRUCTURED_ERROR_MARKER: &str = "\n\n[zorai-upstream-diagnostics]";

#[derive(Debug, Deserialize)]
pub(super) struct StructuredErrorEnvelope {
    pub(super) class: String,
    pub(super) summary: String,
    pub(super) diagnostics: serde_json::Value,
}

pub(super) fn parse_structured_error(message: &str) -> StructuredErrorEnvelope {
    let (summary, diagnostics) = message
        .split_once(STRUCTURED_ERROR_MARKER)
        .expect("structured diagnostics marker should be present");
    let envelope: StructuredErrorEnvelope =
        serde_json::from_str(diagnostics).expect("diagnostics should decode as json");
    assert_eq!(summary, envelope.summary);
    envelope
}

#[test]
fn anthropic_invalid_request_error_type_overrides_not_supported_heuristic() {
    let err = classify_http_failure_with_retry_after(
        reqwest::StatusCode::BAD_REQUEST,
        "Anthropic",
        r#"{
                "type": "error",
                "error": {
                    "type": "invalid_request_error",
                    "message": "Prefilling assistant messages is not supported for this model."
                },
                "request_id": "req_011CSHoEeqs5C35K2UUqR7Fy"
            }"#,
        None,
    );

    let failure = upstream_failure_error(&err).expect("structured upstream failure");
    assert_eq!(failure.class, UpstreamFailureClass::RequestInvalid);

    let envelope = failure.structured();
    assert_eq!(envelope.class, "request_invalid");
    assert_eq!(envelope.diagnostics["error_type"], "invalid_request_error");
    assert_eq!(
        envelope.diagnostics["request_id"],
        "req_011CSHoEeqs5C35K2UUqR7Fy"
    );
    assert_eq!(
        envelope.diagnostics["raw_message"],
        "Prefilling assistant messages is not supported for this model."
    );
}

#[test]
fn anthropic_http_529_is_temporary_upstream_even_without_body_details() {
    let err = classify_http_failure_with_retry_after(
        reqwest::StatusCode::from_u16(529).expect("529 status code"),
        "Anthropic",
        "",
        None,
    );

    let failure = upstream_failure_error(&err).expect("structured upstream failure");
    assert_eq!(failure.class, UpstreamFailureClass::TemporaryUpstream);
    let envelope = failure.structured();
    assert_eq!(envelope.class, "temporary_upstream");
    assert_eq!(envelope.diagnostics["status"], 529);
}

#[test]
fn openai_edge_507_retry_buffer_overflow_is_retried_as_temporary_upstream() {
    let err = classify_http_failure_with_retry_after(
        reqwest::StatusCode::INSUFFICIENT_STORAGE,
        "openai",
        "exceeded request buffer limit while retrying upstream",
        None,
    );

    let failure = upstream_failure_error(&err).expect("structured upstream failure");
    assert_eq!(failure.class, UpstreamFailureClass::TemporaryUpstream);
    assert_eq!(
        crate::agent::llm_client::analyze_retry_failure(&err).failure_class,
        "upstream",
        "the edge proxy failed its own upstream retry; the daemon must retry instead of failing the turn"
    );
}

#[test]
fn anthropic_http_413_is_request_invalid_even_without_typed_body() {
    let err = classify_http_failure_with_retry_after(
        reqwest::StatusCode::PAYLOAD_TOO_LARGE,
        "Anthropic",
        "",
        None,
    );

    let failure = upstream_failure_error(&err).expect("structured upstream failure");
    assert_eq!(failure.class, UpstreamFailureClass::RequestInvalid);
    let envelope = failure.structured();
    assert_eq!(envelope.class, "request_invalid");
    assert_eq!(envelope.diagnostics["status"], 413);
}

#[test]
fn anthropic_http_402_is_auth_configuration_even_without_typed_body() {
    let err = classify_http_failure_with_retry_after(
        reqwest::StatusCode::PAYMENT_REQUIRED,
        "Anthropic",
        "",
        None,
    );

    let failure = upstream_failure_error(&err).expect("structured upstream failure");
    assert_eq!(failure.class, UpstreamFailureClass::AuthConfiguration);
    let envelope = failure.structured();
    assert_eq!(envelope.class, "auth_configuration");
    assert_eq!(envelope.diagnostics["status"], 402);
}

#[test]
fn kimi_coding_plan_auth_failure_surfaces_moonshot_onboarding_steps() {
    let err = classify_http_failure_with_retry_after(
        reqwest::StatusCode::FORBIDDEN,
        PROVIDER_ID_KIMI_CODING_PLAN,
        r#"{
            "error": {
                "message": "Kimi For Coding is currently only available for Coding Agents. Contact the open-source community or product owner and have them submit a client onboarding request including the User-Agent to be added to the allowlist."
            }
        }"#,
        None,
    );

    let failure = upstream_failure_error(&err).expect("structured upstream failure");
    assert_eq!(failure.class, UpstreamFailureClass::AuthConfiguration);

    let envelope = failure.structured();
    assert_eq!(envelope.class, "auth_configuration");
    assert!(envelope.summary.contains("not allowlisted by Moonshot yet"));
    assert!(envelope.summary.contains("code@moonshot.ai"));
    assert!(envelope.summary.contains("User-Agent to allowlist"));
    assert!(envelope.diagnostics["raw_message"]
        .as_str()
        .expect("raw message")
        .contains("Kimi For Coding is currently only available for Coding Agents"));
}

pub(super) async fn collect_chunks(mut stream: CompletionStream) -> Vec<CompletionChunk> {
    let mut chunks = Vec::new();
    while let Some(chunk) = stream.next().await {
        chunks.push(chunk.expect("chunk should be ok"));
    }
    chunks
}

pub(super) async fn spawn_responses_error_server(
    responses_status_line: &'static str,
    responses_body: String,
    chat_body: Option<String>,
    request_paths: Arc<Mutex<VecDeque<String>>>,
    chat_requests: Arc<AtomicUsize>,
) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind error server");
    let addr = listener.local_addr().expect("error server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request_paths = request_paths.clone();
            let chat_requests = chat_requests.clone();
            let responses_body = responses_body.clone();
            let chat_body = chat_body.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let read = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                    .await
                    .expect("read error server request");
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();
                request_paths
                    .lock()
                    .expect("lock request paths")
                    .push_back(path.clone());

                let response = if path.ends_with("/chat/completions") {
                    chat_requests.fetch_add(1, Ordering::SeqCst);
                    let body = chat_body.clone().unwrap_or_else(|| {
                            concat!(
                                "data: {\"choices\":[{\"delta\":{\"content\":\"fallback\"}}]}\n\n",
                                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
                                "data: [DONE]\n\n"
                            )
                            .to_string()
                        });
                    format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                } else {
                    format!(
                            "HTTP/1.1 {responses_status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            responses_body.len(),
                            responses_body
                        )
                };

                tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                    .await
                    .expect("write error server response");
            });
        }
    });

    format!("http://{addr}/v1")
}

pub(super) fn responses_test_config(base_url: String, auth_source: AuthSource) -> ProviderConfig {
    ProviderConfig {
        base_url,
        model: "gpt-4.1".to_string(),
        api_key: "test-key".to_string(),
        assistant_id: String::new(),
        auth_source,
        api_transport: ApiTransport::Responses,
        reasoning_effort: String::new(),
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
    }
}

#[test]
fn anthropic_groups_consecutive_tool_results_into_one_user_message() {
    let messages = vec![
        ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text("Checking both".to_string()),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![
                ApiToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ApiToolCallFunction {
                        name: "list_files".to_string(),
                        arguments: "{\"path\":\".\"}".to_string(),
                    },
                },
                ApiToolCall {
                    id: "call_2".to_string(),
                    call_type: "function".to_string(),
                    function: ApiToolCallFunction {
                        name: "read_file".to_string(),
                        arguments: "{\"path\":\"README.md\"}".to_string(),
                    },
                },
            ]),
        },
        ApiMessage {
            role: "tool".to_string(),
            content: ApiContent::Text("file list".to_string()),
            reasoning: None,
            tool_call_id: Some("call_1".to_string()),
            name: Some("list_files".to_string()),
            tool_calls: None,
        },
        ApiMessage {
            role: "tool".to_string(),
            content: ApiContent::Text("readme contents".to_string()),
            reasoning: None,
            tool_call_id: Some("call_2".to_string()),
            name: Some("read_file".to_string()),
            tool_calls: None,
        },
    ];

    let anthropic = build_anthropic_messages(&messages);
    assert_eq!(anthropic.len(), 2);
    assert_eq!(anthropic[0]["role"], "assistant");
    assert_eq!(anthropic[1]["role"], "user");
    let blocks = anthropic[1]["content"]
        .as_array()
        .expect("tool results should be grouped into one block array");
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["type"], "tool_result");
    assert_eq!(blocks[0]["tool_use_id"], "call_1");
    assert_eq!(blocks[1]["tool_use_id"], "call_2");
}

#[tokio::test]
async fn anthropic_count_tokens_uses_count_tokens_endpoint_and_parses_input_tokens() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind count_tokens server");
    let addr = listener.local_addr().expect("count_tokens server addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buffer = vec![0u8; 65536];
        let read = socket.read(&mut buffer).await.expect("read request");
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        assert!(
            request.starts_with("POST /v1/messages/count_tokens HTTP/1.1"),
            "unexpected request line: {request}"
        );

        let body = r#"{"input_tokens":123}"#;
        let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nrequest-id: req_count_tokens_123\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    });

    let tokens = count_request_tokens(
        &reqwest::Client::new(),
        "anthropic",
        &ProviderConfig {
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
    )
    .await
    .expect("count tokens should succeed");

    assert_eq!(tokens.input_tokens, 123);
    assert_eq!(tokens.request_id.as_deref(), Some("req_count_tokens_123"));
}
#[test]
fn anthropic_drops_incomplete_tool_result_batches() {
    let messages = vec![
        ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text("Checking both".to_string()),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![
                ApiToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ApiToolCallFunction {
                        name: "list_files".to_string(),
                        arguments: "{\"path\":\".\"}".to_string(),
                    },
                },
                ApiToolCall {
                    id: "call_2".to_string(),
                    call_type: "function".to_string(),
                    function: ApiToolCallFunction {
                        name: "read_file".to_string(),
                        arguments: "{\"path\":\"README.md\"}".to_string(),
                    },
                },
            ]),
        },
        ApiMessage {
            role: "tool".to_string(),
            content: ApiContent::Text("file list".to_string()),
            reasoning: None,
            tool_call_id: Some("call_1".to_string()),
            name: Some("list_files".to_string()),
            tool_calls: None,
        },
        ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text("Continue".to_string()),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        },
    ];

    let anthropic = build_anthropic_messages(&messages);
    assert_eq!(anthropic.len(), 1);
    assert_eq!(anthropic[0]["role"], "user");
    assert_eq!(anthropic[0]["content"], "Continue");
}

#[test]
fn messages_to_api_format_keeps_reused_tool_ids_across_turns() {
    let messages = vec![
        AgentMessage {
            id: String::new(),
            role: MessageRole::Assistant,
            content: "first".to_string(),
            content_blocks: Vec::new(),
            tool_calls: Some(vec![ToolCall {
                id: "2013".to_string(),
                function: ToolFunction {
                    name: "tool_a".to_string(),
                    arguments: "{}".to_string(),
                },
                weles_review: None,
            }]),
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
            message_kind: crate::agent::types::AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 1,
            feedback: None,
        },
        AgentMessage {
            id: String::new(),
            role: MessageRole::Tool,
            content: "ok 1".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: Some("2013".to_string()),
            tool_name: Some("tool_a".to_string()),
            tool_arguments: Some("{}".to_string()),
            tool_status: Some("done".to_string()),
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
            message_kind: crate::agent::types::AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
            feedback: None,
        },
        AgentMessage::user("next", 3),
        AgentMessage {
            id: String::new(),
            role: MessageRole::Assistant,
            content: "second".to_string(),
            content_blocks: Vec::new(),
            tool_calls: Some(vec![ToolCall {
                id: "2013".to_string(),
                function: ToolFunction {
                    name: "tool_b".to_string(),
                    arguments: "{}".to_string(),
                },
                weles_review: None,
            }]),
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
            message_kind: crate::agent::types::AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 4,
            feedback: None,
        },
        AgentMessage {
            id: String::new(),
            role: MessageRole::Tool,
            content: "ok 2".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: Some("2013".to_string()),
            tool_name: Some("tool_b".to_string()),
            tool_arguments: Some("{}".to_string()),
            tool_status: Some("done".to_string()),
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
            message_kind: crate::agent::types::AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 5,
            feedback: None,
        },
    ];

    let api_messages = messages_to_api_format(&messages);
    let tool_results = api_messages
        .iter()
        .filter(|message| message.role == "tool")
        .count();
    assert_eq!(tool_results, 2);
}
