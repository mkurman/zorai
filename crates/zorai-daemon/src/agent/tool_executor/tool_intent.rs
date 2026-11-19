use super::*;

pub(crate) const TOOL_INTENT_ARG: &str = "intent";
const TOOL_INTENT_DESCRIPTION: &str =
    "Optional. Extremely short UI status, max 6 words, e.g. \"Running unit tests\".";

pub(crate) fn with_tool_intent_parameter(tools: &mut [ToolDefinition]) {
    for tool in tools {
        let Some(parameters) = tool.function.parameters.as_object_mut() else {
            continue;
        };
        let properties = parameters
            .entry("properties")
            .or_insert_with(|| serde_json::json!({}));
        let Some(properties) = properties.as_object_mut() else {
            continue;
        };
        properties.entry(TOOL_INTENT_ARG).or_insert_with(
            || serde_json::json!({ "type": "string", "description": TOOL_INTENT_DESCRIPTION }),
        );
    }
}

pub(crate) fn without_tool_intent_argument(tool_call: &ToolCall) -> Option<ToolCall> {
    let mut arguments =
        serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments).ok()?;
    arguments.as_object_mut()?.remove(TOOL_INTENT_ARG)?;
    let mut stripped = tool_call.clone();
    stripped.function.arguments = arguments.to_string();
    Some(stripped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(parameters: serde_json::Value) -> ToolDefinition {
        ToolDefinition {
            tool_type: "function".to_string(),
            function: ToolFunctionDef {
                name: "demo".to_string(),
                description: "demo".to_string(),
                parameters,
            },
        }
    }

    fn call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call-1".to_string(),
            function: crate::agent::types::ToolFunction {
                name: "demo".to_string(),
                arguments: arguments.to_string(),
            },
            weles_review: None,
        }
    }

    #[test]
    fn intent_is_offered_as_optional_so_models_never_have_to_send_it() {
        let mut tools = vec![
            tool(serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            })),
            tool(serde_json::json!({ "type": "object" })),
        ];
        with_tool_intent_parameter(&mut tools);
        for tool in &tools {
            let parameters = &tool.function.parameters;
            assert_eq!(parameters["properties"][TOOL_INTENT_ARG]["type"], "string");
            let required = parameters["required"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert!(!required.iter().any(|value| value == TOOL_INTENT_ARG));
        }
        assert_eq!(
            tools[0].function.parameters["required"],
            serde_json::json!(["path"])
        );
    }

    #[test]
    fn a_tool_that_defines_its_own_intent_keeps_its_schema() {
        let own = serde_json::json!({ "type": "integer" });
        let mut tools = vec![tool(serde_json::json!({
            "type": "object",
            "properties": { "intent": own.clone() }
        }))];
        with_tool_intent_parameter(&mut tools);
        assert_eq!(tools[0].function.parameters["properties"]["intent"], own);
    }

    #[test]
    fn intent_is_removed_before_execution_so_tools_only_see_their_own_arguments() {
        let stripped = without_tool_intent_argument(&call(
            r#"{"path":"src/lib.rs","intent":"Reading the library"}"#,
        ))
        .expect("intent should be stripped");
        let arguments: serde_json::Value =
            serde_json::from_str(&stripped.function.arguments).unwrap();
        assert_eq!(arguments, serde_json::json!({ "path": "src/lib.rs" }));
    }

    #[test]
    fn calls_without_intent_are_left_untouched() {
        assert!(without_tool_intent_argument(&call(r#"{"path":"a"}"#)).is_none());
        assert!(without_tool_intent_argument(&call("not json")).is_none());
    }
}
