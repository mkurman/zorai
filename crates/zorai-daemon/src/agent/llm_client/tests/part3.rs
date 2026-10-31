use super::part1::*;
use super::*;
use crate::agent::provider_auth_store;
use crate::test_support::EnvGuard;
use std::collections::VecDeque;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::tempdir;
#[tokio::test]
async fn request_invalid_responses_422_invalid_payload_is_classified_request_invalid() {
    let request_paths = Arc::new(Mutex::new(VecDeque::new()));
    let chat_requests = Arc::new(AtomicUsize::new(0));
    let base_url = spawn_responses_error_server(
        "422 Unprocessable Entity",
        r#"{"error":{"message":"Invalid request body: input[3].content is required"}}"#.to_string(),
        None,
        request_paths,
        chat_requests,
    )
    .await;

    let stream = send_completion_request(
        &reqwest::Client::new(),
        zorai_shared::providers::PROVIDER_ID_OPENAI,
        &responses_test_config(base_url, AuthSource::ApiKey),
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
        ApiTransport::Responses,
        None,
        None,
        RetryStrategy::Bounded {
            max_retries: 0,
            retry_delay_ms: 0,
        },
    );

    let chunks = collect_chunks(stream).await;
    let message = match chunks.last().expect("terminal chunk") {
        CompletionChunk::Error { message } => message,
        other => panic!("expected error chunk, got {other:?}"),
    };
    let diagnostics = parse_structured_error(message);
    assert_eq!(diagnostics.class, "request_invalid");
    assert!(diagnostics.summary.contains("invalid"));
    assert!(diagnostics
        .diagnostics
        .to_string()
        .contains("input[3].content"));
}

#[tokio::test]
async fn transport_incompatibility_responses_does_not_fallback_to_chat_completions() {
    let request_paths = Arc::new(Mutex::new(VecDeque::new()));
    let chat_requests = Arc::new(AtomicUsize::new(0));
    let base_url = spawn_responses_error_server(
            "405 Method Not Allowed",
            r#"{"error":{"message":"Responses API not supported here"}}"#.to_string(),
            Some(
                concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"legacy fallback\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
                    "data: [DONE]\n\n"
                )
                .to_string(),
            ),
            request_paths.clone(),
            chat_requests.clone(),
        )
        .await;

    let stream = send_completion_request(
        &reqwest::Client::new(),
        "openai",
        &responses_test_config(base_url, AuthSource::ApiKey),
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
        ApiTransport::Responses,
        None,
        None,
        RetryStrategy::Bounded {
            max_retries: 0,
            retry_delay_ms: 0,
        },
    );

    let chunks = collect_chunks(stream).await;
    assert!(matches!(chunks.last(), Some(CompletionChunk::Error { .. })));
    assert_eq!(chat_requests.load(Ordering::SeqCst), 0);
    let paths = request_paths.lock().expect("lock request paths");
    assert!(paths.iter().any(|path| path.ends_with("/responses")));
    assert!(!paths.iter().any(|path| path.ends_with("/chat/completions")));
}

#[test]
fn github_copilot_continued_responses_request_omits_orphaned_tool_outputs() {
    let config = ProviderConfig {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::GithubCopilot,
        api_transport: ApiTransport::Responses,
        reasoning_effort: "high".to_string(),
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

    let body = build_openai_responses_body(
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &config,
        "system prompt",
        &[
            ApiMessage {
                role: "tool".to_string(),
                content: ApiContent::Text("file list".to_string()),
                reasoning: None,
                tool_call_id: Some("call_orphaned".to_string()),
                name: Some("list_files".to_string()),
                tool_calls: None,
            },
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("continue".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
        ],
        &[],
        Some("resp_123"),
        false,
    );

    let input = body["input"].as_array().expect("input array");
    assert_eq!(body["previous_response_id"], "resp_123");
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["role"], "user");
}

#[test]
fn openrouter_chat_request_includes_provider_routing_preferences() {
    let mut config = responses_test_config(
        "https://openrouter.ai/api/v1".to_string(),
        AuthSource::ApiKey,
    );
    config.model = "anthropic/claude-sonnet-4.5".to_string();
    config.openrouter_provider_order = vec!["anthropic".to_string(), "openai".to_string()];
    config.openrouter_provider_ignore = vec!["deepinfra".to_string()];
    config.openrouter_allow_fallbacks = Some(false);

    let body = super::build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_OPENROUTER,
        &config,
        "system prompt",
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
    .expect("body should build");

    assert_eq!(
        body["provider"]["order"],
        serde_json::json!(["anthropic", "openai"])
    );
    assert_eq!(body["provider"]["ignore"], serde_json::json!(["deepinfra"]));
    assert_eq!(body["provider"]["allow_fallbacks"], false);
}

#[test]
fn huggingface_chat_request_appends_provider_route_to_model() {
    let mut config = responses_test_config(
        "https://router.huggingface.co/v1".to_string(),
        AuthSource::ApiKey,
    );
    config.model = "openai/gpt-oss-120b".to_string();
    config.huggingface_provider = Some(":SambaNova".to_string());

    let body = super::build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_HUGGINGFACE,
        &config,
        "system prompt",
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
    .expect("body should build");

    assert_eq!(body["model"], "openai/gpt-oss-120b:sambanova");

    config.model = "openai/gpt-oss-120b:fastest".to_string();
    config.huggingface_provider = Some("preferred".to_string());
    let body = super::build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_HUGGINGFACE,
        &config,
        "system prompt",
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
    .expect("body should build");
    assert_eq!(body["model"], "openai/gpt-oss-120b:fastest");
}

#[tokio::test]
async fn openrouter_chat_request_sends_response_cache_header_only_when_enabled() {
    let request_texts = Arc::new(Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind openrouter cache listener");
    let addr = listener
        .local_addr()
        .expect("openrouter cache listener addr");
    let request_texts_for_server = request_texts.clone();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut buf = [0_u8; 8192];
            let size = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
            )
            .await
            .expect("read request timed out")
            .expect("read request");
            request_texts_for_server
                .lock()
                .expect("request text lock")
                .push(String::from_utf8_lossy(&buf[..size]).to_string());
            let body = concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
                    "data: [DONE]\n\n"
                );
            let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
            tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let client = reqwest::Client::new();
    for enabled in [false, true] {
        let mut config = responses_test_config(format!("http://{addr}"), AuthSource::ApiKey);
        config.model = "anthropic/claude-sonnet-4.5".to_string();
        config.openrouter_response_cache_enabled = enabled;
        let stream = send_completion_request(
            &client,
            zorai_shared::providers::PROVIDER_ID_OPENROUTER,
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
            ApiTransport::ChatCompletions,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 0,
                retry_delay_ms: 0,
            },
        );
        let _ = collect_chunks(stream).await;
    }
    server.await.expect("openrouter cache server");

    let requests = request_texts.lock().expect("request text lock");
    assert!(
        !requests[0]
            .to_ascii_lowercase()
            .contains("x-openrouter-cache: true"),
        "cache header should be absent when disabled: {}",
        requests[0]
    );
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("x-openrouter-cache: true"),
        "cache header should be present when enabled: {}",
        requests[1]
    );
}

