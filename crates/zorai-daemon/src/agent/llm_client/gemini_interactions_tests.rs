use super::*;
use crate::agent::types::ToolFunctionDef;

fn user(text: &str) -> ApiMessage {
    ApiMessage {
        role: "user".into(),
        content: ApiContent::Text(text.into()),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    }
}

#[test]
fn interaction_body_keeps_stateless_history_tools_and_thought_signature() {
    let assistant = ApiMessage {
        role: "assistant".into(),
        content: ApiContent::Text(String::new()),
        reasoning: Some(format!("{GEMINI_THOUGHT_MARKER}sig-1")),
        tool_call_id: None,
        name: None,
        tool_calls: Some(vec![ApiToolCall {
            id: "call_1".into(),
            call_type: "function".into(),
            function: ApiToolCallFunction {
                name: "get_weather".into(),
                arguments: r#"{"location":"London"}"#.into(),
            },
        }]),
    };
    let tool = ApiMessage {
        role: "tool".into(),
        content: ApiContent::Text(r#"{"temperature":"22"}"#.into()),
        reasoning: None,
        tool_call_id: Some("call_1".into()),
        name: Some("get_weather".into()),
        tool_calls: None,
    };
    let image = ApiMessage {
        role: "user".into(),
        content: ApiContent::Blocks(vec![serde_json::json!({
            "type": "input_image",
            "image_url": "data:image/png;base64,aGVsbG8="
        })]),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    };
    let tools = [ToolDefinition {
        tool_type: "function".into(),
        function: ToolFunctionDef {
            name: "get_weather".into(),
            description: "Weather".into(),
            parameters: serde_json::json!({"type": "object"}),
        },
    }];
    let body = build_gemini_interaction_body(
        "gemini-3.8-flash",
        "Be brief",
        &[user("Hi"), assistant, tool, image],
        &tools,
        true,
        None,
    );

    assert_eq!(body["model"], "gemini-3.8-flash");
    assert_eq!(body["store"], true);
    assert!(body.get("previous_interaction_id").is_none());
    assert_eq!(body["system_instruction"], "Be brief");
    assert_eq!(body["tools"][0]["name"], "get_weather");
    assert_eq!(body["input"][1]["type"], "thought");
    assert_eq!(body["input"][1]["signature"], "sig-1");
    assert_eq!(body["input"][2]["type"], "function_call");
    assert_eq!(body["input"][2]["arguments"]["location"], "London");
    assert_eq!(body["input"][3]["call_id"], "call_1");
    assert_eq!(body["input"][4]["content"][0]["mime_type"], "image/png");
    assert_eq!(body["input"][4]["content"][0]["data"], "aGVsbG8=");
}

#[test]
fn continuation_sends_only_the_new_turn_against_the_stored_interaction() {
    let tool = ApiMessage {
        role: "tool".into(),
        content: ApiContent::Text(r#"{"temperature":"22"}"#.into()),
        reasoning: None,
        tool_call_id: Some("call_1".into()),
        name: Some("get_weather".into()),
        tool_calls: None,
    };
    let body = build_gemini_interaction_body(
        "gemini-3.8-flash",
        "Be brief",
        &[tool],
        &[],
        true,
        Some("v1_stored"),
    );
    assert_eq!(body["store"], true);
    assert_eq!(body["previous_interaction_id"], "v1_stored");
    assert!(body.get("system_instruction").is_none());
    assert_eq!(body["input"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["input"][0]["type"], "function_result");
}

#[test]
fn sse_stream_emits_text_and_stores_usage() {
    let sse = "\
event: step.start
data: {\"index\":1,\"step\":{\"type\":\"model_output\"},\"event_type\":\"step.start\"}

event: step.delta
data: {\"index\":1,\"delta\":{\"text\":\"Hello\",\"type\":\"text\"},\"event_type\":\"step.delta\"}

event: interaction.completed
data: {\"interaction\":{\"id\":\"v1_abc\",\"status\":\"completed\",\"usage\":{\"total_input_tokens\":8,\"total_output_tokens\":2}},\"event_type\":\"interaction.completed\"}
";
    let mut assembler = GeminiStreamAssembler::default();
    let mut chunks = Vec::new();
    for line in sse.lines() {
        chunks.extend(assembler.push_line(line).expect("line"));
    }
    chunks.extend(assembler.finish().expect("finish"));
    assert!(matches!(
        &chunks[0],
        CompletionChunk::Delta { content, .. } if content == "Hello"
    ));
    match chunks.last() {
        Some(CompletionChunk::Done {
            content,
            input_tokens,
            output_tokens,
            response_id,
            ..
        }) => {
            assert_eq!(content, "Hello");
            assert_eq!(*input_tokens, 8);
            assert_eq!(*output_tokens, 2);
            assert_eq!(response_id.as_deref(), Some("v1_abc"));
        }
        other => panic!("expected done chunk, got {other:?}"),
    }
}

#[test]
fn function_call_interaction_keeps_thought_signature_for_the_next_turn() {
    let mut assembler = GeminiStreamAssembler::default();
    assembler
            .ingest_interaction(&serde_json::json!({
                "id": "v1_call",
                "status": "requires_action",
                "usage": {"total_input_tokens": 10, "total_output_tokens": 4},
                "steps": [
                    {"type": "thought", "signature": "sig-2"},
                    {"type": "function_call", "id": "call_abc", "name": "get_weather", "arguments": {"location": "London"}}
                ]
            }))
            .expect("ingest");
    let chunks = assembler.finish().expect("finish");
    match chunks.last() {
        Some(CompletionChunk::ToolCalls {
            tool_calls,
            reasoning,
            response_id,
            ..
        }) => {
            assert_eq!(tool_calls[0].id, "call_abc");
            assert_eq!(tool_calls[0].function.name, "get_weather");
            assert_eq!(tool_calls[0].function.arguments, r#"{"location":"London"}"#);
            assert!(reasoning.as_deref().unwrap_or("").contains("sig-2"));
            assert_eq!(response_id.as_deref(), Some("v1_call"));
        }
        other => panic!("expected tool calls, got {other:?}"),
    }
}

#[test]
fn function_call_string_arguments_are_unwrapped_for_the_tool_executor() {
    let mut assembler = GeminiStreamAssembler::default();
    assembler
        .ingest_interaction(&serde_json::json!({
            "id": "v1_bash",
            "status": "requires_action",
            "steps": [{
                "type": "function_call",
                "id": "call_bash",
                "name": "bash",
                "arguments": "{\"command\":\"git status\",\"cwd\":\"/mnt/e/sepiq2026\"}"
            }]
        }))
        .expect("ingest");
    let chunks = assembler.finish().expect("finish");
    match chunks.last() {
        Some(CompletionChunk::ToolCalls { tool_calls, .. }) => {
            assert_eq!(
                tool_calls[0].function.arguments,
                r#"{"command":"git status","cwd":"/mnt/e/sepiq2026"}"#
            );
        }
        other => panic!("expected tool calls, got {other:?}"),
    }
}
