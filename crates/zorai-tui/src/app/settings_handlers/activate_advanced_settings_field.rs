use super::*;
impl TuiModel {
    pub(super) fn activate_advanced_settings_field(&mut self, field: &str) -> bool {
        match field {
            "max_context_messages" => self.settings.start_editing(
                "max_context_messages",
                &self.config.max_context_messages.to_string(),
            ),
            "tui_chat_history_page_size" => self.settings.start_editing(
                "tui_chat_history_page_size",
                &self.config.tui_chat_history_page_size.to_string(),
            ),
            "participant_observer_restore_window_hours" => self.settings.start_editing(
                "participant_observer_restore_window_hours",
                &self
                    .config
                    .participant_observer_restore_window_hours
                    .to_string(),
            ),
            "max_tool_loops" => self
                .settings
                .start_editing("max_tool_loops", &self.config.max_tool_loops.to_string()),
            "max_subagent_budget_extensions" => self.settings.start_editing(
                "max_subagent_budget_extensions",
                &self.config.max_subagent_budget_extensions.to_string(),
            ),
            "max_retries" => self
                .settings
                .start_editing("max_retries", &self.config.max_retries.to_string()),
            "auto_refresh_interval_secs" => self.settings.start_editing(
                "auto_refresh_interval_secs",
                &self.config.auto_refresh_interval_secs.to_string(),
            ),
            "retry_delay_ms" => self
                .settings
                .start_editing("retry_delay_ms", &self.config.retry_delay_ms.to_string()),
            "message_loop_delay_ms" => self.settings.start_editing(
                "message_loop_delay_ms",
                &self.config.message_loop_delay_ms.to_string(),
            ),
            "tool_call_delay_ms" => self.settings.start_editing(
                "tool_call_delay_ms",
                &self.config.tool_call_delay_ms.to_string(),
            ),
            "llm_stream_chunk_timeout_secs" => self.settings.start_editing(
                "llm_stream_chunk_timeout_secs",
                &self.config.llm_stream_chunk_timeout_secs.to_string(),
            ),
            "auto_retry" => {
                self.config.auto_retry = !self.config.auto_retry;
                self.sync_config_to_daemon();
            }
            "workspace_repo_monitor_enabled" => {
                let requested_enabled = !self.config.workspace_repo_monitor_enabled;
                self.sync_workspace_repo_monitor_to_daemon(requested_enabled);
            }
            "workspace_repo_monitor_include_dirs" => self.settings.start_editing(
                "workspace_repo_monitor_include_dirs",
                &self.config.workspace_repo_monitor_include_dirs,
            ),
            "workspace_repo_monitor_exclude_dirs" => self.settings.start_editing(
                "workspace_repo_monitor_exclude_dirs",
                &self.config.workspace_repo_monitor_exclude_dirs,
            ),
            "compact_threshold_pct" => self.settings.start_editing(
                "compact_threshold_pct",
                &self.config.compact_threshold_pct.to_string(),
            ),
            "keep_recent_on_compact" => self.settings.start_editing(
                "keep_recent_on_compact",
                &self.config.keep_recent_on_compact.to_string(),
            ),
            "bash_timeout_secs" => self.settings.start_editing(
                "bash_timeout_secs",
                &self.config.bash_timeout_secs.to_string(),
            ),
            "weles_max_concurrent_reviews" => self.settings.start_editing(
                "weles_max_concurrent_reviews",
                &self.config.weles_max_concurrent_reviews.to_string(),
            ),
            _ => return false,
        }
        true
    }
}
