use super::*;
#[test]
fn anthropic_request_defaults_to_top_level_ephemeral_cache_control() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
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
        &[],
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
    assert_eq!(body["cache_control"]["type"], "ephemeral");
    assert!(body["cache_control"]["ttl"].is_null());
}

#[test]
fn anthropic_request_pins_system_prefix_with_long_lived_cache_breakpoint_for_claude() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
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
        "you are a stable system prompt",
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

    let body: serde_json::Value = serde_json::from_slice(
        request
            .body()
            .and_then(|body| body.as_bytes())
            .expect("body bytes"),
    )
    .expect("json body");

    // Tools render before `system`, so a breakpoint here caches tools + system
    // as one stable segment, separate from the volatile message tail.
    let system_block = &body["system"][0];
    assert_eq!(system_block["type"], "text");
    assert_eq!(system_block["text"], "you are a stable system prompt");
    assert_eq!(system_block["cache_control"]["type"], "ephemeral");
}

#[test]
fn anthropic_request_keeps_string_system_for_non_claude_providers() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
        base_url: "https://example.com".to_string(),
        model: "some-other-model".to_string(),
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
        &[],
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
    assert_eq!(body["system"], "system");
}

#[test]
fn anthropic_request_includes_sampling_and_stop_sequence_fields_when_configured() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
        base_url: "https://api.anthropic.com".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        api_key: "secret".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "off".to_string(),
        context_window_tokens: 0,
        response_schema: None,
        stop_sequences: Some(vec!["END".to_string(), "DONE".to_string()]),
        temperature: Some(0.25),
        top_p: Some(0.9),
        top_k: Some(42),
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
        &[],
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
    assert_eq!(body["stop_sequences"], serde_json::json!(["END", "DONE"]));
    assert_eq!(body["temperature"], 0.25);
    assert_eq!(body["top_p"], 0.9);
    assert_eq!(body["top_k"], 42);
}

#[test]
fn anthropic_request_includes_metadata_and_routing_fields_when_configured() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
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
        metadata: Some(AnthropicRequestMetadata {
            user_id: "operator-123".to_string(),
        }),
        service_tier: Some("standard_only".to_string()),
        container: Some("container_123".to_string()),
        inference_geo: Some("us".to_string()),
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
        &[],
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
    assert_eq!(body["metadata"]["user_id"], "operator-123");
    assert_eq!(body["service_tier"], "standard_only");
    assert_eq!(body["container"], "container_123");
    assert_eq!(body["inference_geo"], "us");
}

#[test]
fn anthropic_request_includes_top_level_cache_control_when_configured() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
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
        cache_control: Some(AnthropicCacheControlEphemeral {
            cache_type: "ephemeral".to_string(),
            ttl: Some("1h".to_string()),
        }),
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
        &[],
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
    assert_eq!(body["cache_control"]["type"], "ephemeral");
    assert_eq!(body["cache_control"]["ttl"], "1h");
}

#[test]
fn anthropic_request_uses_configured_max_tokens() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
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
        max_tokens: Some(2048),
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
        &[],
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
    assert_eq!(body["max_tokens"], 2048);
}

#[test]
fn anthropic_request_uses_configured_tool_choice_override() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
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
        anthropic_tool_choice: Some(AnthropicToolChoice::Tool {
            name: "ping".to_string(),
            disable_parallel_tool_use: Some(true),
        }),
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
    assert_eq!(body["tool_choice"]["type"], "tool");
    assert_eq!(body["tool_choice"]["name"], "ping");
    assert_eq!(body["tool_choice"]["disable_parallel_tool_use"], true);
}

#[test]
fn anthropic_request_includes_output_effort_when_configured() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
        base_url: "https://api.anthropic.com".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        api_key: "secret".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "off".to_string(),
        context_window_tokens: 0,
        response_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            }
        })),
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
        output_effort: Some("low".to_string()),
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
        &[],
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
    assert_eq!(body["output_config"]["effort"], "low");
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
}

#[test]
fn anthropic_request_includes_output_effort_without_schema() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        presence_penalty: None,
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
        output_effort: Some("high".to_string()),
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
        &[],
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
    assert_eq!(body["output_config"]["effort"], "high");
    assert!(body["output_config"]["format"].is_null());
}