#[test]
fn github_copilot_full_responses_request_omits_orphaned_function_calls() {
    let config = ProviderConfig {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::GithubCopilot,
        api_transport: ApiTransport::Responses,
        reasoning_effort: "high".to_string(),
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

    let body = build_openai_responses_body(
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &config,
        "system prompt",
        &[
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("first question".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text("I'll inspect that".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: Some(vec![ApiToolCall {
                    id: "call_orphaned".to_string(),
                    call_type: "function".to_string(),
                    function: ApiToolCallFunction {
                        name: "read_file".to_string(),
                        arguments: "{\"path\":\"MEMORY.md\"}".to_string(),
                    },
                }]),
            },
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("continue".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
        ],
        &[],
        None,
        false,
    );

    let input = body["input"].as_array().expect("input array");
    assert_eq!(input.len(), 3);
    assert_eq!(input[0]["role"], "user");
    assert_eq!(input[1]["role"], "assistant");
    assert_eq!(input[2]["role"], "user");
    assert!(input
        .iter()
        .all(|item| item.get("type").and_then(|value| value.as_str()) != Some("function_call")));
}

#[test]
fn github_copilot_full_responses_request_omits_orphaned_tool_outputs() {
    let config = ProviderConfig {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::GithubCopilot,
        api_transport: ApiTransport::Responses,
        reasoning_effort: "high".to_string(),
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

    let body = build_openai_responses_body(
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &config,
        "system prompt",
        &[
            ApiMessage {
                role: "tool".to_string(),
                content: ApiContent::Text("file contents".to_string()),
                reasoning: None,
                tool_call_id: Some("call_l5PZSFtWaiVSUe7OkjcI2SPL".to_string()),
                name: Some("read_file".to_string()),
                tool_calls: None,
            },
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("continue".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
        ],
        &[],
        None,
        false,
    );

    let input = body["input"].as_array().expect("input array");
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["role"], "user");
    assert!(input.iter().all(|item| {
        item.get("type").and_then(|value| value.as_str()) != Some("function_call_output")
    }));
}

#[test]
fn github_copilot_full_responses_request_preserves_function_call_history() {
    let config = ProviderConfig {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::GithubCopilot,
        api_transport: ApiTransport::Responses,
        reasoning_effort: "high".to_string(),
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

    let body = build_openai_responses_body(
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &config,
        "system prompt",
        &[
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("first question".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text("I'll inspect that".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: Some(vec![ApiToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ApiToolCallFunction {
                        name: "read_file".to_string(),
                        arguments: "{\"path\":\"MEMORY.md\"}".to_string(),
                    },
                }]),
            },
            ApiMessage {
                role: "tool".to_string(),
                content: ApiContent::Text("file contents".to_string()),
                reasoning: None,
                tool_call_id: Some("call_1".to_string()),
                name: Some("read_file".to_string()),
                tool_calls: None,
            },
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("continue".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
        ],
        &[],
        None,
        false,
    );

    let input = body["input"].as_array().expect("input array");
    assert_eq!(input.len(), 5);
    assert_eq!(input[0]["role"], "user");
    assert_eq!(input[1]["role"], "assistant");
    assert_eq!(input[2]["type"], "function_call");
    assert_eq!(input[2]["call_id"], "call_1");
    assert_eq!(input[2]["name"], "read_file");
    assert_eq!(input[3]["type"], "function_call_output");
    assert_eq!(input[3]["call_id"], "call_1");
    assert_eq!(input[4]["role"], "user");
}

#[tokio::test]
async fn copilot_validation_returns_static_catalog_models() {
    let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _guard = EnvGuard::new(&[
        "ZORAI_PROVIDER_AUTH_DB_PATH",
        "ZORAI_GITHUB_COPILOT_DISABLE_GH_CLI",
        "ZORAI_GITHUB_COPILOT_DISABLE_ENV_AUTH",
        "COPILOT_GITHUB_TOKEN",
        "GH_TOKEN",
        "GITHUB_TOKEN",
    ]);
    let root = tempdir().unwrap();
    let db_path = root.path().join("provider-auth.db");
    std::env::set_var("ZORAI_PROVIDER_AUTH_DB_PATH", &db_path);
    std::env::set_var("ZORAI_GITHUB_COPILOT_DISABLE_GH_CLI", "1");
    std::env::set_var("ZORAI_GITHUB_COPILOT_DISABLE_ENV_AUTH", "1");
    let auth = serde_json::json!({
        "auth_mode": "github_copilot",
        "access_token": "ghu_browser_token",
        "source": "test",
        "updated_at": 1,
        "created_at": 1
    });
    provider_auth_store::save_provider_auth_state(
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        "github_copilot",
        &auth,
    )
    .unwrap();

    let models = validate_provider_connection(
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        "https://api.githubcopilot.com",
        "",
        AuthSource::GithubCopilot,
    )
    .await
    .expect("validation should succeed")
    .expect("copilot validation should return models");

    assert!(models.len() > 10);
    assert_eq!(
        models.first().map(|model| model.id.as_str()),
        Some("gpt-5.4")
    );
    assert!(models.iter().any(|model| model.id == "gpt-5.5"));
    assert!(models.iter().any(|model| model.id == "gpt-5.4"));
    assert!(models.iter().any(|model| model.id == "claude-sonnet-4.6"));
    assert!(models.iter().any(|model| model.id == "raptor-mini"));
    assert!(models.iter().any(|model| model.id == "goldeneye"));
}

#[tokio::test]
async fn provider_without_remote_model_fetch_returns_built_in_catalog() {
    let models = fetch_models("featherless", "http://127.0.0.1:9", "", None)
        .await
        .expect("providers with built-in-only catalogs should not surface a fetch error");

    assert!(!models.is_empty());
    assert_eq!(
        models.first().map(|model| model.id.as_str()),
        Some("meta-llama/Llama-3.3-70B-Instruct")
    );
    assert!(models
        .iter()
        .any(|model| model.id == "Qwen/Qwen2.5-72B-Instruct"));
    assert!(models
        .iter()
        .any(|model| model.id == "mistralai/Mistral-Small-24B-Instruct-2501"));
}

#[tokio::test]
async fn custom_provider_model_fetch_uses_hydrated_custom_auth_definition() {
    let _lock = crate::test_support::env_test_lock();
    let _env_guard = crate::test_support::EnvGuard::new(&["ZORAI_CUSTOM_AUTH_PATH"]);
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let custom_auth_path = temp_dir.path().join("custom-auth.yaml");
    std::fs::write(
        &custom_auth_path,
        r#"
providers:
  - id: local-openai-fetch
    name: Local OpenAI Fetch
    default_base_url: http://127.0.0.1:1/v1
    default_model: local-default
    api_type: openai
    auth_method: bearer
    supports_model_fetch: true
    models:
      - id: local-default
        name: Local Default
        context_window: 64000
"#,
    )
    .expect("write custom auth");
    std::env::set_var("ZORAI_CUSTOM_AUTH_PATH", &custom_auth_path);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind custom model fetch listener");
    let addr = listener.local_addr().expect("listener addr");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept request");
        let mut buffer = [0_u8; 2048];
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buffer),
        )
        .await
        .expect("read request timed out")
        .expect("read request");
        let request = String::from_utf8_lossy(&buffer[..read]);
        assert!(request.contains("GET /v1/models "));
        assert!(request
            .lines()
            .any(|line| line.eq_ignore_ascii_case("authorization: Bearer test-key")));
        let body = r#"{"data":[{"id":"custom-live","name":"Custom Live","context_window":77777}]}"#;
        let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
            .await
            .expect("write response");
    });

    let models = fetch_models(
        "local-openai-fetch",
        &format!("http://{addr}/v1"),
        "test-key",
        None,
    )
    .await
    .expect("custom provider fetch should succeed");

    server.await.expect("server task");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "custom-live");
    assert_eq!(models[0].context_window, Some(77_777));
}

#[tokio::test]
async fn custom_provider_models_url_fetches_from_its_own_endpoint_and_reads_local_server_context() {
    let _lock = crate::test_support::env_test_lock();
    let _env_guard = crate::test_support::EnvGuard::new(&["ZORAI_CUSTOM_AUTH_PATH"]);
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let custom_auth_path = temp_dir.path().join("custom-auth.yaml");
    std::fs::write(
        &custom_auth_path,
        r#"
providers:
  - id: completion-path-provider
    name: Completion Path Provider
    default_base_url: http://127.0.0.1:1/v1/completion
    default_model: placeholder
    models_url: /v1/models
"#,
    )
    .expect("write custom auth");
    std::env::set_var("ZORAI_CUSTOM_AUTH_PATH", &custom_auth_path);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind models_url listener");
    let addr = listener.local_addr().expect("listener addr");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept request");
        let mut buffer = [0_u8; 2048];
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buffer),
        )
        .await
        .expect("read request timed out")
        .expect("read request");
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        let response = if request.starts_with("GET /v1/models ") {
            let body = r#"{"data":[{"id":"vllm-model","max_model_len":32768},{"id":"llama-cpp-model","meta":{"n_ctx_train":131072}}]}"#;
            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
        } else {
            "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string()
        };
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
            .await
            .expect("write response");
        request
    });

    let models = fetch_models(
        "completion-path-provider",
        &format!("http://{addr}/v1/completion"),
        "test-key",
        None,
    )
    .await
    .expect("models_url fetch should succeed");

    let request = server.await.expect("server task");
    assert!(request.starts_with("GET /v1/models "), "{request}");
    let ids = models
        .iter()
        .map(|model| (model.id.as_str(), model.context_window))
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            ("vllm-model", Some(32_768)),
            ("llama-cpp-model", Some(131_072))
        ]
    );
}

