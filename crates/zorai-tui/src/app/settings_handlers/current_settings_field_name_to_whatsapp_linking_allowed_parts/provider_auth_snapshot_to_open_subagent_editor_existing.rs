use super::*;
use zorai_shared::providers::{PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENAI};
impl TuiModel {
    pub(crate) fn provider_auth_snapshot(&self, provider_id: &str) -> (String, String, String) {
        let mut base_url = providers::find_by_id(provider_id)
            .map(|def| def.default_base_url.to_string())
            .unwrap_or_default();
        let mut api_key = String::new();
        let mut auth_source =
            if provider_id == PROVIDER_ID_OPENAI && self.config.chatgpt_auth_available {
                "chatgpt_subscription".to_string()
            } else {
                providers::default_auth_source_for(provider_id).to_string()
            };

        if self.config.provider == provider_id {
            return (
                self.config.base_url.clone(),
                self.config.api_key.clone(),
                self.config.auth_source.clone(),
            );
        }

        if let Some(provider_config) = self.saved_provider_config(provider_id) {
            if let Some(value) =
                TuiModel::provider_field_str(provider_config, "base_url", "base_url")
            {
                if !value.is_empty() {
                    base_url = value.to_string();
                }
            }
            if let Some(value) = TuiModel::provider_field_str(provider_config, "api_key", "api_key")
            {
                api_key = value.to_string();
            }
            if let Some(value) =
                TuiModel::provider_field_str(provider_config, "auth_source", "auth_source")
            {
                auth_source = value.to_string();
            }
        }

        if let Some(entry) = self
            .auth
            .entries
            .iter()
            .find(|entry| entry.provider_id == provider_id)
        {
            if !entry.auth_source.trim().is_empty() {
                auth_source = entry.auth_source.clone();
            }
        }

        (base_url, api_key, auth_source)
    }

    pub(crate) fn provider_transport_snapshot(
        &self,
        provider_id: &str,
        auth_source: &str,
        model: &str,
        fallback_transport: &str,
    ) -> String {
        let mut api_transport = fallback_transport.to_string();

        if self.config.provider == provider_id {
            api_transport = self.config.api_transport.clone();
        } else if let Some(provider_config) = self.saved_provider_config(provider_id) {
            if let Some(value) =
                TuiModel::provider_field_str(provider_config, "api_transport", "api_transport")
            {
                api_transport = value.to_string();
            }
        }

        self.normalize_provider_transport_for_state(provider_id, auth_source, model, &api_transport)
    }

    fn normalize_provider_transport_for_state(
        &self,
        provider_id: &str,
        auth_source: &str,
        model: &str,
        api_transport: &str,
    ) -> String {
        if provider_id == PROVIDER_ID_OPENAI && auth_source == "chatgpt_subscription" {
            return "responses".to_string();
        }
        if let Some(fixed_transport) = providers::fixed_transport_for_model(provider_id, model) {
            return fixed_transport.to_string();
        }
        if providers::uses_fixed_anthropic_messages(provider_id, model) {
            return "chat_completions".to_string();
        }
        if providers::supported_transports_for(provider_id).contains(&api_transport) {
            api_transport.to_string()
        } else {
            providers::default_transport_for(provider_id).to_string()
        }
    }

    fn saved_provider_config(&self, provider_id: &str) -> Option<&serde_json::Value> {
        self.config.agent_config_raw.as_ref().and_then(|raw| {
            raw.get("providers")
                .and_then(|providers| providers.get(provider_id))
                .or_else(|| raw.get(provider_id))
        })
    }

    pub(crate) fn should_fetch_remote_models(&self, provider_id: &str, auth_source: &str) -> bool {
        providers::supports_model_fetch_for(provider_id)
            && !(provider_id == PROVIDER_ID_OPENAI && auth_source == "chatgpt_subscription")
    }

