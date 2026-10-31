use super::super::*;
use super::{
    normalize_compliance_mode, normalize_provider_auth_source, normalize_provider_transport,
};

impl TuiModel {
    pub(in crate::app) fn build_config_patch_value(&mut self) -> serde_json::Value {
        let mut patch = self
            .config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        let previous_main_provider = patch
            .get("provider")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let previous_main_model = patch
            .get("model")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                previous_main_provider.as_deref().and_then(|provider_id| {
                    patch
                        .get("providers")
                        .and_then(|providers| providers.get(provider_id))
                        .or_else(|| patch.get(provider_id))
                        .and_then(|provider| provider.get("model"))
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                })
            })
            .map(str::to_string);
        let main_provider_or_model_changed_after_setup =
            previous_main_provider.as_deref().is_some_and(|provider| {
                provider != self.config.provider
                    || previous_main_model.as_deref() != Some(&self.config.model)
            }) && previous_main_model.is_some();

        patch["provider"] = serde_json::Value::String(self.config.provider.clone());
        patch["providers"] = serde_json::Value::Object(self.all_provider_wire_config_values());
        patch[&self.config.provider] = self.provider_wire_config_value(&self.config.provider);
        if let Some(providers) = patch["providers"].as_object_mut() {
            for value in providers.values_mut() {
                if let Some(map) = value.as_object_mut() {
                    map.remove("api_key");
                }
            }
        }
        if let Some(map) = patch[&self.config.provider].as_object_mut() {
            map.remove("api_key");
        }
        patch["base_url"] = serde_json::Value::String(self.config.base_url.clone());
        patch["model"] = serde_json::Value::String(self.config.model.clone());
        patch["reasoning_effort"] = serde_json::Value::String(self.config.reasoning_effort.clone());
        patch["claude_permission_mode"] = if self.config.claude_permission_mode.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(self.config.claude_permission_mode.clone())
        };
        patch["context_window_tokens"] =
            serde_json::Value::from(self.effective_current_context_window() as u64);
        patch["search_provider"] = serde_json::Value::String(self.config.search_provider.clone());
        patch["duckduckgo_region"] =
            serde_json::Value::String(self.config.duckduckgo_region.clone());
        patch["duckduckgo_safe_search"] =
            serde_json::Value::String(self.config.duckduckgo_safe_search.clone());
        patch["firecrawl_api_key"] =
            serde_json::Value::String(self.config.firecrawl_api_key.clone());
        patch["exa_api_key"] = serde_json::Value::String(self.config.exa_api_key.clone());
        patch["tavily_api_key"] = serde_json::Value::String(self.config.tavily_api_key.clone());
        patch["search_max_results"] = serde_json::Value::from(self.config.search_max_results);
        patch["search_timeout_secs"] = serde_json::Value::from(self.config.search_timeout_secs);
        patch["browse_provider"] = serde_json::Value::String(self.config.browse_provider.clone());
        patch["enable_streaming"] = serde_json::Value::Bool(self.config.enable_streaming);
        patch["enable_conversation_memory"] =
            serde_json::Value::Bool(self.config.enable_conversation_memory);
        patch["enable_honcho_memory"] = serde_json::Value::Bool(self.config.enable_honcho_memory);
        patch["honcho_api_key"] = serde_json::Value::String(self.config.honcho_api_key.clone());
        patch["honcho_base_url"] = serde_json::Value::String(self.config.honcho_base_url.clone());
        patch["honcho_workspace_id"] =
            serde_json::Value::String(self.config.honcho_workspace_id.clone());
        patch["mlflow_tracing"] = serde_json::json!({
            "enabled": self.config.mlflow_enabled,
            "tracking_uri": self.config.mlflow_tracking_uri,
            "experiment_name": self.config.mlflow_experiment_name,
            "experiment_id": if self.config.mlflow_experiment_id.trim().is_empty() { serde_json::Value::Null } else { serde_json::Value::String(self.config.mlflow_experiment_id.clone()) },
            "capture_mode": self.config.mlflow_capture_mode,
            "scopes": {
                "visible_operator": self.config.mlflow_visible_operator,
                "gateway": self.config.mlflow_gateway,
                "goal_task": self.config.mlflow_goal_task,
                "subagent": self.config.mlflow_subagent,
                "concierge": self.config.mlflow_concierge,
                "heartbeat_autonomous": self.config.mlflow_heartbeat_autonomous,
            }
        });
        patch["tool_gateway"] = serde_json::Value::Bool(self.config.tool_gateway);
        patch["tools"] = serde_json::json!({
            "bash": self.config.tool_bash,
            "file_operations": self.config.tool_file_ops,
            "web_search": self.config.tool_web_search,
            "web_browse": self.config.tool_web_browse,
            "vision": self.config.tool_vision,
            "system_info": self.config.tool_system_info,
            "gateway_messaging": self.config.tool_gateway,
        });
        patch["anticipatory"] = serde_json::json!({
            "enabled": self.config.anticipatory_enabled,
            "morning_brief": self.config.anticipatory_morning_brief,
            "predictive_hydration": self.config.anticipatory_predictive_hydration,
            "stuck_detection": self.config.anticipatory_stuck_detection,
        });
        patch["collaboration"] =
            serde_json::json!({ "enabled": self.config.collaboration_enabled });
        patch["operator_model"] = serde_json::json!({
            "enabled": self.config.operator_model_enabled,
            "allow_message_statistics": self.config.operator_model_allow_message_statistics,
            "allow_approval_learning": self.config.operator_model_allow_approval_learning,
            "allow_attention_tracking": self.config.operator_model_allow_attention_tracking,
            "allow_implicit_feedback": self.config.operator_model_allow_implicit_feedback,
        });
        patch["compliance"] = serde_json::json!({
            "mode": normalize_compliance_mode(&self.config.compliance_mode),
            "retention_days": self.config.compliance_retention_days,
            "sign_all_events": self.config.compliance_sign_all_events,
        });
        patch["tool_synthesis"] = serde_json::json!({
            "enabled": self.config.tool_synthesis_enabled,
            "require_activation": self.config.tool_synthesis_require_activation,
            "max_generated_tools": self.config.tool_synthesis_max_generated_tools,
        });
        patch["managed_execution"] = serde_json::json!({
            "sandbox_enabled": self.config.managed_sandbox_enabled,
            "security_level": self.config.managed_security_level,
        });
        patch["gateway"] = serde_json::json!({
            "enabled": self.config.gateway_enabled,
            "command_prefix": self.config.gateway_prefix,
            "default_agent": self.config.gateway_default_agent,
            "slack_default_agent": self.config.slack_default_agent,
            "discord_default_agent": self.config.discord_default_agent,
            "telegram_default_agent": self.config.telegram_default_agent,
            "whatsapp_default_agent": self.config.whatsapp_default_agent,
            "slack_token": self.config.slack_token,
            "slack_channel_filter": self.config.slack_channel_filter,
            "telegram_token": self.config.telegram_token,
            "telegram_allowed_chats": self.config.telegram_allowed_chats,
            "discord_token": self.config.discord_token,
            "discord_channel_filter": self.config.discord_channel_filter,
            "discord_allowed_users": self.config.discord_allowed_users,
            "whatsapp_token": self.config.whatsapp_token,
            "whatsapp_phone_id": self.config.whatsapp_phone_id,
            "whatsapp_allowed_contacts": self.config.whatsapp_allowed_contacts,
        });
        patch["auto_compact_context"] = serde_json::Value::Bool(self.config.auto_compact_context);
        patch["max_context_messages"] = serde_json::Value::from(self.config.max_context_messages);
        patch["tui_chat_history_page_size"] =
            serde_json::Value::from(self.config.tui_chat_history_page_size);
        patch["participant_observer_restore_window_hours"] =
            serde_json::Value::from(self.config.participant_observer_restore_window_hours);
        patch["max_tool_loops"] = serde_json::Value::from(self.config.max_tool_loops);
        patch["max_subagent_budget_extensions"] =
            serde_json::Value::from(self.config.max_subagent_budget_extensions);
        patch["max_retries"] = serde_json::Value::from(self.config.max_retries);
        patch["auto_refresh_interval_secs"] =
            serde_json::Value::from(self.config.auto_refresh_interval_secs);
        patch["retry_delay_ms"] = serde_json::Value::from(self.config.retry_delay_ms);
        patch["message_loop_delay_ms"] = serde_json::Value::from(self.config.message_loop_delay_ms);
        patch["tool_call_delay_ms"] = serde_json::Value::from(self.config.tool_call_delay_ms);
        patch["llm_stream_chunk_timeout_secs"] =
            serde_json::Value::from(self.config.llm_stream_chunk_timeout_secs);
        patch["auto_retry"] = serde_json::Value::Bool(self.config.auto_retry);
        patch["compact_threshold_pct"] = serde_json::Value::from(self.config.compact_threshold_pct);
        patch["keep_recent_on_compact"] =
            serde_json::Value::from(self.config.keep_recent_on_compact);
        patch["bash_timeout_seconds"] = serde_json::Value::from(self.config.bash_timeout_secs);
        if patch.get("builtin_sub_agents").is_none() {
            patch["builtin_sub_agents"] = serde_json::json!({});
        }
        if patch["builtin_sub_agents"].get("weles").is_none() {
            patch["builtin_sub_agents"]["weles"] = serde_json::json!({});
        }
        if main_provider_or_model_changed_after_setup {
            if let (Some(provider_id), Some(model_id)) = (
                previous_main_provider.as_deref(),
                previous_main_model.as_deref(),
            ) {
                if patch.get("concierge").is_none() {
                    patch["concierge"] = serde_json::json!({});
                }
                let concierge_provider_missing = patch["concierge"]
                    .get("provider")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .is_none_or(str::is_empty);
                let concierge_model_missing = patch["concierge"]
                    .get("model")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .is_none_or(str::is_empty);
                if concierge_provider_missing && concierge_model_missing {
                    patch["concierge"]["provider"] =
                        serde_json::Value::String(provider_id.to_string());
                    patch["concierge"]["model"] = serde_json::Value::String(model_id.to_string());
                }

                let weles_provider_missing = patch["builtin_sub_agents"]["weles"]
                    .get("provider")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .is_none_or(str::is_empty);
                let weles_model_missing = patch["builtin_sub_agents"]["weles"]
                    .get("model")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .is_none_or(str::is_empty);
                if weles_provider_missing && weles_model_missing {
                    patch["builtin_sub_agents"]["weles"]["provider"] =
                        serde_json::Value::String(provider_id.to_string());
                    patch["builtin_sub_agents"]["weles"]["model"] =
                        serde_json::Value::String(model_id.to_string());
                }
            }
        }
        patch["builtin_sub_agents"]["weles"]["max_concurrent_reviews"] =
            serde_json::Value::from(self.config.weles_max_concurrent_reviews);
        patch["compaction"] = serde_json::json!({
            "strategy": self.config.compaction_strategy,
            "weles": {
                "provider": self.config.compaction_weles_provider,
                "model": self.config.compaction_weles_model,
                "reasoning_effort": self.config.compaction_weles_reasoning_effort,
            },
            // `weles.api_transport` is optional (inherit when unset); injected below.
            "custom_model": {
                "provider": self.config.compaction_custom_provider,
                "base_url": self.config.compaction_custom_base_url,
                "model": self.config.compaction_custom_model,
                "api_key": self.config.compaction_custom_api_key,
                "assistant_id": self.config.compaction_custom_assistant_id,
                "auth_source": normalize_provider_auth_source(
                    &self.config.compaction_custom_provider,
                    &self.config.compaction_custom_auth_source,
                ),
                "api_transport": normalize_provider_transport(
                    &self.config.compaction_custom_provider,
                    &self.config.compaction_custom_api_transport,
                ),
                "reasoning_effort": self.config.compaction_custom_reasoning_effort,
                "context_window_tokens": self.config.compaction_custom_context_window_tokens,
            },
        });
        if self.config.compaction_weles_api_transport.trim().is_empty() {
            patch["compaction"]["weles"]["api_transport"] = serde_json::Value::Null;
        } else {
            patch["compaction"]["weles"]["api_transport"] =
                serde_json::Value::from(normalize_provider_transport(
                    &self.config.compaction_weles_provider,
                    &self.config.compaction_weles_api_transport,
                ));
        }
        patch["snapshot_retention"] = serde_json::json!({
            "max_snapshots": self.config.snapshot_max_count,
            "max_total_size_mb": self.config.snapshot_max_size_mb,
            "auto_cleanup": self.config.snapshot_auto_cleanup,
        });

        patch
    }
}