#[tokio::test]
async fn minimax_model_fetch_uses_anthropic_v1_models_endpoint() {
    let request = Arc::new(Mutex::new(String::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind minimax model fetch listener");
    let addr = listener.local_addr().expect("listener addr");
    let request_for_server = Arc::clone(&request);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept request");
        let mut buffer = [0_u8; 4096];
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buffer),
        )
        .await
        .expect("read request timed out")
        .expect("read request");
        let raw_request = String::from_utf8_lossy(&buffer[..read]).to_string();
        *request_for_server.lock().expect("lock minimax request") = raw_request.clone();
        let request_line = raw_request.lines().next().unwrap_or_default();

        let response = if request_line.contains("GET /anthropic/v1/models ") {
            let body = r#"{"data":[{"id":"MiniMax-M3","display_name":"MiniMax-M3","type":"model"}],"has_more":false}"#;
            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
        } else {
            "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string()
        };
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
            .await
            .expect("write response");
    });

    let models = fetch_models(
        zorai_shared::providers::PROVIDER_ID_MINIMAX,
        &format!("http://{addr}/anthropic"),
        "minimax-key",
        None,
    )
    .await
    .expect("minimax model fetch should succeed");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "MiniMax-M3");
    assert_eq!(models[0].name.as_deref(), Some("MiniMax-M3"));

    server.await.expect("server task");
    let raw_request = request.lock().expect("lock minimax request");
    assert!(raw_request.starts_with("GET /anthropic/v1/models "));
    assert!(raw_request
        .lines()
        .any(|line| line.eq_ignore_ascii_case("x-api-key: minimax-key")));
}

#[tokio::test]
async fn fetch_models_preserves_remote_metadata_and_pricing() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind model fetch listener");
    let addr = listener.local_addr().expect("model fetch listener addr");

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept model fetch request");
        let mut buf = [0_u8; 4096];
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
        )
        .await;

        let body = serde_json::json!({
            "data": [
                {
                    "id": "openai/gpt-audio",
                    "name": "OpenAI: GPT Audio",
                    "description": "Audio model",
                    "context_length": 128000,
                    "pricing": {
                        "prompt": "0.0000025",
                        "completion": "0.00001",
                        "audio": "0.000032"
                    },
                    "architecture": {
                        "modality": "text+audio->text+audio",
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text", "audio"]
                    }
                }
            ]
        })
        .to_string();
        let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
            .await
            .expect("write model fetch response");
    });

    let models = fetch_models(
        zorai_shared::providers::PROVIDER_ID_OPENROUTER,
        &format!("http://{addr}"),
        "",
        None,
    )
    .await
    .expect("fetch models should succeed");

    server.await.expect("model fetch server task");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "openai/gpt-audio");
    assert_eq!(
        models[0]
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.prompt.as_deref()),
        Some("0.0000025")
    );
    assert_eq!(
        models[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("architecture"))
            .and_then(|architecture| architecture.get("modality"))
            .and_then(|value| value.as_str()),
        Some("text+audio->text+audio")
    );
}

