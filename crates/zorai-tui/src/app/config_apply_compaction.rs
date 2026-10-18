use super::*;

impl TuiModel {
    pub(super) fn apply_compaction_config_json(&mut self, json: &serde_json::Value) {
        let compaction = json.get("compaction");
        let builtin_weles = json
            .get("builtin_sub_agents")
            .and_then(|value| value.get("weles"));
        let compaction_strategy = compaction
            .and_then(|value| value.get("strategy"))
            .and_then(|value| value.as_str())
            .unwrap_or("heuristic");
        self.config.compaction_strategy = match compaction_strategy {
            "weles" => "weles".to_string(),
            "custom_model" => "custom_model".to_string(),
            _ => "heuristic".to_string(),
        };

        let weles_provider = compaction
            .and_then(|value| value.get("weles"))
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                builtin_weles
                    .and_then(|value| value.get("provider"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or(PROVIDER_ID_OPENAI);
        self.config.compaction_weles_provider = weles_provider.to_string();
        self.config.compaction_weles_model = compaction
            .and_then(|value| value.get("weles"))
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                builtin_weles
                    .and_then(|value| value.get("model"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
            })
            .map(str::to_string)
            .unwrap_or_else(|| {
                providers::default_model_for_provider_auth(weles_provider, "api_key")
            });
        self.config.compaction_weles_reasoning_effort = compaction
            .and_then(|value| value.get("weles"))
            .and_then(|value| value.get("reasoning_effort"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                builtin_weles
                    .and_then(|value| value.get("reasoning_effort"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or("medium")
            .to_string();
        self.config.compaction_weles_api_transport = compaction
            .and_then(|value| value.get("weles"))
            .and_then(|value| value.get("api_transport"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                builtin_weles
                    .and_then(|value| value.get("api_transport"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or("")
            .to_string();

        let custom_provider = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.config.provider.as_str());
        let custom_auth_source = normalize_provider_auth_source(
            custom_provider,
            compaction
                .and_then(|value| value.get("custom_model"))
                .and_then(|value| value.get("auth_source"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| providers::default_auth_source_for(custom_provider)),
        );
        let custom_api_transport = normalize_provider_transport(
            custom_provider,
            compaction
                .and_then(|value| value.get("custom_model"))
                .and_then(|value| value.get("api_transport"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| providers::default_transport_for(custom_provider)),
        );
        let default_custom_base_url = if custom_provider == self.config.provider {
            self.config.base_url.clone()
        } else {
            providers::find_by_id(custom_provider)
                .map(|def| def.default_base_url.to_string())
                .unwrap_or_default()
        };
        let default_custom_model = if custom_provider == self.config.provider {
            self.config.model.clone()
        } else {
            providers::default_model_for_provider_auth(custom_provider, &custom_auth_source)
        };
        let default_custom_context_window = if custom_provider == PROVIDER_ID_CUSTOM {
            256_000
        } else {
            providers::known_context_window_for(custom_provider, &default_custom_model)
                .unwrap_or(256_000)
        };
        self.config.compaction_custom_provider = custom_provider.to_string();
        self.config.compaction_custom_base_url = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("base_url"))
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or(default_custom_base_url);
        self.config.compaction_custom_model = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or(default_custom_model);
        self.config.compaction_custom_api_key = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("api_key"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        self.config.compaction_custom_assistant_id = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("assistant_id"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        self.config.compaction_custom_auth_source = custom_auth_source;
        self.config.compaction_custom_api_transport = custom_api_transport;
        self.config.compaction_custom_reasoning_effort = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("reasoning_effort"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.config.reasoning_effort.as_str())
            .to_string();
        self.config.compaction_custom_context_window_tokens = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("context_window_tokens"))
            .and_then(|value| value.as_u64())
            .map(|value| value.max(1000) as u32)
            .unwrap_or(default_custom_context_window);
        self.settings
            .clamp_field_cursor(self.settings.field_count_with_config(&self.config));
    }
}
