use super::*;

impl TuiModel {
    pub(in crate::app) fn handle_auth_agents_plugins_status_client_event(
        &mut self,
        event: ClientEvent,
    ) -> Option<ClientEvent> {
        match event {
            ClientEvent::ProviderAuthStates(entries) => {
                self.handle_provider_auth_states_event(entries);
                None
            }
            ClientEvent::OpenAICodexAuthStatus(status) => {
                self.handle_openai_codex_auth_status_event(status);
                None
            }
            ClientEvent::OpenAICodexAuthLoginResult(status) => {
                self.handle_openai_codex_auth_login_result_event(status);
                None
            }
            ClientEvent::OpenAICodexAuthLogoutResult { ok, error } => {
                self.handle_openai_codex_auth_logout_result_event(ok, error);
                None
            }
            ClientEvent::ProviderValidation {
                provider_id,
                valid,
                error,
            } => {
                self.handle_provider_validation_event(provider_id, valid, error);
                None
            }
            ClientEvent::SubAgentList(entries) => {
                self.handle_subagent_list_event(entries);
                None
            }
            ClientEvent::SubAgentUpdated(entry) => {
                self.handle_subagent_updated_event(entry);
                None
            }
            ClientEvent::SubAgentRemoved { sub_agent_id } => {
                self.handle_subagent_removed_event(sub_agent_id);
                None
            }
            ClientEvent::ConciergeConfig(raw) => {
                self.handle_concierge_config_event(raw);
                None
            }
            ClientEvent::ConciergeWelcome { content, actions } => {
                self.handle_concierge_welcome_event(content, actions);
                None
            }
            ClientEvent::ConciergeWelcomeDismissed => {
                self.handle_concierge_welcome_dismissed_event();
                None
            }
            ClientEvent::OperatorProfileSessionStarted { session_id, kind } => {
                self.handle_operator_profile_session_started_event(session_id, kind);
                None
            }
            ClientEvent::OperatorProfileQuestion {
                session_id,
                question_id,
                field_key,
                prompt,
                input_kind,
                optional,
            } => {
                self.handle_operator_profile_question_event(
                    session_id,
                    question_id,
                    field_key,
                    prompt,
                    input_kind,
                    optional,
                );
                None
            }
            ClientEvent::OperatorQuestion {
                question_id,
                content,
                options,
                session_id: _,
                thread_id,
            } => {
                self.handle_operator_question_event(question_id, content, options, thread_id);
                None
            }
            ClientEvent::OperatorQuestionResolved {
                question_id,
                answer,
            } => {
                self.handle_operator_question_resolved_event(question_id, answer);
                None
            }
            ClientEvent::OperatorProfileProgress {
                session_id,
                answered,
                remaining,
                completion_ratio,
            } => {
                self.handle_operator_profile_progress_event(
                    session_id,
                    answered,
                    remaining,
                    completion_ratio,
                );
                None
            }
            ClientEvent::OperatorProfileSummary { summary_json } => {
                self.handle_operator_profile_summary_event(summary_json);
                None
            }
            ClientEvent::OperatorModelSummary { model_json } => {
                self.handle_operator_model_summary_event(model_json);
                None
            }
            ClientEvent::OperatorModelReset { ok } => {
                self.handle_operator_model_reset_event(ok);
                None
            }
            ClientEvent::CollaborationSessions { sessions_json } => {
                self.handle_collaboration_sessions_event(sessions_json);
                None
            }
            ClientEvent::CollaborationVoteResult { report_json } => {
                self.handle_collaboration_vote_result_event(report_json);
                None
            }
            ClientEvent::GeneratedTools { tools_json } => {
                self.handle_generated_tools_event(tools_json);
                None
            }
            ClientEvent::OperatorProfileSessionCompleted {
                session_id,
                updated_fields,
            } => {
                self.handle_operator_profile_session_completed_event(session_id, updated_fields);
                None
            }
            ClientEvent::PluginList(plugins) => {
                self.handle_plugin_list_event(plugins);
                None
            }
            ClientEvent::PluginGet {
                plugin: _,
                settings_schema,
            } => {
                self.handle_plugin_get_event(settings_schema);
                None
            }
            ClientEvent::PluginSettings {
                plugin_name: _,
                settings,
            } => {
                self.handle_plugin_settings_event(settings);
                None
            }
            ClientEvent::PluginTestConnection {
                plugin_name: _,
                success,
                message,
            } => {
                self.handle_plugin_test_connection_event(success, message);
                None
            }
            ClientEvent::PluginAction { success, message } => {
                self.handle_plugin_action_event(success, message);
                None
            }
            ClientEvent::PluginCommands(commands) => {
                self.handle_plugin_commands_event(commands);
                None
            }
            ClientEvent::PluginOAuthUrl { name, url } => {
                self.handle_plugin_oauth_url_event(name, url);
                None
            }
            ClientEvent::PluginOAuthComplete {
                name,
                success,
                error,
            } => {
                self.handle_plugin_oauth_complete_event(name, success, error);
                None
            }
            ClientEvent::NotificationSnapshot(notifications) => {
                self.handle_notification_snapshot_event(notifications);
                None
            }
            ClientEvent::NotificationUpsert(notification) => {
                self.handle_notification_upsert_event(notification);
                None
            }
            ClientEvent::Error(message) => {
                if !self.handle_gateway_link_error(&message) {
                    self.handle_error_event(message);
                }
                None
            }
            ClientEvent::RetryStatus {
                thread_id,
                phase,
                attempt,
                max_retries,
                delay_ms,
                failure_class,
                message,
            } => {
                self.handle_retry_status_event(
                    thread_id,
                    phase,
                    attempt,
                    max_retries,
                    delay_ms,
                    failure_class,
                    message,
                );
                None
            }
            ClientEvent::WorkflowNotice {
                thread_id,
                kind,
                message,
                details,
            } => {
                self.handle_workflow_notice_event(thread_id, kind, message, details);
                None
            }
            ClientEvent::WelesHealthUpdate {
                state,
                reason,
                checked_at,
            } => {
                self.handle_weles_health_update_event(state, reason, checked_at);
                None
            }
            ClientEvent::StatusDiagnostics {
                operator_profile_sync_state,
                operator_profile_sync_dirty,
                operator_profile_scheduler_fallback,
                diagnostics_json,
            } => {
                self.handle_status_diagnostics_event(
                    operator_profile_sync_state,
                    operator_profile_sync_dirty,
                    operator_profile_scheduler_fallback,
                    diagnostics_json,
                );
                None
            }
            ClientEvent::StatusSnapshot(snapshot) => {
                self.handle_status_snapshot_event(snapshot);
                None
            }
            ClientEvent::StatisticsSnapshot(snapshot) => {
                self.handle_statistics_snapshot_event(snapshot);
                None
            }
            ClientEvent::PromptInspection(prompt) => {
                self.handle_prompt_inspection_event(prompt);
                None
            }
            ClientEvent::AgentExplanation(payload) => {
                self.handle_agent_explanation_event(payload);
                None
            }
            ClientEvent::DivergentSessionStarted(payload) => {
                self.handle_divergent_session_started_event(payload);
                None
            }
            ClientEvent::DivergentSession(payload) => {
                self.handle_divergent_session_event(payload);
                None
            }
            ClientEvent::DatabaseSyncResult { ok, message } => {
                self.status_line = if ok {
                    message
                } else {
                    format!("Database sync failed: {message}")
                };
                None
            }
            ClientEvent::DatabaseBackendState {
                backend,
                sync_url,
                has_token,
                seeded_at,
            } => {
                self.config.db_backend = backend.unwrap_or_default();
                self.config.db_sync_url = sync_url.unwrap_or_default();
                self.config.db_has_token = has_token;
                self.config.db_seeded_at = seeded_at;
                None
            }
            other => Some(other),
        }
    }
}