#[tokio::test]
async fn openrouter_filtered_model_fetch_requests_output_modality_query_and_preserves_metadata() {
    let request_lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind openrouter model fetch listener");
    let addr = listener
        .local_addr()
        .expect("openrouter model fetch listener addr");
    let request_lines_for_server = std::sync::Arc::clone(&request_lines);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept model fetch request");
        let mut buf = [0_u8; 4096];
        let size = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
        )
        .await
        .expect("read model fetch request timed out")
        .expect("read model fetch request");
        let request = String::from_utf8_lossy(&buf[..size]).to_string();
        let request_line = request.lines().next().unwrap_or_default().to_string();
        request_lines_for_server
            .lock()
            .expect("lock openrouter request lines")
            .push(request_line);

        let body = serde_json::json!({
            "data": [
                {
                    "id": "openai/gpt-image",
                    "name": "GPT Image",
                    "context_length": 128000,
                    "pricing": {
                        "image": "0.00001"
                    },
                    "architecture": {
                        "input_modalities": ["text"],
                        "output_modalities": ["image"]
                    }
                }
            ]
        })
        .to_string();
        let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
            .await
            .expect("write model fetch response");
    });

    let models = fetch_models(
        zorai_shared::providers::PROVIDER_ID_OPENROUTER,
        &format!("http://{addr}"),
        "",
        Some("image"),
    )
    .await
    .expect("fetch models should succeed");

    server.await.expect("openrouter model fetch server task");

    let request_line = request_lines
        .lock()
        .expect("lock request lines")
        .first()
        .cloned()
        .expect("request line should be recorded");
    assert!(
            request_line.contains("/models?output_modalities=image"),
            "expected OpenRouter model fetch to request image output modality filter, got {request_line}"
        );
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "openai/gpt-image");
    assert_eq!(
        models[0]
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.image.as_deref()),
        Some("0.00001")
    );
    assert_eq!(
        models[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("architecture"))
            .and_then(|architecture| architecture.get("output_modalities"))
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
            }),
        Some(vec!["image"])
    );
}

#[tokio::test]
async fn openrouter_embedding_model_fetch_requests_embeddings_catalog() {
    let request_texts = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind openrouter embedding model fetch listener");
    let addr = listener
        .local_addr()
        .expect("openrouter embedding model fetch listener addr");
    let request_texts_for_server = std::sync::Arc::clone(&request_texts);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("accept embedding model fetch request");
        let mut buf = [0_u8; 4096];
        let size = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
        )
        .await
        .expect("read embedding model fetch request timed out")
        .expect("read embedding model fetch request");
        let request = String::from_utf8_lossy(&buf[..size]).to_string();
        request_texts_for_server
            .lock()
            .expect("lock openrouter request lines")
            .push(request);

        let body = serde_json::json!({
            "data": [
                {
                    "id": "openai/text-embedding-3-small",
                    "name": "Text Embedding 3 Small",
                    "context_length": 8192,
                    "architecture": {
                        "input_modalities": ["text"],
                        "output_modalities": ["embeddings"]
                    }
                },
                {
                    "id": "qwen/qwen3-embedding-0.6b",
                    "name": "Qwen3 Embedding 0.6B",
                    "context_length": 32768,
                    "architecture": {
                        "input_modalities": ["text"],
                        "output_modalities": ["embeddings"]
                    }
                }
            ]
        })
        .to_string();
        let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
            .await
            .expect("write embedding model fetch response");
    });

    let models = fetch_models(
        zorai_shared::providers::PROVIDER_ID_OPENROUTER,
        &format!("http://{addr}"),
        "",
        Some("embedding"),
    )
    .await
    .expect("fetch embedding models should succeed");

    server
        .await
        .expect("openrouter embedding model fetch server task");

    let request_text = request_texts
        .lock()
        .expect("lock request texts")
        .first()
        .cloned()
        .expect("request text should be recorded");
    let request_line = request_text.lines().next().unwrap_or_default();
    assert!(
            request_line.contains("/embeddings/models"),
            "expected OpenRouter embedding model fetch to request embeddings catalog, got {request_line}"
        );
    assert!(
            request_text.contains("http-referer: https://zorai.app\r\n"),
            "expected OpenRouter embedding model fetch to include attribution referer header, got {request_text}"
        );
    assert!(
            request_text.contains("x-openrouter-title: Zorai\r\n"),
            "expected OpenRouter embedding model fetch to include attribution title header, got {request_text}"
        );
    assert!(
            request_text.contains("x-openrouter-categories: cli-agent,personal-agent\r\n"),
            "expected OpenRouter embedding model fetch to include attribution categories header, got {request_text}"
        );
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "openai/text-embedding-3-small");
    assert_eq!(models[1].id, "qwen/qwen3-embedding-0.6b");
}

#[tokio::test]
async fn chutes_model_fetch_retries_without_auth_after_invalid_token() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind chutes model fetch listener");
    let addr = listener
        .local_addr()
        .expect("chutes model fetch listener addr");

    let server = tokio::spawn(async move {
        for attempt in 0..2 {
            let (mut stream, _) = listener.accept().await.expect("accept model fetch request");
            let mut buf = [0_u8; 4096];
            let size = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
            )
            .await
            .expect("read model fetch request timed out")
            .expect("read model fetch request");
            let request = String::from_utf8_lossy(&buf[..size]).to_string();
            let has_auth = request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("authorization: Bearer bad-key"));

            let (status, body) = if attempt == 0 {
                assert!(has_auth, "first request should include bearer auth");
                (
                    "401 Unauthorized",
                    r#"{"detail":"Invalid token."}"#.to_string(),
                )
            } else {
                assert!(
                    !has_auth,
                    "retry request should omit bearer auth for chutes catalog"
                );
                (
                    "200 OK",
                    serde_json::json!({
                        "data": [
                            {
                                "id": "Qwen/Qwen3-32B-TEE",
                                "context_length": 40960
                            },
                            {
                                "id": "deepseek-ai/DeepSeek-R1",
                                "context_length": 131072
                            }
                        ]
                    })
                    .to_string(),
                )
            };
            let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
                .await
                .expect("write model fetch response");
        }
    });

    let models = fetch_models(
        zorai_shared::providers::PROVIDER_ID_CHUTES,
        &format!("http://{addr}"),
        "bad-key",
        None,
    )
    .await
    .expect("chutes fetch should fall back to unauthenticated catalog");

    server.await.expect("model fetch server task");

    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "Qwen/Qwen3-32B-TEE");
    assert_eq!(models[1].id, "deepseek-ai/DeepSeek-R1");
}

