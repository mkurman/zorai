use super::*;
use crate::state::settings::SettingsAction;

const CUSTOM_MODEL_EDIT_FIELD: &str = "thread_owner_custom_model";

impl TuiModel {
    pub(crate) fn parse_custom_model_entry(raw: &str) -> Option<(String, String)> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let (name, model_id) = if let Some((lhs, rhs)) = trimmed.split_once('|') {
            (lhs.trim().to_string(), rhs.trim().to_string())
        } else {
            (String::new(), trimmed.to_string())
        };
        if model_id.is_empty() {
            None
        } else {
            Some((name, model_id))
        }
    }

    fn current_thread_custom_model_edit_value(&self) -> String {
        if let Some(pending) = self.pending_target_agent_config.as_ref() {
            return pending.model.clone();
        }
        if self.config.custom_model_name.trim().is_empty()
            || self.config.custom_model_name == self.config.model
        {
            self.config.model.clone()
        } else {
            format!("{} | {}", self.config.custom_model_name, self.config.model)
        }
    }

    pub(crate) fn open_active_thread_custom_model_editor(&mut self) {
        if self.pending_target_agent_config.is_none() {
            if let Some(pending) = self.active_thread_target_agent_config() {
                self.pending_target_agent_config = Some(pending);
            }
        }
        let current = self.current_thread_custom_model_edit_value();
        let owner_label = self.active_thread_context_owner_label();
        if self.modal.top() != Some(modal::ModalKind::CustomModelEditor) {
            self.modal.reduce(modal::ModalAction::Push(
                modal::ModalKind::CustomModelEditor,
            ));
        }
        self.settings
            .start_editing(CUSTOM_MODEL_EDIT_FIELD, &current);
        self.status_line = format!("Enter custom model ID for {owner_label}");
    }

    pub(crate) fn custom_model_editor_body(&self) -> String {
        let owner_label = self.active_thread_context_owner_label();
        let buffer = self.settings.edit_buffer();
        format!(
            "Pinned to {owner_label}\n\nModel: {buffer}█\n\nEnter save   Esc cancel\nName | ID or just ID"
        )
    }

    pub(crate) fn commit_active_thread_custom_model_editor(&mut self) {
        let Some((name, model_id)) = Self::parse_custom_model_entry(self.settings.edit_buffer())
        else {
            self.status_line = "Model ID cannot be empty".to_string();
            return;
        };
        self.apply_active_thread_custom_model(&name, &model_id);
        self.settings.reduce(SettingsAction::ConfirmEdit);
        self.pending_target_agent_config = None;
        self.close_top_modal();
    }

    pub(crate) fn cancel_active_thread_custom_model_editor(&mut self) {
        self.settings.reduce(SettingsAction::CancelEdit);
        self.pending_target_agent_config = None;
        self.close_top_modal();
        self.status_line = "Model unchanged".to_string();
    }

    pub(crate) fn apply_active_thread_custom_model(&mut self, name: &str, model_id: &str) {
        if let Some(pending) = self.pending_target_agent_config.clone() {
            apply_target_agent_custom_model_locally(
                self,
                &pending.target_agent_id,
                &pending.target_agent_name,
                &pending.provider_id,
                model_id,
            );
            apply_active_thread_provider_model_to_daemon(
                self,
                &pending.provider_id,
                model_id,
                pending.reasoning_effort.as_deref(),
            );
            self.status_line = format!("{} model: {model_id}", pending.target_agent_name);
            return;
        }

        apply_svarog_custom_model_locally(self, name, model_id);
        if let Ok(value_json) =
            serde_json::to_string(&serde_json::Value::String(model_id.to_string()))
        {
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: "/model".to_string(),
                value_json: value_json.clone(),
            });
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: format!("/providers/{}/model", self.config.provider),
                value_json: value_json.clone(),
            });
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: format!("/{}/model", self.config.provider),
                value_json,
            });
        }
        self.status_line = format!(
            "{} model: {model_id}",
            self.active_thread_context_owner_label()
        );
        let provider_id = self.config.provider.clone();
        apply_active_thread_provider_model_to_daemon(self, &provider_id, model_id, None);
    }
}

fn apply_svarog_custom_model_locally(model: &mut TuiModel, name: &str, model_id: &str) {
    model.config.model = model_id.to_string();
    let resolved_context_window = providers::resolve_context_window_for_provider_auth(
        &model.config.provider,
        &model.config.auth_source,
        &model.config.model,
        name,
    );
    if providers::model_uses_context_window_override(
        &model.config.provider,
        &model.config.auth_source,
        &model.config.model,
        name,
    ) {
        model.config.custom_model_name = if name.is_empty() {
            model_id.to_string()
        } else {
            name.to_string()
        };
        let next_context =
            resolved_context_window.unwrap_or(providers::default_custom_model_context_window());
        model.config.custom_context_window_tokens = Some(next_context);
        model.config.context_window_tokens = next_context;
    } else {
        model.config.custom_model_name = if name.is_empty() {
            String::new()
        } else {
            name.to_string()
        };
        model.config.custom_context_window_tokens = None;
        model.config.context_window_tokens = resolved_context_window.unwrap_or(128_000);
    }
    model.config.api_transport = model.provider_transport_snapshot(
        &model.config.provider,
        &model.config.auth_source,
        &model.config.model,
        &model.config.api_transport,
    );
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.profile_provider = Some(model.config.provider.clone());
        thread.profile_model = Some(model_id.to_string());
        thread.runtime_provider = Some(model.config.provider.clone());
        thread.runtime_model = Some(model_id.to_string());
    }
}

