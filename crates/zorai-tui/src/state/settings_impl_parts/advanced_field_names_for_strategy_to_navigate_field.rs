use super::*;
use crate::state::config::ConfigState;
use zorai_shared::providers::{PROVIDER_ID_HUGGINGFACE, PROVIDER_ID_OPENROUTER};
impl SettingsState {
    fn advanced_field_names_for_strategy(strategy: &str) -> &'static [&'static str] {
        const HEURISTIC_FIELDS: &[&str] = &[
            "managed_sandbox_enabled",
            "managed_security_level",
            "auto_compact_context",
            "compaction_strategy",
            "max_context_messages",
            "max_tool_loops",
            "max_retries",
            "retry_delay_ms",
            "message_loop_delay_ms",
            "tool_call_delay_ms",
            "llm_stream_chunk_timeout_secs",
            "auto_retry",
            "context_window_tokens",
            "compact_threshold_pct",
            "keep_recent_on_compact",
            "bash_timeout_secs",
            "weles_max_concurrent_reviews",
            "snapshot_auto_cleanup",
            "snapshot_max_count",
            "snapshot_max_size_mb",
            "snapshot_stats",
            "auto_refresh_interval_secs",
            "workspace_repo_monitor_enabled",
            "workspace_repo_monitor_include_dirs",
            "workspace_repo_monitor_exclude_dirs",
        ];
        const WELES_FIELDS: &[&str] = &[
            "managed_sandbox_enabled",
            "managed_security_level",
            "auto_compact_context",
            "compaction_strategy",
            "max_context_messages",
            "max_tool_loops",
            "max_retries",
            "retry_delay_ms",
            "message_loop_delay_ms",
            "tool_call_delay_ms",
            "llm_stream_chunk_timeout_secs",
            "auto_retry",
            "context_window_tokens",
            "compact_threshold_pct",
            "keep_recent_on_compact",
            "bash_timeout_secs",
            "weles_max_concurrent_reviews",
            "compaction_weles_provider",
            "compaction_weles_model",
            "compaction_weles_reasoning_effort",
            "compaction_weles_api_transport",
            "snapshot_auto_cleanup",
            "snapshot_max_count",
            "snapshot_max_size_mb",
            "snapshot_stats",
            "auto_refresh_interval_secs",
            "workspace_repo_monitor_enabled",
            "workspace_repo_monitor_include_dirs",
            "workspace_repo_monitor_exclude_dirs",
        ];
        const CUSTOM_FIELDS: &[&str] = &[
            "managed_sandbox_enabled",
            "managed_security_level",
            "auto_compact_context",
            "compaction_strategy",
            "max_context_messages",
            "max_tool_loops",
            "max_retries",
            "retry_delay_ms",
            "message_loop_delay_ms",
            "tool_call_delay_ms",
            "llm_stream_chunk_timeout_secs",
            "auto_retry",
            "context_window_tokens",
            "compact_threshold_pct",
            "keep_recent_on_compact",
            "bash_timeout_secs",
            "weles_max_concurrent_reviews",
            "compaction_custom_provider",
            "compaction_custom_base_url",
            "compaction_custom_auth_source",
            "compaction_custom_model",
            "compaction_custom_api_transport",
            "compaction_custom_api_key",
            "compaction_custom_assistant_id",
            "compaction_custom_reasoning_effort",
            "compaction_custom_context_window_tokens",
            "snapshot_auto_cleanup",
            "snapshot_max_count",
            "snapshot_max_size_mb",
            "snapshot_stats",
            "auto_refresh_interval_secs",
            "workspace_repo_monitor_enabled",
            "workspace_repo_monitor_include_dirs",
            "workspace_repo_monitor_exclude_dirs",
        ];

        match strategy {
            "weles" => WELES_FIELDS,
            "custom_model" => CUSTOM_FIELDS,
            _ => HEURISTIC_FIELDS,
        }
    }

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
            SettingsTab::Tools => match self.field_cursor {
                0 => "tool_bash",
                1 => "tool_file_ops",
                2 => "tool_web_search",
                3 => "tool_web_browse",
                4 => "tool_vision",
                5 => "tool_system_info",
                6 => "tool_gateway",
                _ => "",
            },
            SettingsTab::WebSearch => match self.field_cursor {
                0 => "web_search_enabled",
                1 => "search_provider",
                2 => "duckduckgo_region",
                3 => "duckduckgo_safe_search",
                4 => "firecrawl_api_key",
                5 => "exa_api_key",
                6 => "tavily_api_key",
                7 => "search_max_results",
                8 => "search_timeout",
                9 => "browse_provider",
                _ => "",
            },
            SettingsTab::Chat => match self.field_cursor {
                0 => "enable_streaming",
                1 => "enable_conversation_memory",
                2 => "enable_honcho_memory",
                3 => "anticipatory_enabled",
                4 => "anticipatory_morning_brief",
                5 => "anticipatory_predictive_hydration",
                6 => "anticipatory_stuck_detection",
                7 => "operator_model_enabled",
                8 => "operator_model_allow_message_statistics",
                9 => "operator_model_allow_approval_learning",
                10 => "operator_model_allow_attention_tracking",
                11 => "operator_model_allow_implicit_feedback",
                12 => "collaboration_enabled",
                13 => "compliance_mode",
                14 => "compliance_retention_days",
                15 => "compliance_sign_all_events",
                16 => "tool_synthesis_enabled",
                17 => "tool_synthesis_require_activation",
                18 => "tool_synthesis_max_generated_tools",
                19 => "tui_chat_history_page_size",
                20 => "participant_observer_restore_window_hours",
                21 => "operator_model_inspect",
                22 => "operator_model_reset",
                23 => "collaboration_sessions_inspect",
                24 => "generated_tools_inspect",
                _ => "",
            },
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
            SettingsTab::Features => match self.field_cursor {
                0 => "feat_tier_override",
                1 => "feat_security_level",
                2 => "feat_heartbeat_cron",
                3 => "feat_heartbeat_quiet_start",
                4 => "feat_heartbeat_quiet_end",
                5 => "feat_check_stale_todos",
                6 => "feat_check_stuck_goals",
                7 => "feat_check_unreplied_messages",
                8 => "feat_check_repo_changes",
                9 => "feat_consolidation_enabled",
                10 => "feat_decay_half_life_hours",
                11 => "feat_heuristic_promotion_threshold",
                12 => "feat_skill_recommendation_enabled",
                13 => "feat_skill_background_community_search",
                14 => "feat_skill_community_preapprove_timeout_secs",
                15 => "feat_skill_suggest_global_enable_after_approvals",
                16 => "feat_audio_stt_enabled",
                17 => "feat_audio_stt_provider",
                18 => "feat_audio_stt_model",
                19 => "feat_audio_tts_enabled",
                20 => "feat_audio_tts_provider",
                21 => "feat_audio_tts_model",
                22 => "feat_audio_tts_voice",
                23 => "feat_image_generation_provider",
                24 => "feat_image_generation_model",
                25 => "feat_embedding_enabled",
                26 => "feat_embedding_provider",
                27 => "feat_embedding_model",
                28 => "feat_embedding_dimensions",
                29 => "feat_auto_thread_title",
                _ => "",
            },
            SettingsTab::Advanced => match self.field_cursor {
                0 => "managed_sandbox_enabled",
                1 => "managed_security_level",
                2 => "auto_compact_context",
                3 => "compaction_strategy",
                4 => "max_context_messages",
                5 => "max_tool_loops",
                6 => "max_retries",
                7 => "retry_delay_ms",
                8 => "message_loop_delay_ms",
                9 => "tool_call_delay_ms",
                10 => "llm_stream_chunk_timeout_secs",
                11 => "auto_retry",
                12 => "context_window_tokens",
                13 => "compact_threshold_pct",
                14 => "keep_recent_on_compact",
                15 => "bash_timeout_secs",
                16 => "weles_max_concurrent_reviews",
                17 => "snapshot_auto_cleanup",
                18 => "snapshot_max_count",
                19 => "snapshot_max_size_mb",
                20 => "snapshot_stats",
                21 => "auto_refresh_interval_secs",
                22 => "workspace_repo_monitor_enabled",
                23 => "workspace_repo_monitor_include_dirs",
                24 => "workspace_repo_monitor_exclude_dirs",
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

    pub fn current_field_name_with_config<'a>(&'a self, config: &'a ConfigState) -> &'a str {
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
                12 => "huggingface_provider",
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
        if self.active_tab == SettingsTab::Advanced {
            return Self::advanced_field_names_for_strategy(&config.compaction_strategy)
                .get(self.field_cursor)
                .copied()
                .unwrap_or("snapshot_stats");
        }
        self.current_field_name()
    }

    /// Number of navigable fields in the current tab (for cursor clamping).
    pub fn field_count(&self) -> usize {
        match self.active_tab {
            SettingsTab::Provider => 8,
            SettingsTab::Tools => 7,
            SettingsTab::WebSearch => 10,
            SettingsTab::Chat => 25,
            SettingsTab::Mlflow => 13,
            SettingsTab::Gateway => 19,
            SettingsTab::Auth => 1,
            SettingsTab::Agent => 12,
            SettingsTab::SubAgents => 1,
            SettingsTab::Concierge => 9,
            SettingsTab::Features => 30,
            SettingsTab::Advanced => 25,
            SettingsTab::Plugins => 1,
            SettingsTab::Database => 1,
            SettingsTab::About => 0,
        }
    }

    pub fn field_count_with_config(&self, config: &ConfigState) -> usize {
        if self.active_tab == SettingsTab::Provider && config.provider == PROVIDER_ID_OPENROUTER {
            return 12;
        }
        if self.active_tab == SettingsTab::Provider && config.provider == PROVIDER_ID_HUGGINGFACE {
            return 13;
        }
        if self.active_tab == SettingsTab::Provider && config.provider != PROVIDER_ID_OPENROUTER {
            return 8;
        }
        if self.active_tab == SettingsTab::Advanced {
            return Self::advanced_field_names_for_strategy(&config.compaction_strategy).len();
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
