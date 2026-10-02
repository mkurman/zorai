#![allow(dead_code)]

use super::*;
use crate::agent::llm_client::{
    parse_structured_upstream_failure, sanitize_upstream_failure_message,
};

mod finalize;
mod loop_core;
mod prompt;
mod report_back;
mod setup;
mod tool_calls;
pub(crate) mod tool_results;
mod types;

use types::{
    FreshRunnerRetrySignal, LoopDisposition, SendMessageRunner, StreamIteration,
    ToolCallDisposition,
};

pub(super) const DEFAULT_LLM_STREAM_CHUNK_TIMEOUT_SECS: u64 = 300;

pub(super) fn tool_args_summary(arguments: &str) -> String {
    arguments.chars().take(100).collect()
}

pub(super) fn current_epoch_secs() -> u64 {
    now_millis() / 1000
}

impl<'a> SendMessageRunner<'a> {
    pub(super) fn emit_context_window_update_for_messages(&self, messages: &[AgentMessage]) {
        let (active_context_window_start, active_messages) =
            crate::agent::compaction::active_compaction_window(messages);
        let active_context_window_tokens =
            crate::agent::compaction::estimate_message_tokens(active_messages) as u64;
        tracing::info!(
            thread_id = %self.tid,
            active_context_window_start,
            active_context_window_end = messages.len(),
            active_context_window_tokens,
            total_messages = messages.len(),
            active_messages_count = active_messages.len(),
            "agent loop emitting ContextWindowUpdate"
        );
        let _ = self.engine.event_tx.send(AgentEvent::ContextWindowUpdate {
            thread_id: self.tid.clone(),
            active_context_window_start,
            active_context_window_end: messages.len(),
            active_context_window_tokens,
        });
    }

    pub(super) async fn emit_context_window_update_for_current_thread(&self) {
        let threads = self.engine.threads.read().await;
        let Some(thread) = threads.get(&self.tid) else {
            return;
        };
        self.emit_context_window_update_for_messages(&thread.messages);
    }
}