#[tokio::test]
async fn azure_openai_validation_uses_models_endpoint() {
    let request_paths = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind azure validation listener");
    let addr = listener
        .local_addr()
        .expect("azure validation listener addr");
    let request_paths_for_server = std::sync::Arc::clone(&request_paths);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("accept azure validation request");
        let mut buf = [0_u8; 4096];
        let size = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
        )
        .await
        .expect("read azure validation request timed out")
        .expect("read azure validation request");
        let request = String::from_utf8_lossy(&buf[..size]).to_string();
        let request_line = request.lines().next().unwrap_or_default().to_string();
        request_paths_for_server
            .lock()
            .expect("lock azure validation request paths")
            .push(request_line.clone());

        let response = if request_line.contains("GET /openai/v1/models ") {
            let body = "{\"data\":[{\"id\":\"gpt-4.1\"}]}";
            format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
        } else {
            "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string()
        };
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
            .await
            .expect("write azure validation response");
    });

    validate_provider_connection(
        zorai_shared::providers::PROVIDER_ID_AZURE_OPENAI,
        &format!("http://{addr}/openai/v1"),
        "azure-key",
        AuthSource::ApiKey,
    )
    .await
    .expect("azure validation should succeed");

    server.await.expect("azure validation server task");
    let request_paths = request_paths
        .lock()
        .expect("lock azure validation request paths");
    assert_eq!(request_paths.len(), 1);
    assert!(request_paths[0].contains("GET /openai/v1/models "));
    assert!(!request_paths[0].contains("/chat/completions"));
}

#[tokio::test]
async fn azure_openai_validation_surfaces_models_auth_failures() {
    let request_paths = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind azure auth failure listener");
    let addr = listener
        .local_addr()
        .expect("azure auth failure listener addr");
    let request_paths_for_server = std::sync::Arc::clone(&request_paths);

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("accept azure auth failure request");
        let mut buf = [0_u8; 4096];
        let size = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
        )
        .await
        .expect("read azure auth failure request timed out")
        .expect("read azure auth failure request");
        let request = String::from_utf8_lossy(&buf[..size]).to_string();
        let request_line = request.lines().next().unwrap_or_default().to_string();
        request_paths_for_server
            .lock()
            .expect("lock azure auth failure request paths")
            .push(request_line.clone());

        let response = if request_line.contains("GET /openai/v1/models ") {
            let body = "invalid azure";
            format!(
                    "HTTP/1.1 401 Unauthorized\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
        } else {
            "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string()
        };
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
            .await
            .expect("write azure auth failure response");
    });

    let err = validate_provider_connection(
        zorai_shared::providers::PROVIDER_ID_AZURE_OPENAI,
        &format!("http://{addr}/openai/v1"),
        "azure-key",
        AuthSource::ApiKey,
    )
    .await
    .expect_err("azure validation should surface auth failure");

    server.await.expect("azure auth failure server task");
    let message = err.to_string();
    assert!(message.contains("401"));
    let request_paths = request_paths
        .lock()
        .expect("lock azure auth failure request paths");
    assert_eq!(request_paths.len(), 1);
    assert!(request_paths[0].contains("GET /openai/v1/models "));
    assert!(!request_paths[0].contains("/chat/completions"));
}

async fn responses_sse_test_response(
    body: &str,
) -> (reqwest::Response, tokio::task::JoinHandle<()>) {
    let client = reqwest::Client::new();
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
        let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
    });
    let response = client
        .get(format!("http://{addr}"))
        .send()
        .await
        .expect("send test request");
    (response, server)
}

#[tokio::test]
async fn copilot_responses_parser_collects_reasoning_summary_parts() {
    let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_123\"}}\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_1\",\"encrypted_content\":\"enc\"}}\n",
            "data: {\"type\":\"response.reasoning_summary_part.added\",\"item_id\":\"rs_1\",\"summary_index\":0}\n",
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_1\",\"summary_index\":0,\"delta\":\"thinking\"}\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_responses_sse(
        response,
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &tx,
    )
    .await
    .expect("parse should succeed");
    drop(tx);

    let mut reasoning_deltas = Vec::new();
    let mut final_reasoning = None;
    while let Some(chunk) = rx.recv().await {
        match chunk.expect("chunk") {
            CompletionChunk::Delta {
                reasoning: Some(reasoning),
                ..
            } => reasoning_deltas.push(reasoning),
            CompletionChunk::Done {
                reasoning: Some(reasoning),
                ..
            } => final_reasoning = Some(reasoning),
            _ => {}
        }
    }

    assert_eq!(reasoning_deltas, vec!["thinking".to_string()]);
    assert_eq!(final_reasoning.as_deref(), Some("thinking"));
    server.await.expect("server task");
}

#[tokio::test]
async fn copilot_responses_parser_collects_reasoning_summary_from_output_item_done() {
    let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_456\"}}\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_2\",\"encrypted_content\":\"enc\"}}\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_2_rotated\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"final reasoning\"}]}}\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":4}}}\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_responses_sse(
        response,
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &tx,
    )
    .await
    .expect("parse should succeed");
    drop(tx);

    let mut reasoning_deltas = Vec::new();
    let mut final_reasoning = None;
    while let Some(chunk) = rx.recv().await {
        match chunk.expect("chunk") {
            CompletionChunk::Delta {
                reasoning: Some(reasoning),
                ..
            } => reasoning_deltas.push(reasoning),
            CompletionChunk::Done {
                reasoning: Some(reasoning),
                ..
            } => final_reasoning = Some(reasoning),
            _ => {}
        }
    }

    assert!(reasoning_deltas.is_empty());
    assert_eq!(final_reasoning.as_deref(), Some("final reasoning"));
    server.await.expect("server task");
}

#[tokio::test]
async fn copilot_responses_parser_tolerates_unknown_events_before_completion() {
    let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_unknown\"}}\n",
            "data: {\"type\":\"response.custom_ignored\",\"value\":\"ignored\"}\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":7}}}\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_responses_sse(
        response,
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &tx,
    )
    .await
    .expect("parse should succeed");
    drop(tx);

    let mut deltas = Vec::new();
    let mut done_content = None;
    while let Some(chunk) = rx.recv().await {
        match chunk.expect("chunk") {
            CompletionChunk::Delta { content, .. } if !content.is_empty() => deltas.push(content),
            CompletionChunk::Done { content, .. } => done_content = Some(content),
            _ => {}
        }
    }

    assert_eq!(deltas, vec!["hello".to_string()]);
    assert_eq!(done_content.as_deref(), Some("hello"));
    server.await.expect("server task");
}

