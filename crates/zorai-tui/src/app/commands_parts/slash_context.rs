use super::*;
use crate::state::settings::SettingsAction;

const MIN_CONTEXT_WINDOW_TOKENS: u32 = 1_000;
const MAX_CONTEXT_WINDOW_TOKENS: u32 = 2_000_000;
const CONTEXT_WINDOW_EDIT_FIELD: &str = "thread_owner_context_window";

impl TuiModel {
    pub(crate) fn parse_context_window_tokens(raw: &str) -> Option<u32> {
        let normalized = raw.trim().replace(['_', ',', ' '], "");
        if normalized.is_empty() {
            return None;
        }
        normalized
            .parse::<u32>()
            .ok()
            .map(|tokens| tokens.clamp(MIN_CONTEXT_WINDOW_TOKENS, MAX_CONTEXT_WINDOW_TOKENS))
    }

    pub(crate) fn current_thread_context_window_tokens(&self) -> u32 {
        self.current_conversation_agent_profile()
            .context_window_tokens
            .or_else(|| {
                self.chat
                    .active_thread()
                    .and_then(|thread| thread.profile_context_window_tokens)
            })
            .unwrap_or(self.config.context_window_tokens)
            .max(1)
    }

    pub(crate) fn active_thread_context_owner_label(&self) -> String {
        let label = self.current_conversation_agent_profile().agent_label;
        if label.trim().is_empty() {
            "current thread owner".to_string()
        } else {
            label
        }
    }

    pub(crate) fn open_active_thread_context_window_editor(&mut self) {
        let current = self.current_thread_context_window_tokens().to_string();
        let owner_label = self.active_thread_context_owner_label();
        if let Some(pending) = self.active_thread_target_agent_config() {
            self.pending_target_agent_config = Some(pending);
        }
        if self.modal.top() != Some(modal::ModalKind::ContextWindowEditor) {
            self.modal.reduce(modal::ModalAction::Push(
                modal::ModalKind::ContextWindowEditor,
            ));
        }
        self.settings
            .start_editing(CONTEXT_WINDOW_EDIT_FIELD, &current);
        self.status_line = format!("Enter context window tokens for {owner_label}");
    }

    pub(crate) fn context_window_editor_body(&self) -> String {
        let owner_label = self.active_thread_context_owner_label();
        let buffer = self.settings.edit_buffer();
        format!(
            "Pinned to {owner_label}\n\nTokens: {buffer}█\n\nEnter save   Esc cancel\n{MIN_CONTEXT_WINDOW_TOKENS} – {MAX_CONTEXT_WINDOW_TOKENS} tokens"
        )
    }

    pub(crate) fn commit_active_thread_context_window_editor(&mut self) {
        let Some(tokens) = Self::parse_context_window_tokens(self.settings.edit_buffer()) else {
            self.status_line = "Context window must be a number of tokens".to_string();
            return;
        };
        self.apply_active_thread_context_window(tokens);
        self.settings.reduce(SettingsAction::ConfirmEdit);
        self.pending_target_agent_config = None;
        self.close_top_modal();
    }

    pub(crate) fn cancel_active_thread_context_window_editor(&mut self) {
        self.settings.reduce(SettingsAction::CancelEdit);
        self.pending_target_agent_config = None;
        self.close_top_modal();
        self.status_line = "Context window unchanged".to_string();
    }

    pub(crate) fn known_model_context_window(
        &self,
        provider_id: &str,
        model_id: &str,
        picked: Option<u32>,
    ) -> Option<u32> {
        let model_id = model_id.trim();
        picked
            .filter(|tokens| *tokens > 0)
            .or_else(|| {
                if self.model_picker_provider_id.as_deref() != Some(provider_id) {
                    return None;
                }
                self.config
                    .fetched_models()
                    .iter()
                    .find(|entry| entry.id == model_id)
                    .and_then(|entry| entry.context_window)
                    .filter(|tokens| *tokens > 0)
            })
            .or_else(|| {
                let auth_source = if provider_id == self.config.provider {
                    self.config.auth_source.as_str()
                } else {
                    "api_key"
                };
                providers::resolve_context_window_for_provider_auth(
                    provider_id,
                    auth_source,
                    model_id,
                    "",
                )
            })
    }