    fn upsert_saved_provider_api_key(&mut self, provider_id: &str, api_key: &str) {
        let mut raw = self
            .config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        if raw
            .get(provider_id)
            .and_then(|value| value.as_object())
            .is_none()
        {
            raw[provider_id] = self.provider_config_value(provider_id);
        }
        raw[provider_id]["api_key"] = serde_json::Value::String(api_key.to_string());

        if raw
            .get("providers")
            .and_then(|value| value.as_object())
            .is_none()
        {
            raw["providers"] = serde_json::json!({});
        }
        if raw["providers"]
            .get(provider_id)
            .and_then(|value| value.as_object())
            .is_none()
        {
            raw["providers"][provider_id] = self.provider_wire_config_value(provider_id);
        }
        raw["providers"][provider_id]["api_key"] = serde_json::Value::String(api_key.to_string());

        self.config.agent_config_raw = Some(raw);
    }

    pub(crate) fn clear_saved_provider_api_key(&mut self, provider_id: &str) {
        self.upsert_saved_provider_api_key(provider_id, "");
    }

    pub(crate) fn apply_provider_selection_internal(&mut self, provider_id: &str, sync: bool) {
        let Some(def) = providers::find_by_id(provider_id) else {
            let Some(entry) = self
                .auth
                .entries
                .iter()
                .find(|entry| entry.provider_id == provider_id)
                .cloned()
            else {
                self.status_line = format!("Unknown provider: {provider_id}");
                return;
            };

            self.config.provider = entry.provider_id.clone();
            self.config.base_url.clear();
            self.config.model = entry.model;
            self.config.custom_model_name.clear();
            self.config.api_key.clear();
            self.config.auth_source = entry.auth_source;
            self.config.api_transport = "chat_completions".to_string();
            self.config.assistant_id.clear();
            self.config.custom_context_window_tokens = None;
            self.config.context_window_tokens = 128_000;
            self.status_line = format!("Provider: {}", entry.provider_name);
            if sync {
                self.sync_config_to_daemon();
            }
            return;
        };

        self.config.provider = def.id.to_string();
        self.config.base_url = def.default_base_url.to_string();
        self.config.model = def.default_model.to_string();
        self.config.custom_model_name.clear();
        self.config.api_key.clear();
        self.config.auth_source =
            if def.id == PROVIDER_ID_OPENAI && self.config.chatgpt_auth_available {
                "chatgpt_subscription".to_string()
            } else {
                providers::default_auth_source_for(def.id).to_string()
            };
        self.config.api_transport = def.default_transport.to_string();
        self.config.assistant_id.clear();
        self.config.custom_context_window_tokens = if def.id == PROVIDER_ID_CUSTOM {
            Some(128_000)
        } else {
            None
        };
        self.config.context_window_tokens = if def.id == PROVIDER_ID_CUSTOM {
            self.config.custom_context_window_tokens.unwrap_or(128_000)
        } else {
            providers::known_context_window_for(def.id, def.default_model).unwrap_or(128_000)
        };

        if let Some(raw) = &self.config.agent_config_raw {
            if let Some(provider_config) = raw
                .get("providers")
                .and_then(|providers| providers.get(def.id))
                .or_else(|| raw.get(def.id))
            {
                if let Some(saved_base_url) =
                    TuiModel::provider_field_str(provider_config, "base_url", "base_url")
                {
                    if !saved_base_url.is_empty()
                        && (providers::provider_uses_configurable_base_url(def.id)
                            || providers::provider_base_url_is_customized(def.id, saved_base_url))
                    {
                        self.config.base_url = saved_base_url.to_string();
                    }
                }
                if let Some(key) =
                    TuiModel::provider_field_str(provider_config, "api_key", "api_key")
                {
                    self.config.api_key = key.to_string();
                }
                if let Some(saved_model) =
                    TuiModel::provider_field_str(provider_config, "model", "model")
                {
                    if !saved_model.is_empty() {
                        self.config.model = saved_model.to_string();
                    }
                }
                if let Some(saved_custom_model_name) = TuiModel::provider_field_str(
                    provider_config,
                    "custom_model_name",
                    "custom_model_name",
                ) {
                    self.config.custom_model_name = saved_custom_model_name.to_string();
                }
                if let Some(saved_transport) =
                    TuiModel::provider_field_str(provider_config, "api_transport", "api_transport")
                {
                    self.config.api_transport =
                        if def.supported_transports.contains(&saved_transport) {
                            saved_transport.to_string()
                        } else {
                            def.default_transport.to_string()
                        };
                }
                if let Some(saved_auth_source) =
                    TuiModel::provider_field_str(provider_config, "auth_source", "auth_source")
                {
                    self.config.auth_source =
                        if def.supported_auth_sources.contains(&saved_auth_source) {
                            saved_auth_source.to_string()
                        } else {
                            def.default_auth_source.to_string()
                        };
                }
                if let Some(saved_assistant_id) =
                    TuiModel::provider_field_str(provider_config, "assistant_id", "assistant_id")
                {
                    self.config.assistant_id = saved_assistant_id.to_string();
                }
                self.config.custom_context_window_tokens = TuiModel::provider_field_u64(
                    provider_config,
                    "context_window_tokens",
                    "context_window_tokens",
                )
                .map(|value| value.max(1000) as u32);
                self.config.context_window_tokens =
                    TuiModel::effective_context_window_for_provider_value(def.id, provider_config);
            }
        }

        if def.id == PROVIDER_ID_OPENAI && self.config.auth_source == "chatgpt_subscription" {
            self.config.api_transport = "responses".to_string();
            self.refresh_openai_auth_status();
        }

        self.config.api_transport = self.normalize_provider_transport_for_state(
            &self.config.provider,
            &self.config.auth_source,
            &self.config.model,
            &self.config.api_transport,
        );

        self.refresh_provider_models_for_current_auth();
        if providers::known_models_for_provider_auth(
            &self.config.provider,
            &self.config.auth_source,
        )
        .iter()
        .any(|model| model.id == self.config.model)
        {
            self.config.custom_model_name.clear();
        } else if self.config.custom_model_name.trim().is_empty()
            && !self.config.model.trim().is_empty()
        {
            self.config.custom_model_name = self.config.model.clone();
        }
        self.status_line = format!("Provider: {}", def.name);
        if sync {
            self.sync_config_to_daemon();
        }
    }

