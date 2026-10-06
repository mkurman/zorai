use super::part1::*;
use super::*;
use crate::agent::provider_auth_store;
use crate::agent::types::{AgentContentBlock, AgentMessage, MessageRole};
use crate::test_support::EnvGuard;
use std::collections::VecDeque;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::tempdir;
use zorai_shared::providers::{
    PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_KIMI_CODING_PLAN, PROVIDER_ID_OPENAI,
    PROVIDER_ID_OPENCODE_GO, PROVIDER_ID_OPENROUTER,
};

#[test]
fn messages_to_api_format_normalizes_empty_tool_ids() {
    let messages = vec![
        AgentMessage {
            id: String::new(),
            role: MessageRole::Assistant,
            content: String::new(),
            content_blocks: Vec::new(),
            tool_calls: Some(vec![ToolCall {
                id: String::new(),
                function: ToolFunction {
                    name: "list_sessions".to_string(),
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
            timestamp: 42,
            feedback: None,
        },
        AgentMessage {
            id: String::new(),
            role: MessageRole::Tool,
            content: "ok".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: Some(String::new()),
            tool_name: Some("list_sessions".to_string()),
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
            timestamp: 43,
            feedback: None,
        },
    ];

    let api_messages = messages_to_api_format(&messages);
    assert_eq!(api_messages.len(), 2);
    let assistant_tool_id = api_messages[0]
        .tool_calls
        .as_ref()
        .and_then(|calls| calls.first())
        .map(|call| call.id.clone())
        .expect("assistant tool call should have normalized id");
    assert!(!assistant_tool_id.is_empty());
    assert_eq!(
        api_messages[1].tool_call_id.as_deref(),
        Some(assistant_tool_id.as_str())
    );
}

#[test]
fn messages_to_api_format_preserves_assistant_reasoning() {
    let mut message = AgentMessage::user("", 42);
    message.role = MessageRole::Assistant;
    message.reasoning = Some("checked the workspace before choosing a tool".to_string());
    message.tool_calls = Some(vec![ToolCall {
        id: "call_1".to_string(),
        function: ToolFunction {
            name: "list_sessions".to_string(),
            arguments: "{}".to_string(),
        },
        weles_review: None,
    }]);

    let api_messages = messages_to_api_format(&[message]);

    assert_eq!(
        api_messages[0].reasoning.as_deref(),
        Some("checked the workspace before choosing a tool")
    );
}

#[test]
fn messages_to_api_format_normalizes_invalid_tool_call_arguments() {
    let mut message = AgentMessage::user("", 42);
    message.role = MessageRole::Assistant;
    message.tool_calls = Some(vec![ToolCall {
        id: "call_bad_args".to_string(),
        function: ToolFunction {
            name: tool_names::UPDATE_TODO.to_string(),
            arguments: r#"{"items":[{"content":"Read spec","status":"in_progress"}]"#.to_string(),
        },
        weles_review: None,
    }]);

    let api_messages = messages_to_api_format(&[message]);
    let arguments = api_messages[0]
        .tool_calls
        .as_ref()
        .and_then(|calls| calls.first())
        .map(|call| call.function.arguments.as_str())
        .expect("tool call arguments should exist");

    let parsed: serde_json::Value =
        serde_json::from_str(arguments).expect("tool call arguments should be valid JSON");
    assert_eq!(
        parsed["_invalid_tool_arguments"],
        r#"{"items":[{"content":"Read spec","status":"in_progress"}]"#
    );
}

#[test]
fn chat_completion_messages_empty_assistant_content_for_tool_calls() {
    let messages = vec![ApiMessage {
        role: "assistant".to_string(),
        content: ApiContent::Text("I'll inspect that now".to_string()),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: Some(vec![ApiToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ApiToolCallFunction {
                name: "list_sessions".to_string(),
                arguments: "{}".to_string(),
            },
        }]),
    }];

    let serialized = build_chat_completion_messages("system prompt", &messages).expect("serialize");
    assert_eq!(serialized.len(), 2);
    assert_eq!(serialized[1]["role"], "assistant");
    assert_eq!(serialized[1]["content"], "");
    assert_eq!(serialized[1]["tool_calls"][0]["id"], "call_1");
}

#[test]
fn chat_completions_rewrites_responses_image_block_types() {
    let mut message = AgentMessage::user("hi", 1);
    message.content_blocks = vec![
        AgentContentBlock::text("hi"),
        AgentContentBlock::Image {
            url: None,
            data_url: Some("data:image/png;base64,abc".to_string()),
            mime_type: Some("image/png".to_string()),
        },
    ];

    let api_messages = messages_to_api_format(&[message]);
    let ApiContent::Blocks(internal_blocks) = &api_messages[0].content else {
        panic!("image attachments must stay structured in the shared API format");
    };
    assert_eq!(internal_blocks[0]["type"], "input_text");
    assert_eq!(internal_blocks[1]["type"], "input_image");
    assert_eq!(internal_blocks[1]["image_url"], "data:image/png;base64,abc");

    let responses = messages_to_responses_input(PROVIDER_ID_OPENAI, &api_messages, None);
    assert_eq!(responses[0]["content"][0]["type"], "input_text");
    assert_eq!(responses[0]["content"][1]["type"], "input_image");

    let serialized =
        build_chat_completion_messages("system prompt", &api_messages).expect("serialize");
    assert_eq!(
        serialized[1]["content"],
        serde_json::json!([
            {"type": "text", "text": "hi"},
            {
                "type": "image_url",
                "image_url": {"url": "data:image/png;base64,abc"}
            }
        ])
    );
}

#[test]
fn chat_completions_keeps_already_valid_multimodal_blocks() {
    let messages = vec![ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Blocks(vec![
            serde_json::json!({"type": "text", "text": "describe this"}),
            serde_json::json!({
                "type": "image_url",
                "image_url": {"url": "https://example.test/cat.png"}
            }),
            serde_json::json!({
                "type": "input_audio",
                "input_audio": {"data": "abc", "format": "wav"}
            }),
        ]),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    }];

    let serialized = build_chat_completion_messages("system prompt", &messages).expect("serialize");
    assert_eq!(
        serialized[1]["content"],
        serde_json::json!([
            {"type": "text", "text": "describe this"},
            {
                "type": "image_url",
                "image_url": {"url": "https://example.test/cat.png"}
            },
            {
                "type": "input_audio",
                "input_audio": {"data": "abc", "format": "wav"}
            }
        ])
    );
}

#[test]
fn openrouter_chat_completion_messages_preserve_reasoning_content_for_tool_calls() {
    let messages = vec![ApiMessage {
        role: "assistant".to_string(),
        content: ApiContent::Text("I'll inspect that now".to_string()),
        reasoning: Some("Need the directory listing before answering.".to_string()),
        tool_call_id: None,
        name: None,
        tool_calls: Some(vec![ApiToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ApiToolCallFunction {
                name: "list_sessions".to_string(),
                arguments: "{}".to_string(),
            },
        }]),
    }];

    let serialized = build_chat_completion_messages_with_options(
        "system prompt",
        &messages,
        true,
        false,
        true,
        false,
    )
    .expect("serialize");

    assert_eq!(
        serialized[1]["reasoning_content"],
        "Need the directory listing before answering."
    );
}

#[test]
fn openrouter_chat_completion_messages_add_legacy_tool_reasoning_placeholder() {
    let messages = vec![ApiMessage {
        role: "assistant".to_string(),
        content: ApiContent::Text(String::new()),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: Some(vec![ApiToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ApiToolCallFunction {
                name: "list_sessions".to_string(),
                arguments: "{}".to_string(),
            },
        }]),
    }];

    let serialized = build_chat_completion_messages_with_options(
        "system prompt",
        &messages,
        true,
        true,
        true,
        false,
    )
    .expect("serialize");

    assert_eq!(serialized[1]["reasoning_content"], " ");
}