impl AgentEngine {
    fn parse_content_blocks_json(
        content_blocks_json: Option<&str>,
    ) -> Result<Vec<AgentContentBlock>> {
        match content_blocks_json
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(json) => serde_json::from_str::<Vec<AgentContentBlock>>(json)
                .map_err(|error| anyhow::anyhow!("invalid content_blocks_json: {error}")),
            None => Ok(Vec::new()),
        }
    }

    pub(in crate::agent) async fn send_message_inner_with_options(
        &self,
        thread_id: Option<&str>,
        content: &str,
        content_blocks_json: Option<&str>,
        task_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        backend_override: Option<&str>,
        llm_user_content_override: Option<&str>,
        stream_chunk_timeout_override: Option<std::time::Duration>,
        client_surface: Option<zorai_protocol::ClientSurface>,
        record_operator: bool,
        initial_reuse_existing_user_message: bool,
        allow_auto_participant_queue_drain: bool,
        run_participant_observers_after_turn: bool,
        agent_scope_override: Option<&str>,
    ) -> Result<SendMessageOutcome> {
        let stored_user_content = content;
        let stored_user_content_blocks = Self::parse_content_blocks_json(content_blocks_json)?;
        let mut current_thread_id = thread_id.map(str::to_string);
        let mut current_llm_user_content = llm_user_content_override.unwrap_or(content).to_string();
        let mut current_record_operator = record_operator;
        let mut reuse_existing_user_message = initial_reuse_existing_user_message;

        loop {
            let agent_scope_id = match agent_scope_override {
                Some(scope) if task_id.is_none() && !scope.trim().is_empty() => {
                    crate::agent::agent_identity::agent_turn_scope_id(scope)
                }
                _ => {
                    self.agent_scope_id_for_turn(current_thread_id.as_deref(), task_id)
                        .await
                }
            };
            let thread_for_turn = current_thread_id.clone();
            let llm_user_content_for_turn = current_llm_user_content.clone();
            let stored_user_content_blocks_for_turn = stored_user_content_blocks.clone();
            let record_operator_for_turn = current_record_operator;
            let reuse_existing_for_turn = reuse_existing_user_message;

            let outcome = Box::pin(run_with_agent_scope(agent_scope_id, async move {
                if thread_for_turn.as_deref() == Some(crate::agent::concierge::CONCIERGE_THREAD_ID)
                {
                    self.send_concierge_message_on_thread(
                        crate::agent::concierge::CONCIERGE_THREAD_ID,
                        stored_user_content,
                        preferred_session_hint,
                        record_operator_for_turn,
                        true,
                    )
                    .await?;
                    return Ok(SendMessageOutcome {
                        thread_id: crate::agent::concierge::CONCIERGE_THREAD_ID.to_string(),
                        stream_generation: 0,
                        interrupted_for_approval: false,
                        terminated_for_budget: false,
                        subagent_report: None,
                        upstream_message: None,
                        provider_final_result: None,
                        fresh_runner_retry: None,
                        handoff_restart: None,
                    });
                }

                let config = self.config.read().await.clone();
                let selected_backend = backend_override
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(AgentBackend::parse)
                    .unwrap_or(config.agent_backend.clone());

                match selected_backend {
                    AgentBackend::Openclaw | AgentBackend::Hermes => {
                        let mut runtime_config = config.clone();
                        runtime_config.agent_backend = selected_backend;
                        return self
                            .send_message_external(
                                &runtime_config,
                                thread_for_turn.as_deref(),
                                &llm_user_content_for_turn,
                            )
                            .await
                            .map(|thread_id| SendMessageOutcome {
                                thread_id,
                                stream_generation: 0,
                                interrupted_for_approval: false,
                                terminated_for_budget: false,
                                subagent_report: None,
                                upstream_message: None,
                                provider_final_result: None,
                                fresh_runner_retry: None,
                                handoff_restart: None,
                            });
                    }
                    _ => {}
                }

                Box::pin(self.run_internal_send_loop(
                    thread_for_turn.as_deref(),
                    stored_user_content,
                    &stored_user_content_blocks_for_turn,
                    &llm_user_content_for_turn,
                    task_id,
                    preferred_session_hint,
                    stream_chunk_timeout_override,
                    client_surface,
                    record_operator_for_turn,
                    reuse_existing_for_turn,
                ))
                .await
            }))
            .await?;

            if let Some(restart) = outcome.handoff_restart.clone() {
                current_thread_id = Some(outcome.thread_id);
                current_llm_user_content = restart.llm_user_content;
                current_record_operator = false;
                reuse_existing_user_message = true;
                continue;
            }

            if allow_auto_participant_queue_drain && !outcome.interrupted_for_approval {
                let sent_any = Box::pin(
                    self.maybe_auto_send_next_thread_participant_suggestion(&outcome.thread_id),
                )
                .await?;
                if sent_any {
                    return Ok(outcome);
                }
            }

            if run_participant_observers_after_turn && !outcome.interrupted_for_approval {
                if let Err(error) =
                    Box::pin(self.run_participant_observers(&outcome.thread_id)).await
                {
                    let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                        thread_id: outcome.thread_id.clone(),
                        kind: "participant_observer_error".to_string(),
                        message: "participant observers failed".to_string(),
                        details: Some(error.to_string()),
                    });
                }
            }

            return Ok(outcome);
        }
    }

    pub(in crate::agent) async fn agent_scope_id_for_turn(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
    ) -> String {
        if let Some(current_task_id) = task_id {
            match self
                .history
                .agent_task_override_system_prompt(current_task_id)
                .await
            {
                Ok(Some(Some(prompt))) => {
                    return crate::agent::agent_identity::extract_persona_id(Some(&prompt))
                        .unwrap_or_else(|| MAIN_AGENT_ID.to_string());
                }
                Ok(Some(None)) => return MAIN_AGENT_ID.to_string(),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(
                        task_id = current_task_id,
                        %error,
                        "failed to query task scope prompt from history"
                    );
                }
            }
            let task = self.task_by_id_for_turn_scope(current_task_id).await;
            return agent_scope_id_for_task(task.as_ref());
        }

        if thread_id == Some(crate::agent::concierge::CONCIERGE_THREAD_ID) {
            return CONCIERGE_AGENT_ID.to_string();
        }

        if let Some(existing_thread_id) = thread_id {
            if let Some(active_agent_id) = self.active_agent_id_for_thread(existing_thread_id).await
            {
                return active_agent_id;
            }

            let owner_alias = self
                .threads
                .read()
                .await
                .get(existing_thread_id)
                .and_then(|thread| thread.agent_name.clone())
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty());
            if let Some(alias) = owner_alias {
                let sub_agents = self.list_sub_agents().await;
                let resolved =
                    crate::agent::agent_identity::resolve_agent_target(&alias, &sub_agents);
                if resolved.scope_id != MAIN_AGENT_ID
                    || crate::agent::agent_identity::is_main_agent_scope(&alias)
                {
                    return resolved.scope_id;
                }
                return alias.to_ascii_lowercase();
            }
        }

        MAIN_AGENT_ID.to_string()
    }

    async fn task_by_id_for_turn_scope(&self, task_id: &str) -> Option<AgentTask> {
        match self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some(task_id.to_string()),
                status: None,
                statuses: Vec::new(),
                source: None,
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: false,
                order_by_recent_activity_desc: false,
                limit: Some(1),
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await
            .into_iter()
            .next()
        {
            Some(task) => Some(task),
            None => {
                let tasks = self.tasks.lock().await;
                tasks.iter().find(|task| task.id == task_id).cloned()
            }
        }
    }

    pub(in crate::agent) async fn run_internal_send_loop(
        &self,
        initial_thread_id: Option<&str>,
        stored_user_content: &str,
        stored_user_content_blocks: &[AgentContentBlock],
        llm_user_content: &str,
        task_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        stream_chunk_timeout_override: Option<std::time::Duration>,
        client_surface: Option<zorai_protocol::ClientSurface>,
        initial_record_operator: bool,
        initial_reuse_existing_user_message: bool,
    ) -> Result<SendMessageOutcome> {
        let mut thread_id = initial_thread_id.map(str::to_string);
        let mut record_operator = initial_record_operator;
        let mut reuse_existing_user_message = initial_reuse_existing_user_message;
        let mut scheduled_retry_cycles = 0u32;
        if let Some(thread_id) = thread_id.as_deref() {
            self.clear_operator_stream_stop(thread_id).await;
        }

        loop {
            let runner = SendMessageRunner::initialize(
                self,
                thread_id.as_deref(),
                stored_user_content,
                stored_user_content_blocks,
                llm_user_content,
                task_id,
                preferred_session_hint,
                stream_chunk_timeout_override,
                client_surface,
                record_operator,
                reuse_existing_user_message,
                scheduled_retry_cycles,
            )
            .await?;
            let outcome = Box::pin(runner.run()).await?;
            if let Some(retry) = outcome.fresh_runner_retry {
                if self.stream_cancellation_requested(&outcome.thread_id).await {
                    self.finish_stream_cancellation(&outcome.thread_id, outcome.stream_generation)
                        .await;
                    return Ok(outcome);
                }
                thread_id = Some(outcome.thread_id.clone());
                record_operator = false;
                reuse_existing_user_message = true;
                scheduled_retry_cycles = retry.scheduled_retry_cycles;
                continue;
            }
            return Ok(outcome);
        }
    }

    pub(in crate::agent) async fn send_message_inner(
        &self,
        thread_id: Option<&str>,
        content: &str,
        content_blocks_json: Option<&str>,
        task_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        backend_override: Option<&str>,
        llm_user_content_override: Option<&str>,
        stream_chunk_timeout_override: Option<std::time::Duration>,
        client_surface: Option<zorai_protocol::ClientSurface>,
        record_operator: bool,
    ) -> Result<SendMessageOutcome> {
        self.send_message_inner_with_options(
            thread_id,
            content,
            content_blocks_json,
            task_id,
            preferred_session_hint,
            backend_override,
            llm_user_content_override,
            stream_chunk_timeout_override,
            client_surface,
            record_operator,
            false,
            true,
            true,
            None,
        )
        .await
    }

    pub(in crate::agent) async fn resend_existing_user_message(
        &self,
        thread_id: &str,
        content: &str,
    ) -> Result<SendMessageOutcome> {
        self.send_message_inner_with_options(
            Some(thread_id),
            content,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
            true,
            true,
            None,
        )
        .await
    }

    pub(in crate::agent) async fn resend_existing_user_message_for_task(
        &self,
        thread_id: &str,
        content: &str,
        task_id: &str,
    ) -> Result<SendMessageOutcome> {
        self.send_message_inner_with_options(
            Some(thread_id),
            content,
            None,
            Some(task_id),
            None,
            None,
            None,
            None,
            None,
            false,
            true,
            true,
            true,
            None,
        )
        .await
    }

    pub(in crate::agent) async fn continue_existing_user_message_without_queue_drain(
        &self,
        thread_id: &str,
        content: &str,
    ) -> Result<SendMessageOutcome> {
        self.send_message_inner_with_options(
            Some(thread_id),
            content,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
            false,
            false,
            None,
        )
        .await
    }

    pub(in crate::agent) fn resolve_provider_config(
        &self,
        config: &AgentConfig,
    ) -> Result<ProviderConfig> {
        resolve_active_provider_config(config)
    }

    pub(in crate::agent) fn resolve_sub_agent_provider_config(
        &self,
        config: &AgentConfig,
        sub_agent_provider: &str,
    ) -> Result<ProviderConfig> {
        resolve_provider_config_for(config, sub_agent_provider, None)
    }
}