#[tokio::test]
async fn openai_responses_parser_uses_total_tokens_when_output_details_are_hidden() {
    let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_usage_total\"}}\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_usage_total\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":75,\"input_tokens_details\":{\"cached_tokens\":12},\"output_tokens\":100,\"output_tokens_details\":{\"reasoning_tokens\":1024},\"total_tokens\":1261},\"error\":null}}\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_responses_sse(response, zorai_shared::providers::PROVIDER_ID_OPENAI, &tx)
        .await
        .expect("parse should succeed");
    drop(tx);

    let mut done_tokens = None;
    while let Some(chunk) = rx.recv().await {
        if let CompletionChunk::Done {
            input_tokens,
            output_tokens,
            cache_read_input_tokens,
            provider_final_result,
            ..
        } = chunk.expect("chunk")
        {
            let (final_usage, persisted_cache_read) = match provider_final_result
                .expect("provider result")
            {
                crate::agent::types::CompletionProviderFinalResult::OpenAiResponses(response) => (
                    response.response.expect("terminal response").usage,
                    response.cache_read_input_tokens,
                ),
                other => panic!("expected responses provider result, got {other:?}"),
            };
            done_tokens = Some((
                input_tokens,
                output_tokens,
                cache_read_input_tokens,
                persisted_cache_read,
                final_usage,
            ));
            break;
        }
    }

    let (input_tokens, output_tokens, cache_read_input_tokens, persisted_cache_read, final_usage) =
        done_tokens.expect("expected terminal done chunk");
    assert_eq!(input_tokens, 75);
    assert_eq!(output_tokens, 1186);
    assert_eq!(cache_read_input_tokens, Some(12));
    assert_eq!(persisted_cache_read, Some(12));
    assert_eq!(final_usage.total_tokens, Some(1261));
    assert_eq!(
        final_usage
            .output_tokens_details
            .as_ref()
            .unwrap()
            .reasoning_tokens,
        Some(1024)
    );
    server.await.expect("server task");
}

#[tokio::test]
async fn copilot_responses_parser_rejects_chat_completions_payloads() {
    let body = concat!(
        "data: {\"id\":\"chatcmpl_123\",\"choices\":[{\"delta\":{\"content\":\"legacy\"}}]}\n",
        "data: [DONE]\n"
    );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, _rx) = mpsc::channel(8);
    let err = parse_openai_responses_sse(
        response,
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &tx,
    )
    .await
    .expect_err("choices payload should be rejected");

    let failure = upstream_failure_error(&err).expect("structured upstream failure");
    assert_eq!(failure.class, UpstreamFailureClass::TransportIncompatible);
    server.await.expect("server task");
}

#[tokio::test]
async fn openai_responses_parser_classifies_context_window_overflow_for_recovery() {
    let body = concat!(
            "data: {\"type\":\"error\",\"error\":{\"code\":\"context_length_exceeded\",\"message\":\"Your input exceeds the context window of this model. Please adjust your input and try again.\"}}\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_responses_sse(response, zorai_shared::providers::PROVIDER_ID_OPENAI, &tx)
        .await
        .expect("parse should complete with an error chunk");
    drop(tx);

    let mut saw_error = None;
    while let Some(chunk) = rx.recv().await {
        if let CompletionChunk::Error { message } = chunk.expect("chunk") {
            saw_error = Some(message);
            break;
        }
    }

    let message = saw_error.expect("expected structured error chunk");
    let diagnostics = parse_structured_error(&message);
    assert_eq!(diagnostics.class, "context_window_exceeded");
    assert_eq!(diagnostics.diagnostics["event_type"], "error");
    assert_eq!(
        diagnostics.diagnostics["upstream_error"]["code"],
        "context_length_exceeded"
    );
    server.await.expect("server task");
}

#[tokio::test]
async fn copilot_responses_parser_surfaces_response_failed_as_structured_error() {
    let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_failed\"}}\n",
            "data: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp_failed\",\"status\":\"failed\",\"usage\":{\"input_tokens\":2,\"output_tokens\":0,\"total_tokens\":2},\"error\":{\"code\":\"server_error\",\"message\":\"Over capacity\"}}}\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_responses_sse(
        response,
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &tx,
    )
    .await
    .expect("parse should complete with an error chunk");
    drop(tx);

    let mut saw_error = None;
    while let Some(chunk) = rx.recv().await {
        if let CompletionChunk::Error { message } = chunk.expect("chunk") {
            saw_error = Some(message);
            break;
        }
    }

    let message = saw_error.expect("expected structured error chunk");
    let diagnostics = parse_structured_error(&message);
    assert_eq!(diagnostics.class, "temporary_upstream");
    assert_eq!(diagnostics.diagnostics["event_type"], "response.failed");
    assert_eq!(diagnostics.diagnostics["response_id"], "resp_failed");
    assert_eq!(
        diagnostics.diagnostics["upstream_error"]["code"],
        "server_error"
    );
    assert_eq!(
        diagnostics.diagnostics["upstream_error"]["message"],
        "Over capacity"
    );
    server.await.expect("server task");
}

#[tokio::test]
async fn copilot_responses_parser_classifies_missing_tool_call_output_as_request_invalid() {
    let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_failed\"}}\n",
            "data: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp_failed\",\"status\":\"failed\",\"usage\":{\"input_tokens\":2,\"output_tokens\":0,\"total_tokens\":2},\"error\":{\"code\":\"bad_request\",\"message\":\"No tool call found for function call output with call_id call_oTvo8eFwoaFskvWlhGcUp5rU.\"}}}\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_responses_sse(
        response,
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &tx,
    )
    .await
    .expect("parse should complete with an error chunk");
    drop(tx);

    let mut saw_error = None;
    while let Some(chunk) = rx.recv().await {
        if let CompletionChunk::Error { message } = chunk.expect("chunk") {
            saw_error = Some(message);
            break;
        }
    }

    let message = saw_error.expect("expected structured error chunk");
    let diagnostics = parse_structured_error(&message);
    assert_eq!(diagnostics.class, "request_invalid");
    assert_eq!(diagnostics.diagnostics["event_type"], "response.failed");
    assert_eq!(diagnostics.diagnostics["response_id"], "resp_failed");
    assert_eq!(
        diagnostics.diagnostics["upstream_error"]["code"],
        "bad_request"
    );
    assert_eq!(
        diagnostics.diagnostics["upstream_error"]["message"],
        "No tool call found for function call output with call_id call_oTvo8eFwoaFskvWlhGcUp5rU."
    );
    server.await.expect("server task");
}

#[tokio::test]
async fn copilot_responses_parser_surfaces_top_level_error_as_structured_error() {
    let body = concat!(
            "data: {\"type\":\"error\",\"error\":{\"code\":\"rate_limit_exceeded\",\"message\":\"Slow down\",\"param\":\"model\"}}\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_responses_sse(
        response,
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &tx,
    )
    .await
    .expect("parse should complete with an error chunk");
    drop(tx);

    let mut saw_error = None;
    while let Some(chunk) = rx.recv().await {
        if let CompletionChunk::Error { message } = chunk.expect("chunk") {
            saw_error = Some(message);
            break;
        }
    }

    let message = saw_error.expect("expected structured error chunk");
    let diagnostics = parse_structured_error(&message);
    assert_eq!(diagnostics.class, "rate_limit");
    assert_eq!(diagnostics.diagnostics["event_type"], "error");
    assert_eq!(
        diagnostics.diagnostics["upstream_error"]["code"],
        "rate_limit_exceeded"
    );
    assert_eq!(
        diagnostics.diagnostics["upstream_error"]["message"],
        "Slow down"
    );
    assert_eq!(diagnostics.diagnostics["upstream_error"]["param"], "model");
    server.await.expect("server task");
}

