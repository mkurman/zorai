use super::*;

fn config_bool(json: &serde_json::Value, camel_key: &str, snake_key: &str, default: bool) -> bool {
    json.get(snake_key)
        .and_then(|value| value.as_bool())
        .or_else(|| json.get(camel_key).and_then(|value| value.as_bool()))
        .unwrap_or(default)
}

fn config_u32(json: &serde_json::Value, camel_key: &str, snake_key: &str, default: u32) -> u32 {
    json.get(snake_key)
        .and_then(|value| value.as_u64())
        .or_else(|| json.get(camel_key).and_then(|value| value.as_u64()))
        .map(|value| value as u32)
        .unwrap_or(default)
}

fn config_string(json: &serde_json::Value, camel_key: &str, snake_key: &str) -> String {
    json.get(snake_key)
        .and_then(|value| value.as_str())
        .or_else(|| json.get(camel_key).and_then(|value| value.as_str()))
        .unwrap_or("")
        .to_string()
}

impl TuiModel {
    pub(super) fn apply_general_config_json(&mut self, json: &serde_json::Value) {
        self.config.tool_bash = json
            .get("tools")
            .and_then(|v| v.get("bash"))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| config_bool(json, "enableBashTool", "enable_bash_tool", true));
        self.config.tool_file_ops = json
            .get("tools")
            .and_then(|v| v.get("file_operations"))
            .and_then(|v| v.as_bool())
            .unwrap_or(self.config.tool_file_ops);
        self.config.tool_web_search = json
            .get("tools")
            .and_then(|v| v.get(zorai_protocol::tool_names::WEB_SEARCH))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                config_bool(json, "enableWebSearchTool", "enable_web_search_tool", false)
            });
        self.config.tool_web_browse = json
            .get("tools")
            .and_then(|v| v.get("web_browse"))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                config_bool(
                    json,
                    "enableWebBrowsingTool",
                    "enable_web_browsing_tool",
                    false,
                )
            });
        self.config.tool_vision = json
            .get("tools")
            .and_then(|v| v.get("vision"))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| config_bool(json, "enableVisionTool", "enable_vision_tool", false));
        self.config.tool_system_info = json
            .get("tools")
            .and_then(|v| v.get("system_info"))
            .and_then(|v| v.as_bool())
            .unwrap_or(self.config.tool_system_info);
        self.config.tool_gateway = json
            .get("tools")
            .and_then(|v| v.get("gateway_messaging"))
            .and_then(|v| v.as_bool())
            .unwrap_or(self.config.gateway_enabled);

        self.config.reasoning_effort = {
            let effort = config_string(json, "reasoning_effort", "reasoning_effort");
            if effort.is_empty() {
                self.config.reasoning_effort.clone()
            } else {
                effort
            }
        };
        self.config.claude_permission_mode =
            config_string(json, "claude_permission_mode", "claude_permission_mode");

        self.config.search_provider = {
            let provider = config_string(json, "search_provider", "search_provider");
            if provider.is_empty() {
                self.config.search_provider.clone()
            } else {
                provider
            }
        };
        self.config.duckduckgo_region = {
            let region = config_string(json, "duckduckgo_region", "duckduckgo_region");
            if region.is_empty() {
                "us-en".to_string()
            } else {
                region
            }
        };
        self.config.duckduckgo_safe_search = {
            let safe_search =
                config_string(json, "duckduckgo_safe_search", "duckduckgo_safe_search");
            if safe_search.is_empty() {
                "moderate".to_string()
            } else {
                safe_search
            }
        };
        self.config.firecrawl_api_key =
            config_string(json, "firecrawl_api_key", "firecrawl_api_key");
        self.config.exa_api_key = config_string(json, "exa_api_key", "exa_api_key");
        self.config.tavily_api_key = config_string(json, "tavily_api_key", "tavily_api_key");
        self.config.search_max_results =
            config_u32(json, "search_max_results", "search_max_results", 8);
        self.config.search_timeout_secs =
            config_u32(json, "search_timeout_secs", "search_timeout_secs", 20);
        self.config.browse_provider = {
            let provider = config_string(json, "browse_provider", "browse_provider");
            if provider.is_empty() {
                "auto".to_string()
            } else {
                provider
            }
        };
        self.config.enable_streaming =
            config_bool(json, "enable_streaming", "enable_streaming", true);
        self.config.enable_conversation_memory = config_bool(
            json,
            "enable_conversation_memory",
            "enable_conversation_memory",
            true,
        );
        self.config.enable_honcho_memory =
            config_bool(json, "enable_honcho_memory", "enable_honcho_memory", false);
        self.config.honcho_api_key = config_string(json, "honcho_api_key", "honcho_api_key");
        self.config.honcho_base_url = config_string(json, "honcho_base_url", "honcho_base_url");
        self.config.honcho_workspace_id = {
            let ws = config_string(json, "honcho_workspace_id", "honcho_workspace_id");
            if ws.is_empty() {
                "zorai".to_string()
            } else {
                ws
            }
        };
        let mlflow = json.get("mlflow_tracing");
        self.config.mlflow_enabled = mlflow
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.mlflow_tracking_uri = mlflow
            .and_then(|value| value.get("tracking_uri"))
            .and_then(|value| value.as_str())
            .unwrap_or("http://127.0.0.1:5000")
            .to_string();
        self.config.mlflow_experiment_name = mlflow
            .and_then(|value| value.get("experiment_name"))
            .and_then(|value| value.as_str())
            .unwrap_or("zorai-conversations")
            .to_string();
        self.config.mlflow_experiment_id = mlflow
            .and_then(|value| value.get("experiment_id"))
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        self.config.mlflow_capture_mode = mlflow
            .and_then(|value| value.get("capture_mode"))
            .and_then(|value| value.as_str())
            .unwrap_or("guarded")
            .to_string();
        let mlflow_scopes = mlflow.and_then(|value| value.get("scopes"));
        self.config.mlflow_visible_operator = mlflow_scopes
            .and_then(|value| value.get("visible_operator"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.mlflow_gateway = mlflow_scopes
            .and_then(|value| value.get("gateway"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.mlflow_goal_task = mlflow_scopes
            .and_then(|value| value.get("goal_task"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.mlflow_subagent = mlflow_scopes
            .and_then(|value| value.get("subagent"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.mlflow_concierge = mlflow_scopes
            .and_then(|value| value.get("concierge"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.mlflow_heartbeat_autonomous = mlflow_scopes
            .and_then(|value| value.get("heartbeat_autonomous"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.anticipatory_enabled = json
            .get("anticipatory")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.anticipatory_morning_brief = json
            .get("anticipatory")
            .and_then(|value| value.get("morning_brief"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.anticipatory_predictive_hydration = json
            .get("anticipatory")
            .and_then(|value| value.get("predictive_hydration"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.anticipatory_stuck_detection = json
            .get("anticipatory")
            .and_then(|value| value.get("stuck_detection"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_enabled = json
            .get("operator_model")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_allow_message_statistics = json
            .get("operator_model")
            .and_then(|value| value.get("allow_message_statistics"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_allow_approval_learning = json
            .get("operator_model")
            .and_then(|value| value.get("allow_approval_learning"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_allow_attention_tracking = json
            .get("operator_model")
            .and_then(|value| value.get("allow_attention_tracking"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_allow_implicit_feedback = json
            .get("operator_model")
            .and_then(|value| value.get("allow_implicit_feedback"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.collaboration_enabled = json
            .get("collaboration")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.compliance_mode = json
            .get("compliance")
            .and_then(|value| value.get("mode"))
            .and_then(|value| value.as_str())
            .map(normalize_compliance_mode)
            .unwrap_or_else(|| "standard".to_string());
        self.config.compliance_retention_days = json
            .get("compliance")
            .and_then(|value| value.get("retention_days"))
            .and_then(|value| value.as_u64())
            .unwrap_or(30) as u32;
        self.config.compliance_sign_all_events = json
            .get("compliance")
            .and_then(|value| value.get("sign_all_events"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.tool_synthesis_enabled = json
            .get("tool_synthesis")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.tool_synthesis_require_activation = json
            .get("tool_synthesis")
            .and_then(|value| value.get("require_activation"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.tool_synthesis_max_generated_tools = json
            .get("tool_synthesis")
            .and_then(|value| value.get("max_generated_tools"))
            .and_then(|value| value.as_u64())
            .unwrap_or(24) as u32;
        self.config.managed_sandbox_enabled = json
            .get("managed_execution")
            .and_then(|value| value.get("sandbox_enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.managed_security_level = json
            .get("managed_execution")
            .and_then(|value| value.get("security_level"))
            .and_then(|value| value.as_str())
            .map(normalize_managed_security_level)
            .unwrap_or_else(|| "lowest".to_string());

        self.config.auto_compact_context =
            config_bool(json, "auto_compact_context", "auto_compact_context", true);
        self.config.max_context_messages =
            config_u32(json, "max_context_messages", "max_context_messages", 100);
        self.config.tui_chat_history_page_size = config_u32(
            json,
            "tui_chat_history_page_size",
            "tui_chat_history_page_size",
            25,
        )
        .clamp(20, 500);
        self.config.participant_observer_restore_window_hours = config_u32(
            json,
            "participant_observer_restore_window_hours",
            "participant_observer_restore_window_hours",
            24,
        )
        .clamp(0, 24 * 30);
        self.config.max_tool_loops = config_u32(json, "max_tool_loops", "max_tool_loops", 25);
        self.config.max_retries = config_u32(json, "max_retries", "max_retries", 3);
        self.config.auto_refresh_interval_secs = config_u32(
            json,
            "auto_refresh_interval_secs",
            "auto_refresh_interval_secs",
            300,
        )
        .clamp(0, 86_400);
        self.config.retry_delay_ms = config_u32(json, "retry_delay_ms", "retry_delay_ms", 5_000);
        self.config.message_loop_delay_ms =
            config_u32(json, "message_loop_delay_ms", "message_loop_delay_ms", 100);
        self.config.tool_call_delay_ms =
            config_u32(json, "tool_call_delay_ms", "tool_call_delay_ms", 100);
        self.config.llm_stream_chunk_timeout_secs = config_u32(
            json,
            "llm_stream_chunk_timeout_secs",
            "llm_stream_chunk_timeout_secs",
            300,
        );
        self.config.auto_retry = config_bool(json, "auto_retry", "auto_retry", true);
        self.config.compact_threshold_pct =
            config_u32(json, "compact_threshold_pct", "compact_threshold_pct", 80);
        self.config.keep_recent_on_compact =
            config_u32(json, "keep_recent_on_compact", "keep_recent_on_compact", 10);
        self.config.bash_timeout_secs =
            config_u32(json, "bash_timeout_seconds", "bash_timeout_seconds", 30);
        self.config.weles_max_concurrent_reviews = json
            .get("builtin_sub_agents")
            .and_then(|value| value.get("weles"))
            .and_then(|value| value.get("max_concurrent_reviews"))
            .and_then(|value| value.as_u64())
            .map(|value| value.clamp(1, 16) as u32)
            .unwrap_or(2);
    }
}
