use super::*;
use crate::providers;
impl TuiModel {
    pub(crate) fn commit_subagent_editor(&mut self) {
        let Some(editor) = self.subagents.editor.clone() else {
            return;
        };
        if editor.name.trim().is_empty() {
            self.status_line = "Sub-agent name is required".to_string();
            return;
        }
        if editor.provider.trim().is_empty() {
            self.status_line = "Sub-agent provider is required".to_string();
            return;
        }
        if editor.model.trim().is_empty() {
            self.status_line = "Sub-agent model is required".to_string();
            return;
        }

        let id = editor.id.clone().unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            format!("subagent-{now}")
        });
        let active_owner_matches = self
            .active_thread_owner_agent_id()
            .is_some_and(|owner_id| owner_id.eq_ignore_ascii_case(&id));
        let mut raw = editor
            .raw_json
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let role = if editor.role.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(editor.role.trim().to_string())
        };
        let system_prompt = if editor.system_prompt.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(editor.system_prompt.trim().to_string())
        };
        let existing_name = raw
            .get("name")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        if let Some(obj) = raw.as_object_mut() {
            obj.insert("id".to_string(), serde_json::Value::String(id));
            let name = if editor.identity_is_mutable() {
                editor.name.trim().to_string()
            } else {
                existing_name.unwrap_or_else(|| editor.name.trim().to_string())
            };
            obj.insert("name".to_string(), serde_json::Value::String(name));
            obj.insert(
                "provider".to_string(),
                serde_json::Value::String(editor.provider.clone()),
            );
            obj.insert(
                "model".to_string(),
                serde_json::Value::String(editor.model.clone()),
            );
            let base_url = editor.base_url.trim();
            if base_url.is_empty() {
                obj.remove("base_url");
            } else {
                obj.insert(
                    "base_url".to_string(),
                    serde_json::Value::String(base_url.to_string()),
                );
            }
            if let Some(context_window_tokens) = editor.context_window_tokens {
                obj.insert(
                    "context_window_tokens".to_string(),
                    serde_json::Value::Number(context_window_tokens.into()),
                );
            } else {
                obj.remove("context_window_tokens");
            }
            obj.insert("role".to_string(), role);
            obj.insert("system_prompt".to_string(), system_prompt);
            obj.insert(
                "enabled".to_string(),
                serde_json::Value::Bool(editor.enabled),
            );
            obj.insert(
                "builtin".to_string(),
                serde_json::Value::Bool(editor.builtin),
            );
            obj.insert(
                "immutable_identity".to_string(),
                serde_json::Value::Bool(editor.immutable_identity),
            );
            obj.insert(
                "disable_allowed".to_string(),
                serde_json::Value::Bool(editor.disable_allowed),
            );
            obj.insert(
                "delete_allowed".to_string(),
                serde_json::Value::Bool(editor.delete_allowed),
            );
            obj.insert(
                "protected_reason".to_string(),
                editor
                    .protected_reason
                    .clone()
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            obj.insert(
                "reasoning_effort".to_string(),
                editor
                    .reasoning_effort
                    .clone()
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            obj.insert(
                "api_transport".to_string(),
                editor
                    .api_transport
                    .clone()
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            obj.insert(
                "claude_permission_mode".to_string(),
                editor
                    .claude_permission_mode
                    .clone()
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            if editor.provider == zorai_shared::providers::PROVIDER_ID_OPENROUTER {
                obj.insert(
                    "openrouter_provider_order".to_string(),
                    crate::state::subagents::openrouter_provider_list_to_json(
                        &editor.openrouter_provider_order,
                    ),
                );
                obj.insert(
                    "openrouter_provider_ignore".to_string(),
                    crate::state::subagents::openrouter_provider_list_to_json(
                        &editor.openrouter_provider_ignore,
                    ),
                );
                obj.insert(
                    "openrouter_allow_fallbacks".to_string(),
                    serde_json::Value::Bool(editor.openrouter_allow_fallbacks),
                );
            } else {
                obj.remove("openrouter_provider_order");
                obj.remove("openrouter_provider_ignore");
                obj.remove("openrouter_allow_fallbacks");
            }
            if editor.provider == zorai_shared::providers::PROVIDER_ID_HUGGINGFACE {
                let huggingface_provider = editor.huggingface_provider.trim();
                if huggingface_provider.is_empty() {
                    obj.remove("huggingface_provider");
                } else {
                    obj.insert(
                        "huggingface_provider".to_string(),
                        serde_json::Value::String(huggingface_provider.to_string()),
                    );
                }
            } else {
                obj.remove("huggingface_provider");
            }
            obj.insert(
                "created_at".to_string(),
                serde_json::Value::Number(editor.created_at.into()),
            );
        }
        self.send_daemon_command(DaemonCommand::SetSubAgent(raw.to_string()));
        if active_owner_matches {
            crate::app::commands::apply_active_thread_provider_model_to_daemon(
                self,
                &editor.provider,
                &editor.model,
                editor.reasoning_effort.as_deref(),
            );
        }

        let optimistic = crate::state::SubAgentEntry {
            id: raw
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            name: raw
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            provider: raw
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            model: raw
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            role: raw
                .get("role")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            enabled: raw.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
            builtin: raw
                .get("builtin")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            immutable_identity: raw
                .get("immutable_identity")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            disable_allowed: raw
                .get("disable_allowed")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            delete_allowed: raw
                .get("delete_allowed")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            protected_reason: raw
                .get("protected_reason")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            reasoning_effort: raw
                .get("reasoning_effort")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            api_transport: raw
                .get("api_transport")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(ToString::to_string),
            claude_permission_mode: raw
                .get("claude_permission_mode")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(ToString::to_string),
            openrouter_provider_order: crate::state::subagents::openrouter_provider_list_from_json(
                &raw,
                "openrouter_provider_order",
            ),
            openrouter_provider_ignore: crate::state::subagents::openrouter_provider_list_from_json(
                &raw,
                "openrouter_provider_ignore",
            ),
            openrouter_allow_fallbacks: raw
                .get("openrouter_allow_fallbacks")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            huggingface_provider: raw
                .get("huggingface_provider")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            raw_json: Some(raw),
        };
        if self
            .subagents
            .entries
            .iter()
            .any(|entry| entry.id == optimistic.id)
        {
            self.subagents
                .reduce(crate::state::subagents::SubAgentsAction::Updated(
                    optimistic,
                ));
            self.status_line = "Updated sub-agent".to_string();
        } else {
            self.subagents
                .reduce(crate::state::subagents::SubAgentsAction::Added(optimistic));
            self.status_line = "Added sub-agent".to_string();
        }
        self.subagents.editor = None;
        self.settings.reduce(SettingsAction::CancelEdit);
    }

    pub(crate) fn open_subagent_provider_picker(&mut self) {
        self.settings_picker_target = Some(SettingsPickerTarget::SubAgentProvider);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
        self.sync_provider_picker_item_count();
    }

    pub(crate) fn open_subagent_model_picker(&mut self) {
        let Some(editor) = self.subagents.editor.as_ref() else {
            return;
        };
        let provider_id = editor.provider.clone();
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        self.open_provider_backed_model_picker(
            SettingsPickerTarget::SubAgentModel,
            provider_id,
            base_url,
            api_key,
            auth_source,
        );
    }

    pub(crate) fn open_subagent_role_picker(&mut self) {
        self.settings_picker_target = Some(SettingsPickerTarget::SubAgentRole);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::RolePicker));
        self.modal
            .set_picker_item_count(crate::state::subagents::role_picker_item_count());
    }

    pub(crate) fn open_subagent_effort_picker(&mut self) {
        self.settings_picker_target = Some(SettingsPickerTarget::SubAgentReasoningEffort);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
        self.modal.set_picker_item_count(7);
    }

    pub(crate) fn open_subagent_openrouter_provider_picker(
        &mut self,
        target: SettingsPickerTarget,
    ) {
        let Some(editor) = self.subagents.editor.as_ref() else {
            return;
        };
        if editor.provider != zorai_shared::providers::PROVIDER_ID_OPENROUTER {
            self.status_line =
                "OpenRouter provider routing only applies to OpenRouter sub-agents".to_string();
            return;
        }
        let model = editor.model.clone();
        let (base_url, api_key, _auth_source) =
            self.provider_auth_snapshot(zorai_shared::providers::PROVIDER_ID_OPENROUTER);
        self.open_openrouter_provider_picker_for(target, model, base_url, api_key);
    }

    pub(crate) fn send_concierge_config(&mut self) {
        let mut config = serde_json::json!({
            "enabled": self.concierge.enabled,
            "detail_level": self.concierge.detail_level,
            "provider": self.concierge.provider,
            "model": self.concierge.model,
            "base_url": (!self.concierge.base_url.trim().is_empty())
                .then(|| self.concierge.base_url.trim().to_string()),
            "reasoning_effort": self.concierge.reasoning_effort,
            "api_transport": self.concierge.api_transport,
            "claude_permission_mode": self.concierge.claude_permission_mode,
            "auto_cleanup_on_navigate": self.concierge.auto_cleanup_on_navigate,
        });
        if self.concierge.provider.as_deref()
            == Some(zorai_shared::providers::PROVIDER_ID_OPENROUTER)
        {
            config["openrouter_provider_order"] =
                crate::state::subagents::openrouter_provider_list_to_json(
                    &self.concierge.openrouter_provider_order,
                );
            config["openrouter_provider_ignore"] =
                crate::state::subagents::openrouter_provider_list_to_json(
                    &self.concierge.openrouter_provider_ignore,
                );
            config["openrouter_allow_fallbacks"] =
                serde_json::Value::Bool(self.concierge.openrouter_allow_fallbacks);
        }
        if self.concierge.provider.as_deref()
            == Some(zorai_shared::providers::PROVIDER_ID_HUGGINGFACE)
        {
            let huggingface_provider = self.concierge.huggingface_provider.trim();
            if !huggingface_provider.is_empty() {
                config["huggingface_provider"] =
                    serde_json::Value::String(huggingface_provider.to_string());
            }
        }
        self.send_daemon_command(DaemonCommand::SetConciergeConfig(config.to_string()));
        self.send_daemon_command(DaemonCommand::GetConciergeConfig);
    }

    pub(crate) fn open_concierge_openrouter_provider_picker(
        &mut self,
        target: SettingsPickerTarget,
    ) {
        if self.concierge.provider.as_deref()
            != Some(zorai_shared::providers::PROVIDER_ID_OPENROUTER)
        {
            self.status_line =
                "OpenRouter provider routing only applies to OpenRouter agents".to_string();
            return;
        }
        let Some(model) = self.concierge.model.clone() else {
            self.status_line = "Choose an OpenRouter model first".to_string();
            return;
        };
        let (base_url, api_key, _auth_source) =
            self.provider_auth_snapshot(zorai_shared::providers::PROVIDER_ID_OPENROUTER);
        self.open_openrouter_provider_picker_for(target, model, base_url, api_key);
    }

    pub(super) fn refresh_provider_models_for_current_auth(&mut self) {
        let models = providers::known_models_for_provider_auth(
            &self.config.provider,
            &self.config.auth_source,
        );
        if !models.is_empty() {
            self.config
                .reduce(config::ConfigAction::ModelsFetched(models.clone()));
            if !models.iter().any(|model| model.id == self.config.model) {
                let fallback = providers::default_model_for_provider_auth(
                    &self.config.provider,
                    &self.config.auth_source,
                );
                self.config.reduce(config::ConfigAction::SetModel(fallback));
            }
        }
    }

    pub(crate) fn apply_provider_selection(&mut self, provider_id: &str) {
        self.apply_provider_selection_internal(provider_id, true);
    }

    pub(crate) fn apply_provider_selection_without_sync(&mut self, provider_id: &str) {
        self.apply_provider_selection_internal(provider_id, false);
    }

    pub(crate) fn run_auth_tab_action(&mut self) {
        let Some(entry) = self.auth.entries.get(self.auth.selected).cloned() else {
            return;
        };

        match self.auth.action_cursor {
            0 => {
                if entry.provider_id == zorai_shared::providers::PROVIDER_ID_CURSOR {
                    if entry.authenticated {
                        self.send_daemon_command(DaemonCommand::LogoutProvider {
                            provider_id: entry.provider_id.clone(),
                        });
                        self.status_line = "Signing out of the Cursor subscription...".to_string();
                    } else {
                        self.send_daemon_command(DaemonCommand::LoginProvider {
                            provider_id: entry.provider_id.clone(),
                            api_key: String::new(),
                            base_url: String::new(),
                        });
                        self.status_line =
                            "Started Cursor subscription login. Refresh after you finish in the browser."
                                .to_string();
                    }
                    return;
                }
                if entry.provider_id == zorai_shared::providers::PROVIDER_ID_CLAUDE_CODE_CLI {
                    self.status_line = if entry.authenticated {
                        "Claude Code CLI detected — no API key needed (local auth via `claude`)."
                            .to_string()
                    } else {
                        "Claude Code CLI not found on PATH. Install Claude Code; no API key is used."
                            .to_string()
                    };
                    return;
                }
                if entry.authenticated {
                    if entry.provider_id == PROVIDER_ID_OPENAI
                        && entry.auth_source == "chatgpt_subscription"
                    {
                        self.send_daemon_command(DaemonCommand::LogoutOpenAICodex);
                    } else if entry.provider_id == PROVIDER_ID_GITHUB_COPILOT
                        && entry.auth_source == "github_copilot"
                    {
                        match crate::auth::clear_github_copilot_auth() {
                            Ok(()) => {
                                self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                                self.status_line = "GitHub Copilot auth cleared".to_string();
                            }
                            Err(err) => {
                                self.status_line =
                                    format!("Failed to clear GitHub Copilot auth: {err}");
                            }
                        }
                    } else {
                        if self.config.provider == entry.provider_id {
                            self.config.api_key.clear();
                        }
                        self.clear_saved_provider_api_key(&entry.provider_id);
                        if let Ok(value_json) =
                            serde_json::to_string(&serde_json::Value::String(String::new()))
                        {
                            self.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/providers/{}/api_key", entry.provider_id),
                                value_json: value_json.clone(),
                            });
                            self.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/{}/api_key", entry.provider_id),
                                value_json: value_json.clone(),
                            });
                            if self.config.provider == entry.provider_id {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/api_key".to_string(),
                                    value_json,
                                });
                            }
                        }
                        self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                        self.status_line =
                            format!("Cleared credentials for {}", entry.provider_name);
                    }
                } else if entry.provider_id == PROVIDER_ID_OPENAI
                    && entry.auth_source == "chatgpt_subscription"
                {
                    self.send_daemon_command(DaemonCommand::LoginOpenAICodex);
                } else if entry.provider_id == PROVIDER_ID_GITHUB_COPILOT
                    && entry.auth_source == "github_copilot"
                {
                    self.start_auth_login(&entry.provider_id, &entry.provider_name);
                } else {
                    self.start_auth_login(&entry.provider_id, &entry.provider_name);
                }
            }
            1 => {
                if entry.provider_id == zorai_shared::providers::PROVIDER_ID_CLAUDE_CODE_CLI {
                    self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                    self.status_line = "Re-checking Claude Code CLI availability...".to_string();
                    return;
                }
                if entry.provider_id == zorai_shared::providers::PROVIDER_ID_CURSOR {
                    self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                    self.status_line = "Re-checking Cursor subscription login...".to_string();
                    return;
                }
                if entry.provider_id == PROVIDER_ID_OPENAI && self.config.chatgpt_auth_available {
                    self.send_daemon_command(DaemonCommand::LogoutOpenAICodex);
                } else if entry.provider_id == PROVIDER_ID_OPENAI {
                    self.send_daemon_command(DaemonCommand::LoginOpenAICodex);
                } else if !entry.authenticated
                    && entry.provider_id == PROVIDER_ID_GITHUB_COPILOT
                    && entry.auth_source == "github_copilot"
                {
                    match crate::auth::begin_github_copilot_auth_flow() {
                        Ok(crate::auth::GithubCopilotAuthFlowResult::AlreadyAvailable) => {
                            self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                            self.status_line = "GitHub Copilot auth already available".to_string();
                        }
                        Ok(crate::auth::GithubCopilotAuthFlowResult::ImportedFromGhCli) => {
                            self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                            self.status_line =
                                "Imported GitHub Copilot auth from GitHub CLI".to_string();
                        }
                        Ok(crate::auth::GithubCopilotAuthFlowResult::Started) => {
                            self.status_line =
                                "Started GitHub Copilot browser login. Refresh after completing it."
                                    .to_string();
                        }
                        Err(err) => {
                            self.status_line =
                                format!("Failed to start GitHub Copilot auth: {err}");
                        }
                    }
                } else {
                    let (base_url, api_key, auth_source) =
                        self.provider_auth_snapshot(&entry.provider_id);
                    if entry.provider_id == PROVIDER_ID_OPENAI
                        && auth_source == "chatgpt_subscription"
                    {
                        self.refresh_openai_auth_status();
                        self.status_line =
                            "Refreshing ChatGPT subscription auth status...".to_string();
                    } else {
                        self.auth.validating = Some(entry.provider_id.clone());
                        self.send_daemon_command(DaemonCommand::ValidateProvider {
                            provider_id: entry.provider_id,
                            base_url,
                            api_key,
                            auth_source,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn run_subagent_action(&mut self) {
        match self.subagents.action_cursor {
            0 => {
                self.open_subagent_editor_new();
            }
            1 => {
                self.open_subagent_editor_existing();
            }
            2 => {
                if let Some(entry) = self.subagents.entries.get(self.subagents.selected) {
                    if !entry.delete_allowed {
                        self.status_line = "This sub-agent cannot be deleted".to_string();
                        return;
                    }
                    self.send_daemon_command(DaemonCommand::RemoveSubAgent(entry.id.clone()));
                    self.subagents
                        .reduce(crate::state::subagents::SubAgentsAction::Removed(
                            entry.id.clone(),
                        ));
                }
            }
            3 => {
                if let Some(entry) = self.subagents.entries.get(self.subagents.selected) {
                    if entry.enabled && !entry.disable_allowed {
                        self.status_line = "This sub-agent cannot be disabled".to_string();
                        return;
                    }
                    if let Some(ref raw) = entry.raw_json {
                        let mut updated = raw.clone();
                        if let Some(obj) = updated.as_object_mut() {
                            obj.insert(
                                "enabled".to_string(),
                                serde_json::Value::Bool(!entry.enabled),
                            );
                        }
                        self.send_daemon_command(DaemonCommand::SetSubAgent(updated.to_string()));
                        self.subagents.reduce(
                            crate::state::subagents::SubAgentsAction::ToggleEnabled(
                                entry.id.clone(),
                            ),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