#[test]
fn openrouter_chat_request_drops_empty_assistant_messages_without_tool_calls() {
    let mut config = responses_test_config(
        "https://openrouter.ai/api/v1".to_string(),
        AuthSource::ApiKey,
    );
    config.model = "deepseek/deepseek-chat-v3.1".to_string();

    let body = build_openai_chat_completions_body(
        PROVIDER_ID_OPENROUTER,
        &config,
        "system prompt",
        &[
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text(String::new()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("Background operation finished.".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
        ],
        &[],
    )
    .expect("body should build");

    let messages = body["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
}

#[test]
fn openrouter_deepseek_keeps_reasoning_only_assistant_with_empty_content() {
    let mut config = responses_test_config(
        "https://openrouter.ai/api/v1".to_string(),
        AuthSource::ApiKey,
    );
    config.model = "deepseek/deepseek-v4.1-flash".to_string();

    let body = build_openai_chat_completions_body(
        PROVIDER_ID_OPENROUTER,
        &config,
        "system prompt",
        &[
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("Continue.".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text(String::new()),
                reasoning: Some("Need to inspect the completed operation.".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
        ],
        &[],
    )
    .expect("body should build");

    let messages = body["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(messages[2]["content"], "");
    assert_eq!(
        messages[2]["reasoning_content"],
        "Need to inspect the completed operation."
    );
    assert!(messages[2].get("tool_calls").is_none());
}

#[test]
fn openrouter_deepseek_tool_call_assistant_uses_empty_string_content() {
    let mut config = responses_test_config(
        "https://openrouter.ai/api/v1".to_string(),
        AuthSource::ApiKey,
    );
    config.model = "deepseek/deepseek-v4.1-flash".to_string();

    let body = build_openai_chat_completions_body(
        PROVIDER_ID_OPENROUTER,
        &config,
        "system prompt",
        &[ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text(String::new()),
            reasoning: Some("calling a tool".to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![ApiToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ApiToolCallFunction {
                    name: "list_files".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
        }],
        &[],
    )
    .expect("body should build");

    assert_eq!(body["messages"][1]["content"], "");
    assert!(!body["messages"][1]["content"].is_null());
    assert_eq!(body["messages"][1]["tool_calls"][0]["id"], "call_1");
}

#[test]
fn openrouter_chat_request_opts_into_usage_accounting_for_cost() {
    let mut config = responses_test_config(
        "https://openrouter.ai/api/v1".to_string(),
        AuthSource::ApiKey,
    );
    config.model = "deepseek/deepseek-chat-v3.1".to_string();
    let user = ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text("hi".to_string()),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    };

    let body = build_openai_chat_completions_body(
        PROVIDER_ID_OPENROUTER,
        &config,
        "sys",
        &[user.clone()],
        &[],
    )
    .expect("body should build");
    assert_eq!(
        body["usage"]["include"].as_bool(),
        Some(true),
        "OpenRouter must opt into usage accounting so usage.cost (USD) is returned"
    );

    let openai_config =
        responses_test_config("https://api.openai.com/v1".to_string(), AuthSource::ApiKey);
    let openai_body =
        build_openai_chat_completions_body(PROVIDER_ID_OPENAI, &openai_config, "sys", &[user], &[])
            .expect("body should build");
    assert!(
        openai_body.get("usage").is_none(),
        "non-OpenRouter providers must not receive the OpenRouter usage flag"
    );
}

#[test]
fn openrouter_chat_request_pins_prompt_cache_to_the_thread_session() {
    let config = responses_test_config(
        "https://openrouter.ai/api/v1".to_string(),
        AuthSource::ApiKey,
    );
    let user = ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text("continue".to_string()),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    };
    let mut body = build_openai_chat_completions_body(
        PROVIDER_ID_OPENROUTER,
        &config,
        "sys",
        &[user],
        &[],
    )
    .expect("body should build");
    apply_openrouter_session_id(PROVIDER_ID_OPENROUTER, Some("thread-1"), &mut body);
    assert_eq!(
        body["session_id"].as_str(),
        Some("thread-1"),
        "OpenRouter sticky routing keys off session_id, not the per-call generation id"
    );

    apply_openrouter_session_id(PROVIDER_ID_OPENAI, Some("thread-1"), &mut body);
    assert_eq!(body["session_id"].as_str(), Some("thread-1"));
    let mut openai_body = serde_json::json!({});
    apply_openrouter_session_id(PROVIDER_ID_OPENAI, Some("thread-1"), &mut openai_body);
    assert!(openai_body.get("session_id").is_none());

    let mut rejected = serde_json::json!({});
    apply_openrouter_session_id(PROVIDER_ID_OPENROUTER, Some("  "), &mut rejected);
    apply_openrouter_session_id(
        PROVIDER_ID_OPENROUTER,
        Some(&"x".repeat(257)),
        &mut rejected,
    );
    assert!(rejected.get("session_id").is_none());

    let client = reqwest::Client::new();
    let request = apply_openrouter_session_header(
        client.post("https://openrouter.ai/api/v1/chat/completions"),
        PROVIDER_ID_OPENROUTER,
        Some("thread-1"),
    )
    .build()
    .expect("request should build");
    assert_eq!(
        request
            .headers()
            .get("x-session-id")
            .and_then(|value| value.to_str().ok()),
        Some("thread-1")
    );
}

#[test]
fn openrouter_chat_request_drops_assistant_messages_with_empty_content_blocks_without_tool_calls() {
    let mut config = responses_test_config(
        "https://openrouter.ai/api/v1".to_string(),
        AuthSource::ApiKey,
    );
    config.model = "deepseek/deepseek-chat-v3.1".to_string();

    let body = build_openai_chat_completions_body(
        PROVIDER_ID_OPENROUTER,
        &config,
        "system prompt",
        &[
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Blocks(vec![serde_json::json!({
                    "type": "input_text",
                    "text": ""
                })]),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("Continue after placeholder.".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
        ],
        &[],
    )
    .expect("body should build");

    let messages = body["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
}

#[test]
fn deepseek_chat_request_preserves_tool_reasoning_content() {
    let mut config =
        responses_test_config("https://api.deepseek.com".to_string(), AuthSource::ApiKey);
    config.model = "deepseek-v4-pro".to_string();
    config.reasoning_effort = "high".to_string();

    let body = build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_DEEPSEEK,
        &config,
        "system prompt",
        &[ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text(String::new()),
            reasoning: Some("Need the date before calling weather.".to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![ApiToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ApiToolCallFunction {
                    name: "get_date".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
        }],
        &[],
    )
    .expect("body should build");

    assert_eq!(
        body["messages"][1]["reasoning_content"],
        "Need the date before calling weather."
    );
}

#[test]
fn opencode_go_deepseek_model_preserves_tool_reasoning_content() {
    let mut config = responses_test_config(
        "https://opencode.ai/zen/go/v1".to_string(),
        AuthSource::ApiKey,
    );
    config.model = "deepseek-v4-pro".to_string();
    config.reasoning_effort = "high".to_string();

    let body = build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_OPENCODE_GO,
        &config,
        "system prompt",
        &[ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text(String::new()),
            reasoning: Some("Need the date before calling weather.".to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![ApiToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ApiToolCallFunction {
                    name: "get_date".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
        }],
        &[],
    )
    .expect("body should build");

    assert_eq!(
        body["messages"][1]["reasoning_content"],
        "Need the date before calling weather."
    );
}

#[test]
fn opencode_go_non_deepseek_model_omits_tool_reasoning_content() {
    let mut config = responses_test_config(
        "https://opencode.ai/zen/go/v1".to_string(),
        AuthSource::ApiKey,
    );
    config.model = "glm-5.1".to_string();
    config.reasoning_effort = "high".to_string();

    let body = build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_OPENCODE_GO,
        &config,
        "system prompt",
        &[ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text(String::new()),
            reasoning: Some("internal".to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![ApiToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ApiToolCallFunction {
                    name: "get_date".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
        }],
        &[],
    )
    .expect("body should build");

    assert!(body["messages"][1].get("reasoning_content").is_none());
}

#[test]
fn deepseek_chat_request_adds_legacy_tool_reasoning_placeholder() {
    let mut config =
        responses_test_config("https://api.deepseek.com".to_string(), AuthSource::ApiKey);
    config.model = "deepseek-v4-pro".to_string();
    config.reasoning_effort = "high".to_string();

    let body = build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_DEEPSEEK,
        &config,
        "system prompt",
        &[ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text(String::new()),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![ApiToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ApiToolCallFunction {
                    name: "get_date".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
        }],
        &[],
    )
    .expect("body should build");

    assert_eq!(body["messages"][1]["reasoning_content"], " ");
}

#[test]
fn deepseek_chat_request_repairs_missing_tool_reasoning_when_effort_unset() {
    let mut config =
        responses_test_config("https://api.deepseek.com".to_string(), AuthSource::ApiKey);
    config.model = "deepseek-v4-pro".to_string();
    config.reasoning_effort = String::new();

    let body = build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_DEEPSEEK,
        &config,
        "system prompt",
        &[
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text(String::new()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: Some(vec![ApiToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ApiToolCallFunction {
                        name: "get_date".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
            },
            ApiMessage {
                role: "tool".to_string(),
                content: ApiContent::Text("2026-05-02".to_string()),
                reasoning: None,
                tool_call_id: Some("call_1".to_string()),
                name: Some("get_date".to_string()),
                tool_calls: None,
            },
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text("Done.".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
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
        ],
        &[],
    )
    .expect("body should build");

    assert_eq!(body["messages"][1]["reasoning_content"], " ");
    assert_eq!(body["messages"][3]["reasoning_content"], " ");
}

#[test]
fn deepseek_chat_request_preserves_final_answer_reasoning_after_tool_call_turn() {
    let mut config =
        responses_test_config("https://api.deepseek.com".to_string(), AuthSource::ApiKey);
    config.model = "deepseek-v4-pro".to_string();
    config.reasoning_effort = "high".to_string();

    let body = build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_DEEPSEEK,
        &config,
        "system prompt",
        &[
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("What's the weather tomorrow?".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text(String::new()),
                reasoning: Some("Need tomorrow's date before weather.".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: Some(vec![ApiToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ApiToolCallFunction {
                        name: "get_date".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
            },
            ApiMessage {
                role: "tool".to_string(),
                content: ApiContent::Text("2026-05-02".to_string()),
                reasoning: None,
                tool_call_id: Some("call_1".to_string()),
                name: Some("get_date".to_string()),
                tool_calls: None,
            },
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text("The weather tomorrow is cloudy.".to_string()),
                reasoning: Some("The weather result is ready to summarize.".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("What about another city?".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
        ],
        &[],
    )
    .expect("body should build");

    assert_eq!(
        body["messages"][4]["reasoning_content"],
        "The weather result is ready to summarize."
    );
}

#[test]
fn deepseek_chat_request_omits_standalone_final_answer_reasoning_content() {
    let mut config =
        responses_test_config("https://api.deepseek.com".to_string(), AuthSource::ApiKey);
    config.model = "deepseek-v4-pro".to_string();
    config.reasoning_effort = "high".to_string();

    let body = build_openai_chat_completions_body(
        zorai_shared::providers::PROVIDER_ID_DEEPSEEK,
        &config,
        "system prompt",
        &[ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text("The date is 2026-05-01.".to_string()),
            reasoning: Some("No tool was needed.".to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }],
        &[],
    )
    .expect("body should build");

    assert!(body["messages"][1].get("reasoning_content").is_none());
}

#[test]
fn github_models_chat_completion_url_uses_inference_prefix() {
    assert_eq!(
        build_chat_completion_url("https://models.github.ai"),
        "https://models.github.ai/inference/chat/completions"
    );
    assert_eq!(
        build_chat_completion_url("https://models.github.ai/inference"),
        "https://models.github.ai/inference/chat/completions"
    );
}

#[test]
fn github_copilot_default_url_uses_copilot_api_origin() {
    let provider = get_provider_definition(PROVIDER_ID_GITHUB_COPILOT).expect("copilot provider");
    assert_eq!(provider.default_base_url, "https://api.githubcopilot.com");
    assert_eq!(
        build_chat_completion_url(provider.default_base_url),
        "https://api.githubcopilot.com/chat/completions"
    );
}

#[test]
fn deepseek_default_url_uses_documented_chat_completion_endpoint() {
    let provider = get_provider_definition(zorai_shared::providers::PROVIDER_ID_DEEPSEEK)
        .expect("deepseek provider");
    assert_eq!(provider.default_base_url, "https://api.deepseek.com");
    assert_eq!(
        build_chat_completion_url(provider.default_base_url),
        "https://api.deepseek.com/chat/completions"
    );
}

#[test]
fn github_copilot_stored_auth_adds_bearer_header_to_requests() {
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
        PROVIDER_ID_GITHUB_COPILOT,
        "github_copilot",
        &auth,
    )
    .unwrap();

    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://models.github.ai".to_string(),
        model: "openai/gpt-4.1".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::GithubCopilot,
        api_transport: ApiTransport::ChatCompletions,
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
    };
    let request = apply_openai_auth_headers(
        client.get("https://models.github.ai/inference/chat/completions"),
        PROVIDER_ID_GITHUB_COPILOT,
        &config,
        CopilotInitiator::User,
    )
    .build()
    .expect("request should build");

    assert_eq!(
        request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer ghu_browser_token")
    );
}

#[test]
fn github_copilot_requests_include_copilot_headers() {
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
        PROVIDER_ID_GITHUB_COPILOT,
        "github_copilot",
        &auth,
    )
    .unwrap();

    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: "gpt-4.1".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::GithubCopilot,
        api_transport: ApiTransport::ChatCompletions,
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
    };
    let request = apply_openai_auth_headers(
        client.get("https://api.githubcopilot.com/chat/completions"),
        PROVIDER_ID_GITHUB_COPILOT,
        &config,
        CopilotInitiator::User,
    )
    .build()
    .expect("request should build");

    assert_eq!(
        request
            .headers()
            .get("openai-intent")
            .and_then(|value| value.to_str().ok()),
        Some("conversation-edits")
    );
    assert_eq!(
        request
            .headers()
            .get("x-initiator")
            .and_then(|value| value.to_str().ok()),
        Some("user")
    );
    assert!(request.headers().get("user-agent").is_some());
}

#[test]
fn github_copilot_internal_requests_use_agent_initiator() {
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
        PROVIDER_ID_GITHUB_COPILOT,
        "github_copilot",
        &auth,
    )
    .unwrap();

    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: "gpt-4.1".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::GithubCopilot,
        api_transport: ApiTransport::ChatCompletions,
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
    };
    let request = apply_openai_auth_headers(
        client.get("https://api.githubcopilot.com/chat/completions"),
        PROVIDER_ID_GITHUB_COPILOT,
        &config,
        CopilotInitiator::Agent,
    )
    .build()
    .expect("request should build");

    assert_eq!(
        request
            .headers()
            .get("x-initiator")
            .and_then(|value| value.to_str().ok()),
        Some("agent")
    );
}

#[test]
fn kimi_coding_plan_requests_include_sdk_headers() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://api.kimi.com/coding/v1".to_string(),
        model: "kimi-for-coding".to_string(),
        api_key: "test-key".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
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
    };

    let request = apply_dashscope_coding_plan_sdk_headers(
        build_openai_auth_request(
            &client,
            "https://api.kimi.com/coding/v1/chat/completions",
            PROVIDER_ID_KIMI_CODING_PLAN,
            &config,
            CopilotInitiator::User,
            false,
        ),
        PROVIDER_ID_KIMI_CODING_PLAN,
        &config.base_url,
        ApiType::OpenAI,
    )
    .build()
    .expect("request should build");

    assert_eq!(
        request
            .headers()
            .get("user-agent")
            .and_then(|value| value.to_str().ok()),
        Some("zorai/1.0.0")
    );
    assert_eq!(
        request
            .headers()
            .get("x-stainless-lang")
            .and_then(|value| value.to_str().ok()),
        Some("js")
    );
    assert_eq!(
        request
            .headers()
            .get("x-stainless-package-version")
            .and_then(|value| value.to_str().ok()),
        Some("4.3.0")
    );
}

#[test]
fn openrouter_requests_include_app_attribution_headers() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://openrouter.ai/api/v1".to_string(),
        model: "arcee-ai/trinity-large-thinking".to_string(),
        api_key: "openrouter-key".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
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
    };

    let request = apply_openai_auth_headers(
        client.get("https://openrouter.ai/api/v1/chat/completions"),
        PROVIDER_ID_OPENROUTER,
        &config,
        CopilotInitiator::Agent,
    )
    .build()
    .expect("request should build");

    assert_eq!(
        request
            .headers()
            .get("http-referer")
            .and_then(|value| value.to_str().ok()),
        Some("https://zorai.app")
    );
    assert_eq!(
        request
            .headers()
            .get("x-openrouter-title")
            .and_then(|value| value.to_str().ok()),
        Some("Zorai")
    );
    assert_eq!(
        request
            .headers()
            .get("x-openrouter-categories")
            .and_then(|value| value.to_str().ok()),
        Some("cli-agent,personal-agent")
    );
}

#[test]
fn opencode_go_requests_include_session_affinity_headers() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://opencode.ai/zen/go/v1".to_string(),
        model: "glm-5.1".to_string(),
        api_key: "opencode-go-key".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
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
    };

    let request = apply_opencode_go_headers(
        apply_openai_auth_headers(
            client.get("https://opencode.ai/zen/go/v1/chat/completions"),
            PROVIDER_ID_OPENCODE_GO,
            &config,
            CopilotInitiator::Agent,
        ),
        PROVIDER_ID_OPENCODE_GO,
        Some("thread-opencode-go-1"),
    )
    .build()
    .expect("request should build");

    assert_eq!(
        request
            .headers()
            .get("user-agent")
            .and_then(|value| value.to_str().ok()),
        Some("zorai-daemon")
    );
    assert_eq!(
        request
            .headers()
            .get("x-opencode-client")
            .and_then(|value| value.to_str().ok()),
        Some("zorai")
    );
    assert_eq!(
        request
            .headers()
            .get("x-opencode-session")
            .and_then(|value| value.to_str().ok()),
        Some("thread-opencode-go-1")
    );

    let mut headers = reqwest::header::HeaderMap::new();
    insert_opencode_go_headers(
        &mut headers,
        PROVIDER_ID_OPENCODE_GO,
        Some("thread-opencode-go-anthropic"),
    );
    assert_eq!(
        headers
            .get("x-opencode-session")
            .and_then(|value| value.to_str().ok()),
        Some("thread-opencode-go-anthropic")
    );
    assert_eq!(
        headers
            .get("x-opencode-client")
            .and_then(|value| value.to_str().ok()),
        Some("zorai")
    );
}

#[test]
fn github_copilot_responses_request_includes_reasoning_summary() {
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
        PROVIDER_ID_GITHUB_COPILOT,
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
        None,
        false,
    );

    assert_eq!(body["reasoning"]["effort"], "high");
    assert_eq!(body["reasoning"]["summary"], "auto");
}

#[test]
fn responses_request_sets_tool_choice_when_tools_are_present() {
    let config = ProviderConfig {
        base_url: "https://api.openai.com".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::Responses,
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

    let body = build_openai_responses_body(
        PROVIDER_ID_OPENAI,
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
        &[ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "update_memory".to_string(),
                description: "Store durable memory".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string" }
                    },
                    "required": ["content"]
                }),
            },
        }],
        None,
        false,
    );

    assert_eq!(body["tool_choice"], "auto");
}

#[test]
fn build_openai_responses_request_omits_previous_response_id_for_chatgpt_subscription() {
    let config = ProviderConfig {
        base_url: "https://api.openai.com/v1".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::ChatgptSubscription,
        api_transport: ApiTransport::Responses,
        reasoning_effort: "high".to_string(),
        context_window_tokens: 0,
        response_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"]
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
        output_effort: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        openrouter_response_cache_enabled: false,
        huggingface_provider: None,
    };

    let request = build_openai_responses_request(
        PROVIDER_ID_OPENAI,
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
        ],
        &[ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "update_memory".to_string(),
                description: "Store durable memory".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string" }
                    },
                    "required": ["content"]
                }),
            },
        }],
        Some("resp_123"),
        true,
    );

    let body = serde_json::to_value(&request).expect("serialize request");

    assert_eq!(body["model"], "gpt-5.4");
    assert_eq!(body["instructions"], "system prompt");
    assert!(body.get("previous_response_id").is_none());
    assert_eq!(body["stream"], true);
    assert_eq!(body["input"][0]["role"], "user");
    assert_eq!(body["input"].as_array().map(Vec::len), Some(4));
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["name"], "update_memory");
    assert_eq!(body["tool_choice"], "auto");
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert_eq!(body["text"]["verbosity"], "high");
    assert_eq!(body["reasoning"]["effort"], "high");
    assert!(body["reasoning"].get("summary").is_none());
    assert_eq!(body["store"], false);
    assert_eq!(
        body["include"],
        serde_json::json!(["reasoning.encrypted_content"])
    );
}