#[tokio::test]
async fn copilot_responses_parser_errors_on_truncated_stream_without_terminal_event() {
    let body = concat!(
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_truncated\"}}\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n"
    );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, _rx) = mpsc::channel(8);
    let err = parse_openai_responses_sse(
        response,
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &tx,
    )
    .await
    .expect_err("unterminated responses stream should fail");

    assert!(
        err.to_string().contains("terminal") || err.to_string().contains("truncated"),
        "unexpected error: {err:#}"
    );
    server.await.expect("server task");
}

#[tokio::test]
async fn copilot_responses_parser_errors_on_malformed_event_payload() {
    let body = concat!(
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_bad_json\"}}\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"unterminated\"\n"
    );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, _rx) = mpsc::channel(8);
    let err = parse_openai_responses_sse(
        response,
        zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
        &tx,
    )
    .await
    .expect_err("malformed responses event should fail");

    assert!(
        err.to_string().contains("malformed") || err.to_string().contains("json"),
        "unexpected error: {err:#}"
    );
    server.await.expect("server task");
}

#[tokio::test]
async fn responses_parser_accepts_data_lines_without_space_after_colon() {
    let body = concat!(
        "data:{\"type\":\"response.created\",\"response\":{\"id\":\"resp_no_space\"}}\n",
        "data:{\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n",
        "data:{\"type\":\"response.completed\",\"response\":{\"id\":\"resp_no_space\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":2,\"output_tokens\":3,\"total_tokens\":5},\"error\":null}}\n",
        "data:[DONE]\n"
    );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_responses_sse(response, zorai_shared::providers::PROVIDER_ID_OPENAI, &tx)
        .await
        .expect("parse should succeed");
    drop(tx);

    let mut streamed = String::new();
    let mut done = None;
    while let Some(chunk) = rx.recv().await {
        match chunk.expect("chunk") {
            CompletionChunk::Delta { content, .. } => streamed.push_str(&content),
            CompletionChunk::Done {
                content,
                input_tokens,
                output_tokens,
                ..
            } => done = Some((content, input_tokens, output_tokens)),
            _ => {}
        }
    }

    assert_eq!(streamed, "hello");
    assert_eq!(done, Some(("hello".to_string(), 2, 3)));
    server.await.expect("server task");
}

#[tokio::test]
async fn chat_completions_parser_provider_final_result_uses_normalized_tool_calls() {
    let body = concat!(
            "data: {\"id\":\"chatcmpl_tool_final\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"name\":\"second\",\"arguments\":\"\"}}]}}]}\n",
            "data: {\"id\":\"chatcmpl_tool_final\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_0\",\"function\":{\"name\":\"first\",\"arguments\":\"{\\\"a\\\":1}\"}},{\"index\":1,\"function\":{\"arguments\":\"{\\\"b\\\":2}\"}}]}}]}\n",
            "data: {\"id\":\"chatcmpl_tool_final\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3}}\n",
            "data: [DONE]\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_sse(response, &tx)
        .await
        .expect("parse should succeed");
    drop(tx);

    let mut emitted_tool_calls = None;
    let mut final_result_tool_calls = None;
    while let Some(chunk) = rx.recv().await {
        if let CompletionChunk::ToolCalls {
            tool_calls,
            provider_final_result,
            ..
        } = chunk.expect("chunk")
        {
            final_result_tool_calls = Some(
                match provider_final_result
                    .expect("tool chunk should include provider final result")
                {
                    crate::agent::types::CompletionProviderFinalResult::OpenAiChatCompletions(
                        response,
                    ) => response.tool_calls,
                    other => panic!("expected OpenAI Chat Completions final result, got {other:?}"),
                },
            );
            emitted_tool_calls = Some(tool_calls);
            break;
        }
    }

    let emitted_tool_calls = emitted_tool_calls.expect("expected tool-calls chunk");
    let final_result_tool_calls =
        final_result_tool_calls.expect("expected provider final result tool calls");

    assert_eq!(emitted_tool_calls.len(), 2);
    assert_eq!(final_result_tool_calls.len(), emitted_tool_calls.len());
    assert_eq!(emitted_tool_calls[0].id, "call_0");
    assert_eq!(emitted_tool_calls[0].function.name, "first");
    assert_eq!(
        emitted_tool_calls[1].id,
        "synthetic_tool_call_openai_1_second"
    );
    assert_eq!(emitted_tool_calls[1].function.name, "second");
    assert_eq!(final_result_tool_calls[0].id, emitted_tool_calls[0].id);
    assert_eq!(
        final_result_tool_calls[0].function.name,
        emitted_tool_calls[0].function.name
    );
    assert_eq!(
        final_result_tool_calls[0].function.arguments,
        emitted_tool_calls[0].function.arguments
    );
    assert_eq!(final_result_tool_calls[1].id, emitted_tool_calls[1].id);
    assert_eq!(
        final_result_tool_calls[1].function.name,
        emitted_tool_calls[1].function.name
    );
    assert_eq!(
        final_result_tool_calls[1].function.arguments,
        emitted_tool_calls[1].function.arguments
    );

    server.await.expect("server task");
}

async fn chat_sse_split_test_response(
    first: &str,
    second: &str,
) -> (reqwest::Response, tokio::task::JoinHandle<()>) {
    let client = reqwest::Client::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let first = first.to_string();
    let second = second.to_string();
    let total = first.len() + second.len();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let header = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            total
        );
        let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, header.as_bytes()).await;
        let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, first.as_bytes()).await;
        let _ = tokio::io::AsyncWriteExt::flush(&mut socket).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, second.as_bytes()).await;
        let _ = tokio::io::AsyncWriteExt::flush(&mut socket).await;
    });
    let response = client
        .get(format!("http://{addr}"))
        .send()
        .await
        .expect("send test request");
    (response, server)
}

#[tokio::test]
async fn chat_completions_parser_reassembles_data_line_split_across_chunks() {
    let first =
        "data: {\"id\":\"c1\",\"model\":\"glm-5.2\",\"choices\":[{\"delta\":{\"content\":\"hel";
    let second = concat!(
        "lo world\"}}]}\n",
        "data: {\"id\":\"c1\",\"model\":\"glm-5.2\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n",
        "data: [DONE]\n"
    );
    let (response, server) = chat_sse_split_test_response(first, second).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_sse(response, &tx)
        .await
        .expect("parse should succeed");
    drop(tx);

    let mut streamed = String::new();
    let mut done_content = None;
    while let Some(chunk) = rx.recv().await {
        match chunk.expect("chunk") {
            CompletionChunk::Delta { content, .. } => streamed.push_str(&content),
            CompletionChunk::Done { content, .. } => done_content = Some(content),
            _ => {}
        }
    }

    assert_eq!(streamed, "hello world");
    assert_eq!(done_content.as_deref(), Some("hello world"));
    server.await.expect("server task");
}