pub(crate) fn push_active_thread_execution_profile_to_daemon(
    model: &TuiModel,
    provider_id: &str,
    model_id: &str,
    reasoning_effort: Option<&str>,
) {
    let Some(thread_id) = model.chat.active_thread().map(|thread| thread.id.clone()) else {
        return;
    };
    let mut profile = serde_json::Map::new();
    profile.insert(
        "provider".to_string(),
        serde_json::Value::String(provider_id.trim().to_string()),
    );
    profile.insert(
        "model".to_string(),
        serde_json::Value::String(model_id.trim().to_string()),
    );
    if let Some(reasoning_effort) = reasoning_effort
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        profile.insert(
            "reasoning_effort".to_string(),
            serde_json::Value::String(reasoning_effort.to_string()),
        );
    }
    model.send_daemon_command(DaemonCommand::SetThreadExecutionProfile {
        thread_id,
        profile_json: serde_json::Value::Object(profile).to_string(),
    });
}

pub(crate) fn apply_active_thread_provider_model_to_daemon(
    model: &mut TuiModel,
    provider_id: &str,
    model_id: &str,
    reasoning_effort: Option<&str>,
) {
    let provider_id = provider_id.trim();
    let model_id = model_id.trim();
    let reasoning_effort = reasoning_effort
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if model.chat.active_thread().is_none() && model.pending_new_thread_target_agent.is_some() {
        model.pending_new_thread_execution_profile = Some(PendingThreadExecutionProfile {
            provider_id: provider_id.to_string(),
            model: model_id.to_string(),
            reasoning_effort: reasoning_effort.map(ToOwned::to_owned),
        });
    }
    push_active_thread_execution_profile_to_daemon(model, provider_id, model_id, reasoning_effort);
    let Some(target_agent_id) = model.active_thread_owner_agent_id() else {
        return;
    };
    if target_agent_id.eq_ignore_ascii_case(zorai_protocol::AGENT_ID_SWAROG) {
        return;
    }
    model.send_daemon_command(DaemonCommand::SetTargetAgentProviderModel {
        target_agent_id,
        provider_id: provider_id.to_string(),
        model: model_id.to_string(),
    });
}

pub(crate) fn apply_target_agent_custom_model_locally(
    model: &mut TuiModel,
    target_agent_id: &str,
    target_agent_name: &str,
    provider_id: &str,
    model_id: &str,
) {
    if target_agent_id.eq_ignore_ascii_case(zorai_protocol::AGENT_ID_RAROG) {
        model.concierge.model = Some(model_id.to_string());
        if model.concierge.provider.as_deref().unwrap_or("").is_empty() {
            model.concierge.provider = Some(provider_id.to_string());
        }
    }

    if model.is_explicit_builtin_persona(target_agent_id) {
        let key = target_agent_id.trim().to_ascii_lowercase();
        let mut raw = model
            .config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        if raw.get("builtin_sub_agents").is_none() {
            raw["builtin_sub_agents"] = serde_json::json!({});
        }
        raw["builtin_sub_agents"][key.as_str()]["provider"] =
            serde_json::Value::String(provider_id.to_string());
        raw["builtin_sub_agents"][key.as_str()]["model"] =
            serde_json::Value::String(model_id.to_string());
        model.config.agent_config_raw = Some(raw);
    }

    for entry in &mut model.subagents.entries {
        let id_matches = entry.id.eq_ignore_ascii_case(target_agent_id)
            || entry
                .id
                .strip_suffix("_builtin")
                .is_some_and(|alias| alias.eq_ignore_ascii_case(target_agent_id));
        let name_matches = entry.name.eq_ignore_ascii_case(target_agent_name)
            || entry.name.eq_ignore_ascii_case(target_agent_id);
        if id_matches || name_matches {
            entry.model = model_id.to_string();
            entry.provider = provider_id.to_string();
            if let Some(raw) = entry.raw_json.as_mut() {
                raw["model"] = serde_json::Value::String(model_id.to_string());
                raw["provider"] = serde_json::Value::String(provider_id.to_string());
            }
        }
    }

    if let Some(thread) = model.chat.active_thread_mut() {
        thread.profile_provider = Some(provider_id.to_string());
        thread.profile_model = Some(model_id.to_string());
        thread.runtime_provider = Some(provider_id.to_string());
        thread.runtime_model = Some(model_id.to_string());
    }
}