#[test]
fn anthropic_request_maps_response_schema_to_output_config_json_schema() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://api.anthropic.com".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "off".to_string(),
        context_window_tokens: 0,
        response_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"]
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

    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
    assert_eq!(
        body["output_config"]["format"]["schema"],
        config.response_schema.unwrap()
    );
}

#[tokio::test]
async fn request_invalid_responses_400_malformed_body_is_classified_request_invalid() {
    let request_paths = Arc::new(Mutex::new(VecDeque::new()));
    let chat_requests = Arc::new(AtomicUsize::new(0));
    let base_url = spawn_responses_error_server(
        "400 Bad Request",
        r#"{"error":{"message":"Invalid 'input[12].name': empty string"}}"#.to_string(),
        None,
        request_paths,
        chat_requests,
    )
    .await;

    let stream = send_completion_request(
        &reqwest::Client::new(),
        PROVIDER_ID_GITHUB_COPILOT,
        &responses_test_config(base_url, AuthSource::GithubCopilot),
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
    assert!(diagnostics
        .diagnostics
        .to_string()
        .contains("input[12].name"));
}

#[tokio::test]
async fn temporary_upstream_responses_503_is_classified_temporary_upstream() {
    let request_paths = Arc::new(Mutex::new(VecDeque::new()));
    let chat_requests = Arc::new(AtomicUsize::new(0));
    let base_url = spawn_responses_error_server(
        "503 Service Unavailable",
        r#"{"error":{"message":"Service unavailable, try again later"}}"#.to_string(),
        None,
        request_paths,
        chat_requests,
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
    let message = match chunks.last().expect("terminal chunk") {
        CompletionChunk::Error { message } => message,
        other => panic!("expected error chunk, got {other:?}"),
    };
    let diagnostics = parse_structured_error(message);
    assert_eq!(diagnostics.class, "temporary_upstream");
    assert!(diagnostics.diagnostics.to_string().contains("503"));
}

#[tokio::test]
async fn rate_limit_responses_429_uses_retry_after_delay_from_body() {
    let request_paths = Arc::new(Mutex::new(VecDeque::new()));
    let chat_requests = Arc::new(AtomicUsize::new(0));
    let base_url = spawn_responses_error_server(
        "429 Too Many Requests",
        r#"{"message":"You are being rate limited.","retry_after":0.641,"global":false}"#
            .to_string(),
        None,
        request_paths,
        chat_requests,
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
            max_retries: 1,
            retry_delay_ms: 5_000,
        },
    );

    let chunks = collect_chunks(stream).await;
    let retry = chunks
        .iter()
        .find_map(|chunk| match chunk {
            CompletionChunk::Retry { delay_ms, .. } => Some(*delay_ms),
            _ => None,
        })
        .expect("stream should emit retry chunk");
    assert_eq!(retry, 641);

    let message = match chunks.last().expect("terminal chunk") {
        CompletionChunk::Error { message } => message,
        other => panic!("expected error chunk, got {other:?}"),
    };
    let diagnostics = parse_structured_error(message);
    assert_eq!(diagnostics.class, "rate_limit");
    assert_eq!(diagnostics.diagnostics["retry_after_ms"], 641);
}

#[tokio::test]
async fn rate_limit_responses_429_uses_openrouter_raw_message_without_truncation() {
    let request_paths = Arc::new(Mutex::new(VecDeque::new()));
    let chat_requests = Arc::new(AtomicUsize::new(0));
    let raw_detail = format!(
            "{}qwen/qwen3.6-plus:free is temporarily rate-limited upstream. Please retry shortly, or add your own key to accumulate your rate limits.",
            "prefix ".repeat(40)
        );
    let body = serde_json::json!({
        "error": {
            "message": "Provider returned error",
            "code": 429,
            "metadata": {
                "raw": raw_detail
            }
        }
    })
    .to_string();
    let base_url = spawn_responses_error_server(
        "429 Too Many Requests",
        body,
        None,
        request_paths,
        chat_requests,
    )
    .await;

    let stream = send_completion_request(
        &reqwest::Client::new(),
        PROVIDER_ID_OPENROUTER,
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
            max_retries: 1,
            retry_delay_ms: 5_000,
        },
    );

    let chunks = collect_chunks(stream).await;
    let retry_message = chunks
        .iter()
        .find_map(|chunk| match chunk {
            CompletionChunk::Retry { message, .. } => Some(message.as_str()),
            _ => None,
        })
        .expect("stream should emit retry chunk");
    assert!(
        retry_message.contains("add your own key to accumulate your rate limits"),
        "retry chunk should preserve the detailed upstream raw message: {retry_message}"
    );

    let terminal_message = match chunks.last().expect("terminal chunk") {
        CompletionChunk::Error { message } => message,
        other => panic!("expected error chunk, got {other:?}"),
    };
    let diagnostics = parse_structured_error(terminal_message);
    assert_eq!(diagnostics.class, "rate_limit");
    assert!(
        diagnostics
            .summary
            .contains("add your own key to accumulate your rate limits"),
        "terminal summary should preserve the detailed upstream raw message: {}",
        diagnostics.summary
    );
    assert!(
        diagnostics.diagnostics["raw_message"]
            .as_str()
            .is_some_and(|value| value.contains("add your own key to accumulate your rate limits")),
        "structured diagnostics should preserve the detailed upstream raw message"
    );
}

#[test]
fn claude_code_cli_request_transport_is_forced_to_native_subprocess() {
    use zorai_shared::providers::{PROVIDER_ID_CLAUDE_CODE_CLI, PROVIDER_ID_OPENAI};

    assert_eq!(
        coerce_transport_for_provider(PROVIDER_ID_CLAUDE_CODE_CLI, ApiTransport::ChatCompletions),
        ApiTransport::NativeAssistant
    );
    assert_eq!(
        coerce_transport_for_provider(PROVIDER_ID_CLAUDE_CODE_CLI, ApiTransport::Responses),
        ApiTransport::NativeAssistant
    );
    assert_eq!(
        coerce_transport_for_provider(
            zorai_shared::providers::PROVIDER_ID_CURSOR,
            ApiTransport::ChatCompletions
        ),
        ApiTransport::NativeAssistant
    );
    assert_eq!(
        coerce_transport_for_provider(PROVIDER_ID_OPENAI, ApiTransport::ChatCompletions),
        ApiTransport::ChatCompletions,
        "other providers keep whatever transport was resolved"
    );
}
