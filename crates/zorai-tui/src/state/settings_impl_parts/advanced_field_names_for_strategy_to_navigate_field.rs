use super::*;
use crate::state::config::ConfigState;
use zorai_shared::providers::{PROVIDER_ID_HUGGINGFACE, PROVIDER_ID_OPENROUTER};
impl SettingsState {
    pub fn new() -> Self {
        Self {
            active_tab: SettingsTab::Auth,
            field_cursor: 0,
            editing_field: None,
            edit_buffer: String::new(),
            edit_cursor: 0,
            textarea_mode: false,
            dropdown_open: false,
            dropdown_cursor: 0,
            dirty: false,
        }
    }

    pub fn active_tab(&self) -> SettingsTab {
        self.active_tab
    }

    pub fn field_cursor(&self) -> usize {
        self.field_cursor
    }

    pub fn editing_field(&self) -> Option<&str> {
        self.editing_field.as_deref()
    }

    pub fn is_editing(&self) -> bool {
        self.editing_field.is_some()
    }

    pub fn edit_buffer(&self) -> &str {
        &self.edit_buffer
    }

    pub fn edit_cursor(&self) -> usize {
        self.edit_cursor
    }

    pub fn edit_cursor_line_col(&self) -> (usize, usize) {
        let before = &self.edit_buffer[..self.edit_cursor.min(self.edit_buffer.len())];
        let line = before.matches('\n').count();
        let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        (line, before[last_newline..].chars().count())
    }

    pub fn is_dropdown_open(&self) -> bool {
        self.dropdown_open
    }

    pub fn dropdown_cursor(&self) -> usize {
        self.dropdown_cursor
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn editing_textarea(&self) -> Option<(&str, &str)> {
        if !self.textarea_mode {
            return None;
        }
        self.editing_field
            .as_deref()
            .map(|field| (field, self.edit_buffer.as_str()))
    }

    pub fn is_textarea(&self) -> bool {
        self.textarea_mode
    }

    /// Start inline editing for a field, pre-populated with its current value.
    pub fn start_editing(&mut self, field: &str, current_value: &str) {
        self.editing_field = Some(field.to_string());
        self.edit_buffer = current_value.to_string();
        self.edit_cursor = self.edit_buffer.len();
        self.textarea_mode = field_uses_textarea(field);
    }

    /// Map `field_cursor` to the field name for the active tab.
    pub fn current_field_name(&self) -> &str {
        match self.active_tab {
            SettingsTab::Provider => match self.field_cursor {
                0 => "provider",
                1 => "base_url",
                2 => "auth_source",
                3 => "model",
                4 => "api_transport",
                5 => "assistant_id",
                6 => "reasoning_effort",
                7 => "context_window_tokens",
                8 => "openrouter_provider_order",
                9 => "openrouter_provider_ignore",
                10 => "openrouter_allow_fallbacks",
                11 => "openrouter_response_cache_enabled",
                _ => "",
            },
            SettingsTab::Tools
            | SettingsTab::Skills
            | SettingsTab::WebSearch
            | SettingsTab::Media
            | SettingsTab::Memory
            | SettingsTab::Heartbeat
            | SettingsTab::Proactivity
            | SettingsTab::Workflows
            | SettingsTab::Chat
            | SettingsTab::Compaction
            | SettingsTab::Execution
            | SettingsTab::Safety
            | SettingsTab::Storage => {
                crate::state::settings_registry::settings_page_spec(self.active_tab)
                    .and_then(|page| page.all_fields().nth(self.field_cursor))
                    .map(|spec| spec.name)
                    .unwrap_or("")
            }
            SettingsTab::Mlflow => match self.field_cursor {
                0 => "mlflow_enabled",
                1 => "mlflow_visible_operator",
                2 => "mlflow_gateway",
                3 => "mlflow_goal_task",
                4 => "mlflow_subagent",
                5 => "mlflow_concierge",
                6 => "mlflow_heartbeat_autonomous",
                7 => "mlflow_tracking_uri",
                8 => "mlflow_experiment_name",
                9 => "mlflow_experiment_id",
                10 => "mlflow_capture_mode",
                11 => "mlflow_test_connection",
                12 => "mlflow_send_test_trace",
                _ => "",
            },
            SettingsTab::Gateway => match self.field_cursor {
                0 => "gateway_enabled",
                1 => "gateway_default_agent",
                2 => "gateway_prefix",
                3 => "slack_token",
                4 => "slack_channel_filter",
                5 => "slack_default_agent",
                6 => "telegram_token",
                7 => "telegram_allowed_chats",
                8 => "telegram_default_agent",
                9 => "discord_token",
                10 => "discord_channel_filter",
                11 => "discord_allowed_users",
                12 => "discord_default_agent",
                13 => "whatsapp_allowed_contacts",
                14 => "whatsapp_token",
                15 => "whatsapp_phone_id",
                16 => "whatsapp_default_agent",
                17 => "whatsapp_link_device",
                18 => "whatsapp_relink_device",
                _ => "",
            },
            SettingsTab::Auth => match self.field_cursor {
                0 => "auth_provider_list",
                _ => "",
            },
            SettingsTab::Agent => match self.field_cursor {
                0 => "provider",
                1 => "base_url",
                2 => "auth_source",
                3 => "model",
                4 => "api_transport",
                5 => "assistant_id",
                6 => "reasoning_effort",
                7 => "context_window_tokens",
                8 => "swarog_alias",
                9 => "system_prompt",
                10 => "backend",
                11 => "claude_permission_mode",
                _ => "",
            },
            SettingsTab::SubAgents => match self.field_cursor {
                0 => "subagent_list",
                _ => "",
            },
            SettingsTab::Concierge => match self.field_cursor {
                0 => "concierge_enabled",
                1 => "concierge_detail_level",
                2 => "concierge_provider",
                3 => "concierge_model",
                4 => "concierge_reasoning_effort",
                5 => "concierge_api_transport",
                6 => "concierge_claude_permission_mode",
                7 => "concierge_base_url",
                8 => "rarog_alias",
                _ => "",
            },
            SettingsTab::Plugins => "plugin_field",
            SettingsTab::Database => match self.field_cursor {
                0 => "db_sync_now",
                _ => "",
            },
            SettingsTab::About => "",
        }
    }

    pub fn provider_extra_field_names(provider: &str) -> &'static [&'static str] {
        if provider == PROVIDER_ID_OPENROUTER {
            &[
                "openrouter_provider_order",
                "openrouter_provider_ignore",
                "openrouter_allow_fallbacks",
                "openrouter_response_cache_enabled",
            ]
        } else if provider == PROVIDER_ID_HUGGINGFACE {
            &["huggingface_provider"]
        } else {
            &[]
        }
    }

