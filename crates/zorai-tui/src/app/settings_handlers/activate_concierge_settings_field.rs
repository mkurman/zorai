use super::*;
impl TuiModel {
    pub(super) fn activate_concierge_settings_field(&mut self, field: &str) -> bool {
        match field {
            "subagent_list" => {
                self.subagents.actions_focused = false;
            }
            "concierge_enabled" => {
                self.concierge.enabled = !self.concierge.enabled;
                self.send_concierge_config();
            }
            "concierge_detail_level" => {
                let levels = [
                    "minimal",
                    "context_summary",
                    "proactive_triage",
                    "daily_briefing",
                ];
                let current_idx = levels
                    .iter()
                    .position(|level| *level == self.concierge.detail_level)
                    .unwrap_or(0);
                self.concierge.detail_level = levels[(current_idx + 1) % levels.len()].to_string();
                self.send_concierge_config();
            }
            "concierge_provider" => {
                self.open_provider_picker(SettingsPickerTarget::ConciergeProvider);
            }
            "concierge_model" => {
                let provider_id = self
                    .concierge
                    .provider
                    .clone()
                    .unwrap_or_else(|| self.config.provider.clone());
                let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
                self.open_provider_backed_model_picker(
                    SettingsPickerTarget::ConciergeModel,
                    provider_id,
                    base_url,
                    api_key,
                    auth_source,
                );
            }
            "concierge_reasoning_effort" => {
                self.settings_picker_target = Some(SettingsPickerTarget::ConciergeReasoningEffort);
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
                self.sync_effort_picker_cursor_to_current();
            }
            "concierge_api_transport" => {
                let provider_id = self
                    .concierge
                    .provider
                    .clone()
                    .unwrap_or_else(|| self.config.provider.clone());
                let supported = crate::providers::supported_transports_for(&provider_id);
                let mut options: Vec<&str> = vec![""];
                options.extend_from_slice(supported);
                let current = self.concierge.api_transport.clone().unwrap_or_default();
                let current_idx = options
                    .iter()
                    .position(|transport| *transport == current)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % options.len().max(1);
                let next = options.get(next_idx).copied().unwrap_or("");
                self.concierge.api_transport = if next.is_empty() {
                    None
                } else {
                    Some(next.to_string())
                };
                self.send_concierge_config();
            }
            "concierge_claude_permission_mode" => {
                let options = crate::state::subagents::CLAUDE_PERMISSION_MODE_OPTIONS;
                let current = self
                    .concierge
                    .claude_permission_mode
                    .clone()
                    .unwrap_or_default();
                let current_idx = options
                    .iter()
                    .position(|mode| *mode == current)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % options.len().max(1);
                let next = options.get(next_idx).copied().unwrap_or("");
                self.concierge.claude_permission_mode = if next.is_empty() {
                    None
                } else {
                    Some(next.to_string())
                };
                self.send_concierge_config();
            }
            "concierge_openrouter_provider_order" => self
                .open_concierge_openrouter_provider_picker(
                    SettingsPickerTarget::ConciergeOpenRouterPreferredProviders,
                ),
            "concierge_openrouter_provider_ignore" => self
                .open_concierge_openrouter_provider_picker(
                    SettingsPickerTarget::ConciergeOpenRouterExcludedProviders,
                ),
            "concierge_openrouter_allow_fallbacks" => {
                if self.concierge.provider.as_deref() == Some(PROVIDER_ID_OPENROUTER) {
                    self.concierge.openrouter_allow_fallbacks =
                        !self.concierge.openrouter_allow_fallbacks;
                    self.send_concierge_config();
                } else {
                    self.status_line =
                        "OpenRouter provider routing only applies to OpenRouter agents".to_string();
                }
            }
            "concierge_base_url" => {
                self.settings
                    .start_editing("concierge_base_url", &self.concierge.base_url);
                self.status_line =
                    "Enter Rarog base URL override; empty uses the provider default".to_string();
            }
            "concierge_huggingface_provider" => {
                if self.concierge.provider.as_deref()
                    == Some(zorai_shared::providers::PROVIDER_ID_HUGGINGFACE)
                {
                    self.settings.start_editing(
                        "concierge_huggingface_provider",
                        &self.concierge.huggingface_provider,
                    );
                    self.status_line =
                        "Enter HF route: fastest, cheapest, preferred, or provider slug"
                            .to_string();
                } else {
                    self.status_line =
                        "Hugging Face routing only applies to Hugging Face agents".to_string();
                }
            }
            "managed_security_level" => {
                let levels = ["highest", "moderate", "lowest", "yolo"];
                let current_idx = levels
                    .iter()
                    .position(|level| *level == self.config.managed_security_level)
                    .unwrap_or(2);
                self.config.managed_security_level =
                    levels[(current_idx + 1) % levels.len()].to_string();
                self.sync_config_to_daemon();
            }
            _ => return false,
        }
        true
    }
}