impl<'a> SendMessageRunner<'a> {
    pub(super) async fn run(mut self) -> Result<SendMessageOutcome> {
        while self.max_loops == 0 || self.loop_count < self.max_loops {
            if self.stream_cancel_token.is_cancelled() {
                self.was_cancelled = true;
                break;
            }
            self.loop_count += 1;

            self.maybe_rebuild_prompt_after_memory_flush().await?;

            let iteration = match self.stream_once().await {
                Ok(iteration) => iteration,
                Err(error) => {
                    if let Some(signal) = error.downcast_ref::<types::FreshRunnerRetrySignal>() {
                        self.fresh_runner_retry = Some(FreshRunnerRetryRequest {
                            scheduled_retry_cycles: signal.scheduled_retry_cycles,
                        });
                        break;
                    }
                    if self.was_cancelled {
                        break;
                    }
                    if self
                        .finish_spawned_subagent_on_unrecoverable_error(&error)
                        .await
                    {
                        break;
                    }
                    return Err(error);
                }
            };
            if self.was_cancelled {
                break;
            }
            let retry_timed_out = match self.handle_stream_timeout(&iteration).await {
                Ok(retry_timed_out) => retry_timed_out,
                Err(error) => {
                    if let Some(signal) = error.downcast_ref::<types::FreshRunnerRetrySignal>() {
                        self.fresh_runner_retry = Some(FreshRunnerRetryRequest {
                            scheduled_retry_cycles: signal.scheduled_retry_cycles,
                        });
                        break;
                    }
                    if self
                        .finish_spawned_subagent_on_unrecoverable_error(&error)
                        .await
                    {
                        break;
                    }
                    return Err(error);
                }
            };
            if retry_timed_out {
                continue;
            }
            if iteration.final_chunk.is_some() {
                self.stream_timeout_count = 0;
                self.scheduled_retry_cycles = 0;
                self.engine
                    .record_llm_outcome(&self.config.provider, true)
                    .await;
            }
            match self.handle_iteration(iteration).await? {
                LoopDisposition::Continue => continue,
                LoopDisposition::Break => break,
            }
        }

        self.finish().await
    }
}