    pub(crate) fn apply_target_agent_model_context_window(
        &mut self,
        target_agent_id: &str,
        tokens: u32,
    ) {
        let tokens = tokens.clamp(MIN_CONTEXT_WINDOW_TOKENS, MAX_CONTEXT_WINDOW_TOKENS);
        self.send_daemon_command(DaemonCommand::SetTargetAgentContextWindow {
            target_agent_id: target_agent_id.to_string(),
            context_window_tokens: tokens,
        });
        apply_target_agent_context_window_locally(self, target_agent_id, tokens);
    }

    pub(crate) fn send_svarog_context_window_to_daemon(&mut self, tokens: u32) {
        if let Ok(value_json) = serde_json::to_string(&serde_json::json!(tokens)) {
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: "/context_window_tokens".to_string(),
                value_json: value_json.clone(),
            });
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: format!("/providers/{}/context_window_tokens", self.config.provider),
                value_json: value_json.clone(),
            });
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: format!("/{}/context_window_tokens", self.config.provider),
                value_json,
            });
        }
    }

    pub(crate) fn apply_active_thread_context_window(&mut self, tokens: u32) {
        let tokens = tokens.clamp(MIN_CONTEXT_WINDOW_TOKENS, MAX_CONTEXT_WINDOW_TOKENS);
        if let Some(pending) = self.active_thread_target_agent_config() {
            self.send_daemon_command(DaemonCommand::SetTargetAgentContextWindow {
                target_agent_id: pending.target_agent_id.clone(),
                context_window_tokens: tokens,
            });
            apply_target_agent_context_window_locally(self, &pending.target_agent_id, tokens);
            self.status_line = format!("{} context: {tokens} tok", pending.target_agent_name);
            return;
        }

        apply_svarog_context_window_locally(self, tokens);
        if let Ok(value_json) = serde_json::to_string(&serde_json::json!(tokens)) {
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: "/context_window_tokens".to_string(),
                value_json: value_json.clone(),
            });
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: format!("/providers/{}/context_window_tokens", self.config.provider),
                value_json: value_json.clone(),
            });
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: format!("/{}/context_window_tokens", self.config.provider),
                value_json,
            });
        }
        self.status_line = format!(
            "{} context: {tokens} tok",
            self.active_thread_context_owner_label()
        );
    }
}

fn apply_svarog_context_window_locally(model: &mut TuiModel, tokens: u32) {
    model.config.custom_context_window_tokens = Some(tokens);
    model.config.context_window_tokens = tokens;
    if let Some(raw) = model.config.agent_config_raw.as_mut() {
        raw["context_window_tokens"] = serde_json::json!(tokens);
        let provider_id = model.config.provider.clone();
        if raw
            .get("providers")
            .and_then(|value| value.get(&provider_id))
            .is_some()
        {
            raw["providers"][provider_id.as_str()]["context_window_tokens"] =
                serde_json::json!(tokens);
        }
        if raw.get(&provider_id).is_some() {
            raw[provider_id]["context_window_tokens"] = serde_json::json!(tokens);
        }
    }
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.profile_context_window_tokens = Some(tokens);
    }
}

fn apply_target_agent_context_window_locally(
    model: &mut TuiModel,
    target_agent_id: &str,
    tokens: u32,
) {
    for entry in &mut model.subagents.entries {
        let id_matches = entry.id.eq_ignore_ascii_case(target_agent_id)
            || entry
                .id
                .strip_suffix("_builtin")
                .is_some_and(|alias| alias.eq_ignore_ascii_case(target_agent_id));
        if id_matches || entry.name.eq_ignore_ascii_case(target_agent_id) {
            if let Some(raw) = entry.raw_json.as_mut() {
                raw["context_window_tokens"] = serde_json::json!(tokens);
            }
        }
    }
    if let Some(thread) = model.chat.active_thread_mut() {
        thread.profile_context_window_tokens = Some(tokens);
    }
}
