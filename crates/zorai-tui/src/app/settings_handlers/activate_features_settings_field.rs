use super::*;
impl TuiModel {
    pub(super) fn activate_features_settings_field(&mut self, field: &str) -> bool {
        match field {
            "search_provider" => {
                let next = match self.config.search_provider.as_str() {
                    "none" | "" => "firecrawl",
                    "firecrawl" => "duckduckgo",
                    "duckduckgo" | "ddg" => "exa",
                    "exa" => "tavily",
                    "tavily" => "keenable",
                    "keenable" => "youcom",
                    _ => "none",
                };
                self.config.search_provider = next.to_string();
                self.sync_config_to_daemon();
            }
            "duckduckgo_region" => self
                .settings
                .start_editing("duckduckgo_region", &self.config.duckduckgo_region.clone()),
            "duckduckgo_safe_search" => {
                let next = match self.config.duckduckgo_safe_search.as_str() {
                    "off" => "moderate",
                    "moderate" | "" => "strict",
                    "strict" => "off",
                    _ => "moderate",
                };
                self.config.duckduckgo_safe_search = next.to_string();
                self.sync_config_to_daemon();
            }
            "firecrawl_api_key" => self
                .settings
                .start_editing("firecrawl_api_key", &self.config.firecrawl_api_key.clone()),
            "exa_api_key" => self
                .settings
                .start_editing("exa_api_key", &self.config.exa_api_key.clone()),
            "tavily_api_key" => self
                .settings
                .start_editing("tavily_api_key", &self.config.tavily_api_key.clone()),
            "keenable_api_key" => self
                .settings
                .start_editing("keenable_api_key", &self.config.keenable_api_key.clone()),
            "youcom_api_key" => self
                .settings
                .start_editing("youcom_api_key", &self.config.youcom_api_key.clone()),
            "search_max_results" => self.settings.start_editing(
                "search_max_results",
                &self.config.search_max_results.to_string(),
            ),
            "search_timeout" => self.settings.start_editing(
                "search_timeout",
                &self.config.search_timeout_secs.to_string(),
            ),
            "browse_provider" => {
                let next = match self.config.browse_provider.as_str() {
                    "auto" | "" => "lightpanda",
                    "lightpanda" => "chrome",
                    "chrome" => "none",
                    _ => "auto",
                };
                self.config.browse_provider = next.to_string();
                self.sync_config_to_daemon();
            }
            "enable_honcho_memory" => {
                self.open_honcho_editor();
            }
            "honcho_api_key" => self
                .settings
                .start_editing("honcho_api_key", &self.config.honcho_api_key.clone()),
            "honcho_base_url" => self
                .settings
                .start_editing("honcho_base_url", &self.config.honcho_base_url.clone()),
            "honcho_workspace_id" => self.settings.start_editing(
                "honcho_workspace_id",
                &self.config.honcho_workspace_id.clone(),
            ),
            "operator_model_inspect" => {
                self.send_daemon_command(DaemonCommand::GetOperatorModel);
                self.status_line = "Loading operator model snapshot".to_string();
            }
            "operator_model_reset" => {
                self.send_daemon_command(DaemonCommand::ResetOperatorModel);
                self.status_line = "Resetting operator model".to_string();
            }
            "collaboration_sessions_inspect" => {
                self.main_pane_view = MainPaneView::Collaboration;
                self.focus = FocusArea::Chat;
                self.send_daemon_command(DaemonCommand::GetCollaborationSessions);
                self.status_line = "Loading collaboration sessions".to_string();
            }
            "generated_tools_inspect" => {
                self.send_daemon_command(DaemonCommand::GetGeneratedTools);
                self.status_line = "Loading generated tools".to_string();
            }
            "compliance_mode" => {
                let next = match self.config.compliance_mode.as_str() {
                    "standard" => "soc2",
                    "soc2" => "hipaa",
                    "hipaa" => "fedramp",
                    _ => "standard",
                };
                self.config.compliance_mode = next.to_string();
                self.sync_config_to_daemon();
            }
            "compliance_retention_days" => self.settings.start_editing(
                "compliance_retention_days",
                &self.config.compliance_retention_days.to_string(),
            ),
            "tool_synthesis_max_generated_tools" => self.settings.start_editing(
                "tool_synthesis_max_generated_tools",
                &self.config.tool_synthesis_max_generated_tools.to_string(),
            ),
            "context_window_tokens" => self.settings.start_editing(
                "context_window_tokens",
                &self
                    .config
                    .custom_context_window_tokens
                    .unwrap_or(self.config.context_window_tokens)
                    .to_string(),
            ),
            _ => return false,
        }
        true
    }
}
