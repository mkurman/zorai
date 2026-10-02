use crate::client::DaemonClient;
use anyhow::Result;
use zorai_protocol::ClientMessage;
impl DaemonClient {
    pub fn request_agent_status(&self) -> Result<()> {
        self.send(ClientMessage::AgentStatusQuery)
    }

    pub fn request_agent_statistics(
        &self,
        window: zorai_protocol::AgentStatisticsWindow,
    ) -> Result<()> {
        self.send(ClientMessage::AgentStatisticsQuery {
            window,
            session_limit: None,
            session_offset: None,
        })
    }

    pub fn request_prompt_inspection(&self, agent_id: Option<String>) -> Result<()> {
        self.send(ClientMessage::AgentInspectPrompt {
            agent_id,
            client_surface: Some(zorai_protocol::ClientSurface::Tui),
        })
    }

    pub fn request_file_preview(
        &self,
        path: impl Into<String>,
        max_bytes: Option<usize>,
    ) -> Result<()> {
        self.send(ClientMessage::GetFilePreview {
            path: path.into(),
            max_bytes,
        })
    }

    pub fn send_message(
        &self,
        thread_id: Option<String>,
        content: String,
        content_blocks_json: Option<String>,
        session_id: Option<String>,
        target_agent_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSendMessage {
            thread_id,
            content,
            session_id,
            context_messages_json: None,
            content_blocks_json,
            client_surface: Some(zorai_protocol::ClientSurface::Tui),
            target_agent_id,
        })
    }

    pub fn stop_stream(&self, thread_id: String) -> Result<()> {
        self.send(ClientMessage::AgentStopStream { thread_id })
    }

    pub fn force_compact(&self, thread_id: String) -> Result<()> {
        self.send(ClientMessage::AgentForceCompact { thread_id })
    }

    pub fn send_internal_delegate(
        &self,
        thread_id: Option<String>,
        target_agent_id: String,
        content: String,
        session_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentInternalDelegate {
            thread_id,
            target_agent_id,
            content,
            session_id,
            client_surface: Some(zorai_protocol::ClientSurface::Tui),
        })
    }

    pub fn send_thread_participant_command(
        &self,
        thread_id: String,
        target_agent_id: String,
        action: String,
        instruction: Option<String>,
        session_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentThreadParticipantCommand {
            thread_id,
            target_agent_id,
            action,
            instruction,
            session_id,
            client_surface: Some(zorai_protocol::ClientSurface::Tui),
        })
    }

    pub fn send_thread_handoff(
        &self,
        thread_id: String,
        action: String,
        target_agent_id: Option<String>,
        reason: String,
        summary: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentHandoffThread {
            thread_id,
            action,
            target_agent_id,
            reason,
            summary,
            requested_by: "user".to_string(),
            session_id: None,
            client_surface: Some(zorai_protocol::ClientSurface::Tui),
        })
    }

    pub fn send_participant_suggestion(
        &self,
        thread_id: String,
        suggestion_id: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSendParticipantSuggestion {
            thread_id,
            suggestion_id,
            session_id: None,
            client_surface: Some(zorai_protocol::ClientSurface::Tui),
            force_send: false,
        })
    }

    pub fn dismiss_participant_suggestion(
        &self,
        thread_id: String,
        suggestion_id: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentDismissParticipantSuggestion {
            thread_id,
            suggestion_id,
            session_id: None,
            client_surface: Some(zorai_protocol::ClientSurface::Tui),
        })
    }

    pub fn retry_stream_now(&self, thread_id: String) -> Result<()> {
        self.send(ClientMessage::AgentRetryStreamNow { thread_id })
    }

    pub fn delete_messages(&self, thread_id: String, message_ids: Vec<String>) -> Result<()> {
        self.send(ClientMessage::DeleteAgentMessages {
            thread_id,
            message_ids,
        })
    }

    pub fn delete_thread(&self, thread_id: String) -> Result<()> {
        self.send(ClientMessage::AgentDeleteThread { thread_id })
    }

    pub fn pin_thread_message_for_compaction(
        &self,
        thread_id: String,
        message_id: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentPinThreadMessageForCompaction {
            thread_id,
            message_id,
        })
    }

    pub fn unpin_thread_message_for_compaction(
        &self,
        thread_id: String,
        message_id: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentUnpinThreadMessageForCompaction {
            thread_id,
            message_id,
        })
    }

    pub fn submit_message_feedback(
        &self,
        thread_id: String,
        message_id: String,
        absolute_message_index: Option<usize>,
        reaction: Option<zorai_protocol::Reaction>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentMessageFeedback {
            thread_id,
            message_id,
            absolute_message_index,
            reaction,
        })
    }

    pub fn spawn_session(
        &self,
        shell: Option<String>,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Result<()> {
        self.send(ClientMessage::SpawnSession {
            shell,
            cwd,
            env: None,
            workspace_id: None,
            cols,
            rows,
        })
    }

    pub fn control_goal_run(
        &self,
        goal_run_id: String,
        action: String,
        step_index: Option<usize>,
        payload_json: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentControlGoalRun {
            goal_run_id,
            action,
            step_index,
            payload_json,
        })
    }

    pub fn delete_goal_run(&self, goal_run_id: String) -> Result<()> {
        self.send(ClientMessage::AgentDeleteGoalRun { goal_run_id })
    }

    pub fn cancel_task(&self, task_id: String) -> Result<()> {
        self.send(ClientMessage::AgentCancelTask { task_id })
    }

    pub fn get_workspace_settings(&self, workspace_id: String) -> Result<()> {
        self.send(ClientMessage::AgentGetWorkspaceSettings { workspace_id })
    }

    pub fn list_workspace_settings(&self) -> Result<()> {
        self.send(ClientMessage::AgentListWorkspaceSettings)
    }

    pub fn set_workspace_operator(
        &self,
        workspace_id: String,
        operator: zorai_protocol::WorkspaceOperator,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSetWorkspaceOperator {
            workspace_id,
            operator,
        })
    }

    pub fn set_workspace_repo_monitor(
        &self,
        workspace_id: String,
        repo_monitor_enabled: bool,
        repo_monitor_include_dirs: Vec<String>,
        repo_monitor_exclude_dirs: Vec<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSetWorkspaceRepoMonitor {
            workspace_id,
            repo_monitor_enabled,
            repo_monitor_include_dirs,
            repo_monitor_exclude_dirs,
        })
    }

    pub fn create_workspace_task(
        &self,
        request: zorai_protocol::WorkspaceTaskCreate,
    ) -> Result<()> {
        self.send(ClientMessage::AgentCreateWorkspaceTask { request })
    }

    pub fn list_workspace_tasks(&self, workspace_id: String, include_deleted: bool) -> Result<()> {
        self.send(ClientMessage::AgentListWorkspaceTasks {
            workspace_id,
            include_deleted,
        })
    }

    pub fn list_workspace_notices(
        &self,
        workspace_id: String,
        task_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentListWorkspaceNotices {
            workspace_id,
            task_id,
        })
    }

    pub fn update_workspace_task(
        &self,
        task_id: String,
        update: zorai_protocol::WorkspaceTaskUpdate,
    ) -> Result<()> {
        self.send(ClientMessage::AgentUpdateWorkspaceTask { task_id, update })
    }

    pub fn run_workspace_task(&self, task_id: String) -> Result<()> {
        self.send(ClientMessage::AgentRunWorkspaceTask { task_id })
    }

    pub fn pause_workspace_task(&self, task_id: String) -> Result<()> {
        self.send(ClientMessage::AgentPauseWorkspaceTask { task_id })
    }

    pub fn stop_workspace_task(&self, task_id: String) -> Result<()> {
        self.send(ClientMessage::AgentStopWorkspaceTask { task_id })
    }

    pub fn move_workspace_task(&self, request: zorai_protocol::WorkspaceTaskMove) -> Result<()> {
        self.send(ClientMessage::AgentMoveWorkspaceTask { request })
    }

    pub fn submit_workspace_review(
        &self,
        review: zorai_protocol::WorkspaceReviewSubmission,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSubmitWorkspaceReview { review })
    }

    pub fn delete_workspace_task(&self, task_id: String) -> Result<()> {
        self.send(ClientMessage::AgentDeleteWorkspaceTask { task_id })
    }

    pub fn fetch_models(
        &self,
        provider_id: String,
        base_url: String,
        api_key: String,
        output_modalities: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentFetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        })
    }

    pub fn set_config_item_json(&self, key_path: String, value_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetConfigItem {
            key_path,
            value_json,
        })
    }

    pub fn external_runtime_migration_status(&self) -> Result<()> {
        self.send(ClientMessage::AgentExternalRuntimeMigrationStatus)
    }

    pub fn external_runtime_migration_preview(
        &self,
        runtime: String,
        config_path: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentExternalRuntimeMigrationPreview {
            runtime,
            config_path,
        })
    }

    pub fn external_runtime_migration_apply(
        &self,
        runtime: String,
        config_path: Option<String>,
        conflict_policy: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentExternalRuntimeMigrationApply {
            runtime,
            config_path,
            conflict_policy,
        })
    }

    pub fn external_runtime_migration_report(
        &self,
        runtime: Option<String>,
        limit: Option<usize>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentExternalRuntimeMigrationReport { runtime, limit })
    }

    pub fn external_runtime_migration_shadow_run(&self, runtime: String) -> Result<()> {
        self.send(ClientMessage::AgentExternalRuntimeMigrationShadowRun { runtime })
    }

    pub fn set_provider_model(&self, provider_id: String, model: String) -> Result<()> {
        self.send(ClientMessage::AgentSetProviderModel { provider_id, model })
    }

    pub fn set_target_agent_provider_model(
        &self,
        target_agent_id: String,
        provider_id: String,
        model: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSetTargetAgentProviderModel {
            target_agent_id,
            provider_id,
            model,
        })
    }

    pub fn set_target_agent_reasoning_effort(
        &self,
        target_agent_id: String,
        reasoning_effort: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSetTargetAgentReasoningEffort {
            target_agent_id,
            reasoning_effort,
        })
    }

    pub fn set_target_agent_context_window(
        &self,
        target_agent_id: String,
        context_window_tokens: u32,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSetTargetAgentContextWindow {
            target_agent_id,
            context_window_tokens,
        })
    }

    pub fn set_thread_execution_profile(
        &self,
        thread_id: String,
        profile_json: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSetThreadExecutionProfile {
            thread_id,
            profile_json,
        })
    }

    pub fn get_provider_auth_states(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetProviderAuthStates)
    }

    pub fn get_provider_catalog(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetProviderCatalog)
    }

    pub fn get_openai_codex_auth_status(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetOpenAICodexAuthStatus)
    }

    pub fn login_openai_codex(&self) -> Result<()> {
        self.send(ClientMessage::AgentLoginOpenAICodex)
    }

    pub fn logout_openai_codex(&self) -> Result<()> {
        self.send(ClientMessage::AgentLogoutOpenAICodex)
    }

    pub fn login_provider(
        &self,
        provider_id: String,
        api_key: String,
        base_url: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentLoginProvider {
            provider_id,
            api_key,
            base_url,
        })
    }

    pub fn logout_provider(&self, provider_id: String) -> Result<()> {
        self.send(ClientMessage::AgentLogoutProvider { provider_id })
    }

    pub fn validate_provider(
        &self,
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentValidateProvider {
            provider_id,
            base_url,
            api_key,
            auth_source,
        })
    }

    pub fn set_sub_agent(&self, sub_agent_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetSubAgent { sub_agent_json })
    }

    pub fn remove_sub_agent(&self, sub_agent_id: String) -> Result<()> {
        self.send(ClientMessage::AgentRemoveSubAgent { sub_agent_id })
    }

    pub fn list_sub_agents(&self) -> Result<()> {
        self.send(ClientMessage::AgentListSubAgents)
    }

    pub fn get_concierge_config(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetConciergeConfig)
    }

    pub fn set_concierge_config(&self, config_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetConciergeConfig { config_json })
    }

    pub fn request_concierge_welcome(&self) -> Result<()> {
        self.send(ClientMessage::AgentRequestConciergeWelcome)
    }

    pub fn list_checkpoints(&self, goal_run_id: String) -> Result<()> {
        self.send(ClientMessage::AgentListCheckpoints { goal_run_id })
    }

    pub fn dismiss_concierge_welcome(&self) -> Result<()> {
        self.send(ClientMessage::AgentDismissConciergeWelcome)
    }

    pub fn record_attention(
        &self,
        surface: String,
        thread_id: Option<String>,
        goal_run_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentRecordAttention {
            surface,
            thread_id,
            goal_run_id,
        })
    }

    pub fn start_operator_profile_session(&self, kind: String) -> Result<()> {
        self.send(ClientMessage::AgentStartOperatorProfileSession { kind })
    }

    pub fn next_operator_profile_question(&self, session_id: String) -> Result<()> {
        self.send(ClientMessage::AgentNextOperatorProfileQuestion { session_id })
    }

    pub fn submit_operator_profile_answer(
        &self,
        session_id: String,
        question_id: String,
        answer_json: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        })
    }

    pub fn skip_operator_profile_question(
        &self,
        session_id: String,
        question_id: String,
        reason: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSkipOperatorProfileQuestion {
            session_id,
            question_id,
            reason,
        })
    }

    pub fn defer_operator_profile_question(
        &self,
        session_id: String,
        question_id: String,
        defer_until_unix_ms: Option<u64>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentDeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        })
    }
}
