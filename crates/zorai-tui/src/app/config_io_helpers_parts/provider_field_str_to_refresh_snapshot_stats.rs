use super::super::*;
use super::{
    normalize_provider_auth_source, normalize_provider_transport, openrouter_provider_list_value,
    split_openrouter_provider_list,
};
use crate::providers;
use zorai_shared::providers::{PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENROUTER};

impl TuiModel {
    pub(in crate::app) fn provider_field_str<'a>(
        provider_value: &'a serde_json::Value,
        camel_case: &str,
        snake_case: &str,
    ) -> Option<&'a str> {
        provider_value
            .get(camel_case)
            .and_then(|value| value.as_str())
            .or_else(|| {
                provider_value
                    .get(snake_case)
                    .and_then(|value| value.as_str())
            })
    }

    pub(in crate::app) fn provider_field_u64(
        provider_value: &serde_json::Value,
        camel_case: &str,
        snake_case: &str,
    ) -> Option<u64> {
        provider_value
            .get(camel_case)
            .and_then(|value| value.as_u64())
            .or_else(|| {
                provider_value
                    .get(snake_case)
                    .and_then(|value| value.as_u64())
            })
    }

    pub(in crate::app) fn refresh_openai_auth_status(&mut self) {
        self.send_daemon_command(DaemonCommand::GetOpenAICodexAuthStatus);
    }

    pub(in crate::app) fn effective_context_window_for_provider_value(
        provider_id: &str,
        provider_value: &serde_json::Value,
    ) -> u32 {
        let model = provider_value
            .get("model")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let auth_source = provider_value
            .get("auth_source")
            .and_then(|value| value.as_str())
            .unwrap_or(providers::default_auth_source_for(provider_id));
        let custom_model_name = provider_value
            .get("custom_model_name")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if let Some(context_window) = providers::resolve_context_window_for_provider_auth(
            provider_id,
            auth_source,
            model,
            custom_model_name,
        ) {
            return context_window;
        }

        if providers::model_uses_context_window_override(
            provider_id,
            auth_source,
            model,
            custom_model_name,
        ) {
            return provider_value
                .get("context_window_tokens")
                .and_then(|value| value.as_u64())
                .map(|value| value.max(1000) as u32)
                .unwrap_or(providers::default_custom_model_context_window());
        }

        providers::known_context_window_for(provider_id, model).unwrap_or(256_000)
    }

    pub(in crate::app) fn effective_current_context_window(&self) -> u32 {
        if let Some(context_window) = providers::resolve_context_window_for_provider_auth(
            &self.config.provider,
            &self.config.auth_source,
            &self.config.model,
            &self.config.custom_model_name,
        ) {
            context_window
        } else if providers::model_uses_context_window_override(
            &self.config.provider,
            &self.config.auth_source,
            &self.config.model,
            &self.config.custom_model_name,
        ) {
            self.config.custom_context_window_tokens.unwrap_or(256_000)
        } else {
            providers::known_context_window_for(&self.config.provider, &self.config.model)
                .unwrap_or(256_000)
        }
    }

    pub(in crate::app) fn provider_config_value(&self, provider_id: &str) -> serde_json::Value {
        if provider_id == self.config.provider {
            let mut value = serde_json::json!({
                "base_url": &self.config.base_url,
                "model": &self.config.model,
                "custom_model_name": &self.config.custom_model_name,
                "api_key": &self.config.api_key,
                "assistant_id": &self.config.assistant_id,
                "api_transport": &self.config.api_transport,
                "auth_source": &self.config.auth_source,
                "context_window_tokens": self.config.custom_context_window_tokens,
            });
            if provider_id == PROVIDER_ID_OPENROUTER {
                value["openrouter_provider_order"] = serde_json::json!(
                    split_openrouter_provider_list(&self.config.openrouter_provider_order)
                );
                value["openrouter_provider_ignore"] = serde_json::json!(
                    split_openrouter_provider_list(&self.config.openrouter_provider_ignore)
                );
                value["openrouter_allow_fallbacks"] =
                    serde_json::Value::Bool(self.config.openrouter_allow_fallbacks);
                value["openrouter_response_cache_enabled"] =
                    serde_json::Value::Bool(self.config.openrouter_response_cache_enabled);
            }
            if provider_id == zorai_shared::providers::PROVIDER_ID_HUGGINGFACE {
                value["huggingface_provider"] =
                    serde_json::Value::String(self.config.huggingface_provider.trim().to_string());
            }
            return value;
        }

        if let Some(existing) = self
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| {
                raw.get("providers")
                    .and_then(|providers| providers.get(provider_id))
                    .or_else(|| raw.get(provider_id))
            })
            .cloned()
        {
            return existing;
        }

        let def = providers::find_by_id(provider_id);
        let mut value = serde_json::json!({
            "base_url": def.map(|entry| entry.default_base_url).unwrap_or(""),
            "model": def.map(|entry| entry.default_model).unwrap_or(""),
            "custom_model_name": "",
            "api_key": "",
            "assistant_id": "",
            "api_transport": providers::default_transport_for(provider_id),
            "auth_source": providers::default_auth_source_for(provider_id),
            "context_window_tokens": if provider_id == PROVIDER_ID_CUSTOM { serde_json::Value::from(256_000u32) } else { serde_json::Value::Null },
        });
        if provider_id == PROVIDER_ID_OPENROUTER {
            value["openrouter_provider_order"] = serde_json::json!([]);
            value["openrouter_provider_ignore"] = serde_json::json!([]);
            value["openrouter_allow_fallbacks"] = serde_json::Value::Bool(true);
            value["openrouter_response_cache_enabled"] = serde_json::Value::Bool(false);
        }
        if provider_id == zorai_shared::providers::PROVIDER_ID_HUGGINGFACE {
            value["huggingface_provider"] = serde_json::Value::String(String::new());
        }
        value
    }

    pub(in crate::app) fn provider_wire_config_value(
        &self,
        provider_id: &str,
    ) -> serde_json::Value {
        let ui_value = self.provider_config_value(provider_id);
        let auth_source = normalize_provider_auth_source(
            provider_id,
            Self::provider_field_str(&ui_value, "auth_source", "auth_source")
                .unwrap_or(providers::default_auth_source_for(provider_id)),
        );
        let api_transport = normalize_provider_transport(
            provider_id,
            Self::provider_field_str(&ui_value, "api_transport", "api_transport")
                .unwrap_or(providers::default_transport_for(provider_id)),
        );
        let model = Self::provider_field_str(&ui_value, "model", "model").unwrap_or("");
        let custom_model_name =
            Self::provider_field_str(&ui_value, "custom_model_name", "custom_model_name")
                .unwrap_or("");
        let resolved_context_window = providers::resolve_context_window_for_provider_auth(
            provider_id,
            &auth_source,
            model,
            custom_model_name,
        );
        let reasoning_effort = if provider_id == self.config.provider {
            self.config.reasoning_effort.as_str()
        } else {
            Self::provider_field_str(&ui_value, "reasoning_effort", "reasoning_effort")
                .unwrap_or(self.config.reasoning_effort.as_str())
        };
        let mut value = serde_json::json!({
            "base_url": Self::provider_field_str(&ui_value, "base_url", "base_url").unwrap_or(""),
            "model": model,
            "custom_model_name": custom_model_name,
            "api_key": Self::provider_field_str(&ui_value, "api_key", "api_key").unwrap_or(""),
            "assistant_id": Self::provider_field_str(&ui_value, "assistant_id", "assistant_id").unwrap_or(""),
            "auth_source": auth_source,
            "api_transport": api_transport,
            "reasoning_effort": reasoning_effort,
            "context_window_tokens": if let Some(context_window) = resolved_context_window {
                context_window as u64
            } else if providers::model_uses_context_window_override(
                provider_id,
                &auth_source,
                model,
                custom_model_name,
            ) {
                Self::provider_field_u64(&ui_value, "context_window_tokens", "context_window_tokens")
                    .unwrap_or(providers::default_custom_model_context_window() as u64)
            } else {
                providers::known_context_window_for(
                    provider_id,
                    model,
                )
                .unwrap_or(256_000) as u64
            },
        });
        if provider_id == PROVIDER_ID_OPENROUTER {
            let order_value =
                openrouter_provider_list_value(&ui_value, "openrouter_provider_order");
            let order_fallback = if self.config.provider == PROVIDER_ID_OPENROUTER {
                self.config.openrouter_provider_order.as_str()
            } else {
                ""
            };
            let order_source = if order_value.is_empty() {
                order_fallback
            } else {
                order_value.as_str()
            };
            let ignore_value =
                openrouter_provider_list_value(&ui_value, "openrouter_provider_ignore");
            let ignore_fallback = if self.config.provider == PROVIDER_ID_OPENROUTER {
                self.config.openrouter_provider_ignore.as_str()
            } else {
                ""
            };
            let ignore_source = if ignore_value.is_empty() {
                ignore_fallback
            } else {
                ignore_value.as_str()
            };
            value["openrouter_provider_order"] =
                serde_json::json!(split_openrouter_provider_list(order_source));
            value["openrouter_provider_ignore"] =
                serde_json::json!(split_openrouter_provider_list(ignore_source));
            value["openrouter_allow_fallbacks"] = ui_value
                .get("openrouter_allow_fallbacks")
                .and_then(|value| value.as_bool())
                .map(serde_json::Value::Bool)
                .unwrap_or(serde_json::Value::Bool(
                    if self.config.provider == PROVIDER_ID_OPENROUTER {
                        self.config.openrouter_allow_fallbacks
                    } else {
                        true
                    },
                ));
            value["openrouter_response_cache_enabled"] = ui_value
                .get("openrouter_response_cache_enabled")
                .and_then(|value| value.as_bool())
                .map(serde_json::Value::Bool)
                .unwrap_or(serde_json::Value::Bool(
                    if self.config.provider == PROVIDER_ID_OPENROUTER {
                        self.config.openrouter_response_cache_enabled
                    } else {
                        false
                    },
                ));
        }
        if provider_id == zorai_shared::providers::PROVIDER_ID_HUGGINGFACE {
            let huggingface_provider =
                Self::provider_field_str(&ui_value, "huggingface_provider", "huggingface_provider")
                    .unwrap_or_else(|| {
                        if self.config.provider == zorai_shared::providers::PROVIDER_ID_HUGGINGFACE
                        {
                            self.config.huggingface_provider.as_str()
                        } else {
                            ""
                        }
                    })
                    .trim()
                    .to_string();
            value["huggingface_provider"] = serde_json::Value::String(huggingface_provider);
        }
        value
    }

    #[allow(dead_code)]
    fn all_provider_config_values(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut providers_json = serde_json::Map::new();
        for provider in providers::PROVIDERS {
            providers_json.insert(
                provider.id.to_string(),
                self.provider_config_value(provider.id),
            );
        }
        providers_json
    }

    pub(in crate::app) fn all_provider_wire_config_values(
        &self,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut providers_json = serde_json::Map::new();
        for provider in providers::PROVIDERS {
            providers_json.insert(
                provider.id.to_string(),
                self.provider_wire_config_value(provider.id),
            );
        }
        providers_json
    }

    pub(super) fn snapshot_stats_from_history_db(
        db_path: &std::path::Path,
    ) -> Option<(usize, u64)> {
        let conn = rusqlite::Connection::open(db_path).ok()?;
        let mut stmt = conn.prepare("SELECT path FROM snapshot_index").ok()?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0)).ok()?;

        let mut count = 0usize;
        let mut total_size_bytes = 0u64;
        for path in rows.flatten() {
            if let Ok(metadata) = std::fs::metadata(&path) {
                count += 1;
                total_size_bytes = total_size_bytes.saturating_add(metadata.len());
            }
        }

        Some((count, total_size_bytes))
    }

    pub(in crate::app) fn refresh_snapshot_stats(&mut self) {
        let history_db = zorai_protocol::zorai_data_dir()
            .join("history")
            .join("command-history.db");
        let Some((count, total_size_bytes)) = Self::snapshot_stats_from_history_db(&history_db)
        else {
            self.config.snapshot_count = 0;
            self.config.snapshot_total_size_bytes = 0;
            return;
        };

        self.config.snapshot_count = count;
        self.config.snapshot_total_size_bytes = total_size_bytes;
    }
}