    fn stored_provider_api_key(&self, provider_id: &str) -> String {
        self.config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("providers"))
            .and_then(|providers| providers.get(provider_id))
            .and_then(|provider| TuiModel::provider_field_str(provider, "api_key", "api_key"))
            .map(str::to_string)
            .or_else(|| {
                self.config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get(provider_id))
                    .and_then(|provider| {
                        TuiModel::provider_field_str(provider, "api_key", "api_key")
                    })
                    .map(str::to_string)
            })
            .unwrap_or_default()
    }

    pub(crate) fn start_auth_login(&mut self, provider_id: &str, provider_name: &str) {
        let initial_key = if self.config.provider == provider_id {
            self.config.api_key.clone()
        } else {
            self.stored_provider_api_key(provider_id)
        };
        self.auth.reduce(crate::state::auth::AuthAction::StartLogin(
            provider_id.to_string(),
        ));
        self.auth.login_buffer = initial_key;
        self.auth.login_cursor = self.auth.login_buffer.chars().count();
        self.auth.actions_focused = false;
        self.status_line = format!("Enter API key for {provider_name}");
    }

    pub(crate) fn confirm_auth_login(&mut self) {
        let Some(provider_id) = self.auth.login_target.clone() else {
            return;
        };
        let provider_name = self
            .auth
            .entries
            .iter()
            .find(|entry| entry.provider_id == provider_id)
            .map(|entry| entry.provider_name.clone())
            .or_else(|| providers::find_by_id(&provider_id).map(|def| def.name.to_string()))
            .unwrap_or_else(|| provider_id.clone());
        let api_key = self.auth.login_buffer.trim().to_string();

        if api_key.is_empty() {
            self.status_line = format!("API key required for {provider_name}");
            return;
        }

        let is_active_provider = self.config.provider == provider_id;
        if self.config.provider == provider_id {
            self.config.api_key = api_key.clone();
        }
        self.upsert_saved_provider_api_key(&provider_id, &api_key);
        if let Some(entry) = self
            .auth
            .entries
            .iter_mut()
            .find(|entry| entry.provider_id == provider_id)
        {
            entry.authenticated = true;
            if entry.auth_source.trim().is_empty() {
                entry.auth_source = "api_key".to_string();
            }
        }

        let (base_url, _, _) = self.provider_auth_snapshot(&provider_id);
        self.send_daemon_command(DaemonCommand::LoginProvider {
            provider_id: provider_id.clone(),
            api_key,
            base_url,
        });
        if is_active_provider {
            self.send_daemon_command(DaemonCommand::GetConfig);
        }
        self.auth
            .reduce(crate::state::auth::AuthAction::ConfirmLogin);
        self.status_line = format!("Saved credentials for {provider_name}");
    }

    pub(crate) fn open_subagent_editor_new(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let default_provider = self.config.provider.clone();
        let default_model = if self.config.model.trim().is_empty() {
            providers::default_model_for_provider_auth(&default_provider, "api_key")
        } else {
            self.config.model.clone()
        };
        let mut editor = crate::state::subagents::SubAgentEditorState::new(
            None,
            now,
            default_provider,
            default_model,
        );
        editor.name = format!("Sub-Agent {}", self.subagents.entries.len() + 1);
        editor.context_window_tokens =
            providers::known_context_window_for(&editor.provider, &editor.model);
        self.subagents.editor = Some(editor);
        self.subagents.actions_focused = false;
    }

    pub(crate) fn open_subagent_editor_existing(&mut self) {
        let Some(entry) = self.subagents.selected_entry().cloned() else {
            return;
        };
        self.open_subagent_editor_for_entry(entry);
    }

    pub(crate) fn open_weles_agent_editor(&mut self) {
        let Some(entry) = self
            .subagents
            .entries
            .iter()
            .find(|entry| entry.id == "weles_builtin")
            .cloned()
        else {
            self.send_daemon_command(DaemonCommand::ListSubAgents);
            self.status_line = "WELES agent is not loaded yet".to_string();
            return;
        };
        self.open_subagent_editor_for_entry(entry);
    }

    fn open_subagent_editor_for_entry(&mut self, entry: crate::state::subagents::SubAgentEntry) {
        let raw = entry
            .raw_json
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let created_at = raw
            .get("created_at")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let mut editor = crate::state::subagents::SubAgentEditorState::new(
            Some(entry.id.clone()),
            created_at,
            entry.provider.clone(),
            entry.model.clone(),
        );
        let system_prompt = self.subagent_editor_system_prompt_override(&entry, &raw);
        editor.name = entry.name;
        editor.base_url = raw
            .get("base_url")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        editor.context_window_tokens = raw
            .get("context_window_tokens")
            .and_then(|value| value.as_u64())
            .map(|value| value.min(u32::MAX as u64) as u32);
        editor.role = entry.role.clone().unwrap_or_default();
        editor.system_prompt = system_prompt;
        editor.enabled = entry.enabled;
        editor.builtin = entry.builtin;
        editor.immutable_identity = entry.immutable_identity;
        editor.disable_allowed = entry.disable_allowed;
        editor.delete_allowed = entry.delete_allowed;
        editor.protected_reason = entry.protected_reason.clone();
        editor.reasoning_effort = entry.reasoning_effort.clone();
        editor.api_transport = entry.api_transport.clone();
        editor.claude_permission_mode = entry.claude_permission_mode.clone();
        editor.openrouter_provider_order =
            crate::state::subagents::openrouter_provider_list_from_json(
                &raw,
                "openrouter_provider_order",
            );
        editor.openrouter_provider_ignore =
            crate::state::subagents::openrouter_provider_list_from_json(
                &raw,
                "openrouter_provider_ignore",
            );
        editor.openrouter_allow_fallbacks = raw
            .get("openrouter_allow_fallbacks")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        editor.huggingface_provider = raw
            .get("huggingface_provider")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        editor.raw_json = Some(raw);
        editor.previous_role_preset = editor
            .role_preset_index()
            .and_then(|index| crate::state::subagents::SUBAGENT_ROLE_PRESETS.get(index))
            .map(|preset| preset.id.to_string());
        self.subagents.editor = Some(editor);
        self.subagents.actions_focused = false;
    }
}