    pub fn current_field_name_with_config<'a>(&'a self, config: &'a ConfigState) -> &'a str {
        if let Some(page) = crate::state::settings_registry::settings_page_spec(self.active_tab) {
            return page
                .visible_fields(config)
                .get(self.field_cursor)
                .map(|spec| spec.name)
                .unwrap_or("");
        }
        if self.active_tab == SettingsTab::Agent {
            let extras = Self::provider_extra_field_names(&config.provider);
            return match self.field_cursor {
                0..=7 => self.current_field_name(),
                cursor if cursor < 8 + extras.len() => extras[cursor - 8],
                cursor => match cursor - extras.len() {
                    8 => "swarog_alias",
                    9 => "system_prompt",
                    10 => "backend",
                    11 => "claude_permission_mode",
                    _ => "",
                },
            };
        }
        if self.active_tab == SettingsTab::Provider && config.provider == PROVIDER_ID_HUGGINGFACE {
            return match self.field_cursor {
                0 => "provider",
                1 => "base_url",
                2 => "auth_source",
                3 => "model",
                4 => "api_transport",
                5 => "assistant_id",
                6 => "reasoning_effort",
                7 => "context_window_tokens",
                8 => "huggingface_provider",
                _ => "",
            };
        }
        if self.active_tab == SettingsTab::Provider && config.provider != PROVIDER_ID_OPENROUTER {
            return match self.field_cursor {
                0 => "provider",
                1 => "base_url",
                2 => "auth_source",
                3 => "model",
                4 => "api_transport",
                5 => "assistant_id",
                6 => "reasoning_effort",
                7 => "context_window_tokens",
                _ => "",
            };
        }
        self.current_field_name()
    }

    /// Number of navigable fields in the current tab (for cursor clamping).
    pub fn field_count(&self) -> usize {
        match self.active_tab {
            SettingsTab::Provider => 8,
            SettingsTab::Tools
            | SettingsTab::Skills
            | SettingsTab::WebSearch
            | SettingsTab::Media
            | SettingsTab::Memory
            | SettingsTab::Heartbeat
            | SettingsTab::Proactivity
            | SettingsTab::Workflows
            | SettingsTab::Chat
            | SettingsTab::Compaction
            | SettingsTab::Execution
            | SettingsTab::Safety
            | SettingsTab::Storage => {
                crate::state::settings_registry::settings_page_spec(self.active_tab)
                    .map(|page| page.all_fields().count())
                    .unwrap_or(0)
            }
            SettingsTab::Mlflow => 13,
            SettingsTab::Gateway => 19,
            SettingsTab::Auth => 1,
            SettingsTab::Agent => 12,
            SettingsTab::SubAgents => 1,
            SettingsTab::Concierge => 9,
            SettingsTab::Plugins => 1,
            SettingsTab::Database => 1,
            SettingsTab::About => 0,
        }
    }

    pub fn field_count_with_config(&self, config: &ConfigState) -> usize {
        if let Some(page) = crate::state::settings_registry::settings_page_spec(self.active_tab) {
            return page.visible_fields(config).len();
        }
        if self.active_tab == SettingsTab::Provider && config.provider == PROVIDER_ID_OPENROUTER {
            return 12;
        }
        if self.active_tab == SettingsTab::Provider && config.provider == PROVIDER_ID_HUGGINGFACE {
            return 9;
        }
        if self.active_tab == SettingsTab::Agent {
            return 12 + Self::provider_extra_field_names(&config.provider).len();
        }
        if self.active_tab == SettingsTab::Provider && config.provider != PROVIDER_ID_OPENROUTER {
            return 8;
        }
        self.field_count()
    }

    pub fn clamp_field_cursor(&mut self, field_count: usize) {
        if field_count == 0 {
            self.field_cursor = 0;
        } else {
            self.field_cursor = self.field_cursor.min(field_count.saturating_sub(1));
        }
    }

    /// Navigate fields within `field_count` items; clamps at both ends.
    pub fn navigate_field(&mut self, delta: i32, field_count: usize) {
        if field_count == 0 {
            self.field_cursor = 0;
            return;
        }
        if delta > 0 {
            self.field_cursor =
                (self.field_cursor + delta as usize).min(field_count.saturating_sub(1));
        } else {
            self.field_cursor = self.field_cursor.saturating_sub((-delta) as usize);
        }
    }
}
