use super::*;
use crate::providers;
impl TuiModel {
    pub(super) fn activate_compaction_settings_field(&mut self, field: &str) -> bool {
        match field {
            "compaction_strategy" => self.cycle_compaction_strategy(),
            "compaction_weles_provider" => {
                self.open_provider_picker(SettingsPickerTarget::CompactionWelesProvider);
            }
            "compaction_weles_model" => self.open_compaction_weles_model_picker(),
            "compaction_weles_reasoning_effort" => {
                self.settings_picker_target =
                    Some(SettingsPickerTarget::CompactionWelesReasoningEffort);
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
                self.sync_effort_picker_cursor_to_current();
            }
            "compaction_weles_api_transport" => {
                let supported =
                    providers::supported_transports_for(&self.config.compaction_weles_provider);
                let mut options: Vec<&str> = vec![""];
                options.extend_from_slice(supported);
                let current_idx = options
                    .iter()
                    .position(|transport| *transport == self.config.compaction_weles_api_transport)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % options.len().max(1);
                self.config.compaction_weles_api_transport =
                    options.get(next_idx).copied().unwrap_or("").to_string();
                self.sync_config_to_daemon();
            }
            "compaction_custom_provider" => {
                self.open_provider_picker(SettingsPickerTarget::CompactionCustomProvider);
            }
            "compaction_custom_base_url" => self.settings.start_editing(
                "compaction_custom_base_url",
                &self.config.compaction_custom_base_url.clone(),
            ),
            "compaction_custom_auth_source" => {
                let supported =
                    providers::supported_auth_sources_for(&self.config.compaction_custom_provider);
                let current_idx = supported
                    .iter()
                    .position(|source| *source == self.config.compaction_custom_auth_source)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % supported.len().max(1);
                self.config.compaction_custom_auth_source = supported
                    .get(next_idx)
                    .copied()
                    .unwrap_or("api_key")
                    .to_string();
                self.normalize_compaction_custom_transport();
                self.sync_config_to_daemon();
            }
            "compaction_custom_model" => self.open_compaction_custom_model_picker(),
            "compaction_custom_api_transport" => {
                let supported =
                    providers::supported_transports_for(&self.config.compaction_custom_provider);
                let current_idx = supported
                    .iter()
                    .position(|transport| *transport == self.config.compaction_custom_api_transport)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % supported.len().max(1);
                self.config.compaction_custom_api_transport = supported
                    .get(next_idx)
                    .copied()
                    .unwrap_or("chat_completions")
                    .to_string();
                self.normalize_compaction_custom_transport();
                self.sync_config_to_daemon();
            }
            "compaction_custom_api_key" => self.settings.start_editing(
                "compaction_custom_api_key",
                &self.config.compaction_custom_api_key.clone(),
            ),
            "compaction_custom_assistant_id" => self.settings.start_editing(
                "compaction_custom_assistant_id",
                &self.config.compaction_custom_assistant_id.clone(),
            ),
            "compaction_custom_reasoning_effort" => {
                self.settings_picker_target =
                    Some(SettingsPickerTarget::CompactionCustomReasoningEffort);
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
                self.sync_effort_picker_cursor_to_current();
            }
            "compaction_custom_context_window_tokens" => self.settings.start_editing(
                "compaction_custom_context_window_tokens",
                &self
                    .config
                    .compaction_custom_context_window_tokens
                    .to_string(),
            ),
            "snapshot_max_count" => self.settings.start_editing(
                "snapshot_max_count",
                &self.config.snapshot_max_count.to_string(),
            ),
            "snapshot_max_size_mb" => self.settings.start_editing(
                "snapshot_max_size_mb",
                &self.config.snapshot_max_size_mb.to_string(),
            ),
            "snapshot_stats" => {}
            "agent_name" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get("agent_name"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("Zorai")
                    .to_string();
                self.settings.start_editing("agent_name", &current);
            }
            "system_prompt" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get("system_prompt"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                self.settings.start_editing("system_prompt", &current);
            }
            _ => return false,
        }
        true
    }
}