#[tokio::test]
async fn chat_completions_parser_accepts_data_lines_without_space_after_colon() {
    let body = concat!(
        "data:{\"id\":\"chatcmpl_kimi\",\"model\":\"kimi-k2.6\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":null}}]}\n",
        "data:{\"id\":\"chatcmpl_kimi\",\"model\":\"kimi-k2.6\",\"choices\":[{\"delta\":{\"reasoning_content\":\"thinking\"}}]}\n",
        "data:{\"id\":\"chatcmpl_kimi\",\"model\":\"kimi-k2.6\",\"choices\":[{\"delta\":{\"content\":\"p\"}}]}\n",
        "data:{\"id\":\"chatcmpl_kimi\",\"model\":\"kimi-k2.6\",\"choices\":[{\"delta\":{\"content\":\"ong\"}}]}\n",
        "data:{\"id\":\"chatcmpl_kimi\",\"model\":\"kimi-k2.6\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n",
        "data:{\"id\":\"chatcmpl_kimi\",\"model\":\"kimi-k2.6\",\"choices\":[],\"usage\":{\"prompt_tokens\":13,\"completion_tokens\":50,\"total_tokens\":63}}\n",
        "data: [DONE]\n"
    );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_sse(response, &tx)
        .await
        .expect("parse should succeed");
    drop(tx);

    let mut streamed = String::new();
    let mut reasoning_deltas = Vec::new();
    let mut done = None;
    while let Some(chunk) = rx.recv().await {
        match chunk.expect("chunk") {
            CompletionChunk::Delta { content, reasoning } => {
                streamed.push_str(&content);
                if let Some(reasoning) = reasoning {
                    reasoning_deltas.push(reasoning);
                }
            }
            CompletionChunk::Done {
                content,
                reasoning,
                input_tokens,
                output_tokens,
                provider_final_result,
                ..
            } => {
                let final_result = match provider_final_result.expect("provider result") {
                    crate::agent::types::CompletionProviderFinalResult::OpenAiChatCompletions(
                        response,
                    ) => response,
                    other => panic!("expected chat completions provider result, got {other:?}"),
                };
                done = Some((
                    content,
                    reasoning,
                    input_tokens,
                    output_tokens,
                    final_result.output_text,
                    final_result.reasoning,
                    final_result.input_tokens,
                    final_result.output_tokens,
                ));
            }
            _ => {}
        }
    }

    let (
        done_content,
        done_reasoning,
        input_tokens,
        output_tokens,
        final_output_text,
        final_reasoning,
        final_input_tokens,
        final_output_tokens,
    ) = done.expect("expected terminal done chunk");
    assert_eq!(streamed, "pong");
    assert_eq!(reasoning_deltas, vec!["thinking".to_string()]);
    assert_eq!(done_content, "pong");
    assert_eq!(done_reasoning.as_deref(), Some("thinking"));
    assert_eq!(input_tokens, 13);
    assert_eq!(output_tokens, 50);
    assert_eq!(final_output_text, "pong");
    assert_eq!(final_reasoning.as_deref(), Some("thinking"));
    assert_eq!(final_input_tokens, Some(13));
    assert_eq!(final_output_tokens, Some(50));
    server.await.expect("server task");
}

#[tokio::test]
async fn chat_completions_parser_uses_total_tokens_when_completion_details_are_hidden() {
    let body = concat!(
            "data: {\"id\":\"chatcmpl_usage_total\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n",
            "data: {\"id\":\"chatcmpl_usage_total\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":75,\"prompt_tokens_details\":{\"cached_tokens\":12},\"completion_tokens\":100,\"completion_tokens_details\":{\"reasoning_tokens\":1024},\"total_tokens\":1261}}\n",
            "data: [DONE]\n"
        );
    let (response, server) = responses_sse_test_response(body).await;

    let (tx, mut rx) = mpsc::channel(8);
    parse_openai_sse(response, &tx)
        .await
        .expect("parse should succeed");
    drop(tx);

    let mut done_tokens = None;
    while let Some(chunk) = rx.recv().await {
        if let CompletionChunk::Done {
            input_tokens,
            output_tokens,
            cache_read_input_tokens,
            provider_final_result,
            ..
        } = chunk.expect("chunk")
        {
            let final_result_tokens = match provider_final_result.expect("provider result") {
                crate::agent::types::CompletionProviderFinalResult::OpenAiChatCompletions(
                    response,
                ) => (
                    response.input_tokens.expect("input tokens"),
                    response.output_tokens.expect("output tokens"),
                    response.cache_read_input_tokens,
                ),
                other => panic!("expected chat completions provider result, got {other:?}"),
            };
            done_tokens = Some((
                input_tokens,
                output_tokens,
                cache_read_input_tokens,
                final_result_tokens,
            ));
            break;
        }
    }

    let (input_tokens, output_tokens, cache_read_input_tokens, final_result_tokens) =
        done_tokens.expect("expected terminal done chunk");
    assert_eq!(input_tokens, 75);
    assert_eq!(output_tokens, 1186);
    assert_eq!(cache_read_input_tokens, Some(12));
    assert_eq!(final_result_tokens, (75, 1186, Some(12)));
    server.await.expect("server task");
}

#[test]
fn ollama_model_detail_extracts_context_window_and_vision_capability() {
    let show = serde_json::json!({
        "details": { "family": "kimi-k2", "parameter_size": "1042000000000" },
        "model_info": {
            "general.architecture": "kimi-k2",
            "kimi-k2.context_length": 262144
        },
        "capabilities": ["vision", "thinking", "completion", "tools"]
    });

    let (context_window, metadata) = super::parse_ollama_model_detail(&show);

    assert_eq!(context_window, Some(262144));

    let features = zorai_shared::providers::derive_model_feature_capabilities(
        zorai_shared::providers::PROVIDER_ID_OLLAMA,
        "kimi-k2.7-code:cloud",
        metadata.as_ref(),
        false,
    );
    assert!(
        features.vision,
        "vision capability must propagate into derived model features"
    );
}

#[test]
fn ollama_model_detail_without_vision_does_not_report_vision() {
    let show = serde_json::json!({
        "model_info": { "llama.context_length": 131072 },
        "capabilities": ["completion", "tools"]
    });

    let (context_window, metadata) = super::parse_ollama_model_detail(&show);

    assert_eq!(context_window, Some(131072));

    let features = zorai_shared::providers::derive_model_feature_capabilities(
        zorai_shared::providers::PROVIDER_ID_OLLAMA,
        "llama3.1",
        metadata.as_ref(),
        false,
    );
    assert!(!features.vision);
}
