use super::*;

fn enforce_completion_contract_backstop(task: &mut AgentTask) {
    if task.status != TaskStatus::Completed {
        return;
    }
    let blockers = task.completion_blockers();
    if blockers.is_empty() {
        return;
    }
    task.status = TaskStatus::Blocked;
    task.progress = task.progress.min(99);
    task.completed_at = None;
    task.blocked_reason = Some(format!(
        "completion contract remains open: {}",
        blockers.join("; ")
    ));
    if let Some(contract) = task.completion_contract.as_mut() {
        contract.terminal_status = None;
    }
}

fn sanitize_task_for_persistence(task: &AgentTask) -> AgentTask {
    let mut persisted = task.clone();
    enforce_completion_contract_backstop(&mut persisted);
    if persisted.sub_agent_def_id.as_deref()
        == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    {
        if let Some(prompt) = persisted.override_system_prompt.as_deref() {
            let sanitized =
                crate::agent::weles_governance::strip_weles_internal_payload_markers(prompt);
            persisted.override_system_prompt = if sanitized.trim().is_empty() {
                None
            } else {
                Some(sanitized)
            };
        }
    }
    persisted
}

async fn persist_weles_runtime_context(engine: &AgentEngine, task: &AgentTask) {
    if task.sub_agent_def_id.as_deref()
        != Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    {
        return;
    }

    let Some(prompt) = task.override_system_prompt.as_deref() else {
        return;
    };
    let key = crate::agent::persistence::weles_runtime_context_key(&task.id);

    let is_trusted = engine.trusted_weles_tasks.read().await.contains(&task.id);

    if is_trusted {
        if let Some((_scope, _marker, inspection_context)) =
            crate::agent::weles_governance::parse_weles_internal_override_payload(prompt)
        {
            let value = match serde_json::to_string(&inspection_context) {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(task_id = %task.id, "failed to serialize WELES runtime context: {error}");
                    return;
                }
            };
            if let Err(error) = engine
                .history
                .set_consolidation_state(&key, &value, now_millis())
                .await
            {
                tracing::warn!(task_id = %task.id, "failed to persist WELES runtime context: {error}");
            }
            return;
        }
    }

    if let Err(error) = engine.history.delete_consolidation_state(&key).await {
        tracing::warn!(task_id = %task.id, "failed to clear WELES runtime context: {error}");
    }
}

impl AgentEngine {
    async fn persist_thread_snapshot(&self, thread: &AgentThread) {
        let client_surface = self.get_thread_client_surface(&thread.id).await;
        let execution_profile = self
            .thread_execution_profiles
            .read()
            .await
            .get(&thread.id)
            .cloned();
        let identity = self
            .thread_identity_metadata
            .read()
            .await
            .get(&thread.id)
            .cloned();
        let handoff_state = self.thread_handoff_state(&thread.id).await;
        let thread_participants = self
            .thread_participants
            .read()
            .await
            .get(&thread.id)
            .cloned()
            .unwrap_or_default();
        let thread_participant_suggestions = self
            .thread_participant_suggestions
            .read()
            .await
            .get(&thread.id)
            .cloned()
            .unwrap_or_default();
        let latest_skill_discovery_state = self.get_thread_skill_discovery_state(&thread.id).await;
        let prompt_memory_injection_state =
            self.get_thread_memory_injection_state(&thread.id).await;
        let workspace_context = self.get_thread_workspace_context(&thread.id).await;
        let thread_row = zorai_protocol::AgentDbThread {
            id: thread.id.clone(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(persisted_agent_name_for_thread(
                thread,
                handoff_state.as_ref(),
                !thread_participants.is_empty(),
            )),
            title: thread.title.clone(),
            created_at: thread.created_at as i64,
            updated_at: thread.updated_at as i64,
            message_count: thread.messages.len() as i64,
            total_tokens: (thread.total_input_tokens + thread.total_output_tokens) as i64,
            last_preview: thread
                .messages
                .last()
                .map(|message| message.content.chars().take(100).collect())
                .unwrap_or_default(),
            metadata_json: build_thread_metadata_json(
                thread,
                identity.as_ref(),
                client_surface,
                execution_profile.as_ref(),
                handoff_state.as_ref(),
                &thread_participants,
                &thread_participant_suggestions,
                latest_skill_discovery_state.as_ref(),
                prompt_memory_injection_state.as_ref(),
                workspace_context.as_ref(),
            ),
        };

        let cutoff_ts = self
            .history
            .thread_latest_persisted_message_ts(&thread.id)
            .await
            .ok()
            .flatten()
            .unwrap_or(i64::MIN);
        let message_rows = thread
            .messages
            .iter()
            .filter(|message| (message.timestamp as i64) >= cutoff_ts)
            .map(|message| {
                let metadata_json = build_message_metadata_json(message);
                zorai_protocol::AgentDbMessage {
                    id: message.id.clone(),
                    thread_id: thread.id.clone(),
                    created_at: message.timestamp as i64,
                    role: match message.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Tool => "tool",
                    }
                    .to_string(),
                    content: message.content.clone(),
                    provider: message.provider.clone(),
                    model: message.model.clone(),
                    input_tokens: Some(message.input_tokens as i64),
                    output_tokens: Some(message.output_tokens as i64),
                    total_tokens: Some((message.input_tokens + message.output_tokens) as i64),
                    cost_usd: message.cost,
                    reasoning: message.reasoning.clone(),
                    tool_calls_json: message
                        .tool_calls
                        .as_ref()
                        .and_then(|calls| serde_json::to_string(calls).ok()),
                    metadata_json,
                }
            })
            .collect::<Vec<_>>();

        if let Err(e) = self
            .history
            .reconcile_thread_snapshot(&thread_row, &message_rows)
            .await
        {
            tracing::warn!(
                thread_id = %thread.id,
                message_count = message_rows.len(),
                "failed to persist sqlite thread snapshot: {e}"
            );
        }

        let structural_memory = self.get_thread_structural_memory(&thread.id).await;
        let persistence_result = if let Some(structural_memory) = structural_memory {
            self.history
                .upsert_thread_structural_memory_state(
                    &thread.id,
                    &structural_memory,
                    thread.updated_at,
                )
                .await
        } else {
            self.history
                .delete_thread_structural_memory(&thread.id)
                .await
        };
        if let Err(error) = persistence_result {
            tracing::warn!(thread_id = %thread.id, %error, "failed to persist thread structural memory state");
        }
    }

    pub(crate) async fn persist_thread_by_id(&self, thread_id: &str) {
        self.ensure_thread_messages_loaded(thread_id).await;
        let thread = {
            let threads = self.threads.read().await;
            threads.get(thread_id).cloned()
        };
        if let Some(thread) = thread {
            self.persist_thread_snapshot(&thread).await;
        }
    }

    pub(in crate::agent) async fn persist_threads(&self) {
        let thread_ids = {
            let threads = self.threads.read().await;
            threads.keys().cloned().collect::<Vec<_>>()
        };
        for thread_id in thread_ids {
            self.persist_thread_by_id(&thread_id).await;
        }
    }

    pub(in crate::agent) async fn persist_todos(&self) {
        let todos = self.thread_todos.read().await;
        if let Err(e) = persist_json(&self.data_dir.join("todos.json"), &*todos).await {
            tracing::warn!("failed to persist todos: {e}");
        }
    }

    pub(in crate::agent) async fn persist_work_context(&self) {
        let items = self.thread_work_contexts.read().await;
        if let Err(e) = persist_json(&self.data_dir.join("work-context.json"), &*items).await {
            tracing::warn!("failed to persist work context: {e}");
        }
    }

    pub(in crate::agent) async fn persist_tasks(&self) {
        let mut tasks = self.tasks.lock().await;
        let mut sanitized: Vec<crate::agent::types::AgentTask> = Vec::with_capacity(tasks.len());
        for task in tasks.iter_mut() {
            persist_weles_runtime_context(self, task).await;
            enforce_completion_contract_backstop(task);
            sanitized.push(sanitize_task_for_persistence(task));
        }
        if let Err(e) = self.history.upsert_agent_tasks_batch(&sanitized).await {
            tracing::warn!("failed to persist tasks batch to sqlite: {e}");
        }
        let persisted_tasks: VecDeque<_> = sanitized.into_iter().collect();
        drop(tasks);
        if let Err(e) = persist_json(&self.data_dir.join("tasks.json"), &persisted_tasks).await {
            tracing::warn!("failed to persist tasks: {e}");
        }
    }

    pub(in crate::agent) async fn persist_goal_runs(&self) {
        let goal_runs_snapshot = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs.iter().cloned().collect::<Vec<_>>()
        };
        if let Err(e) = self
            .history
            .upsert_goal_runs_batch(&goal_runs_snapshot)
            .await
        {
            tracing::warn!("failed to persist goal runs batch to sqlite: {e}");
        }

        if let Err(e) =
            persist_json(&self.data_dir.join("goal-runs.json"), &goal_runs_snapshot).await
        {
            tracing::warn!("failed to persist goal runs: {e}");
        }
    }

    pub(in crate::agent) fn persist_goal_runs_in_background(self: &Arc<Self>) {
        let engine = self.clone();
        tokio::spawn(async move {
            engine.persist_goal_runs().await;
        });
    }

    pub(in crate::agent) async fn persist_heartbeat(&self) {
        let items = self.heartbeat_items.read().await;
        if let Err(e) = persist_json(&self.data_dir.join("heartbeat.json"), &*items).await {
            tracing::warn!("failed to persist heartbeat: {e}");
        }
    }

    pub(in crate::agent) async fn persist_config(&self) {
        let config = self.config.read().await.clone();
        if !config.tools.computer_use {
            crate::agent::stop_computer_control(&self.event_tx, "computer use was turned off")
                .await;
        }
        self.store_config_snapshot(config).await;
    }

    pub(in crate::agent) async fn persist_heuristic_store(&self) {
        let store = self.heuristic_store.read().await.clone();
        let path = self.data_dir.join("heuristics.json");
        match serde_json::to_string_pretty(&store) {
            Ok(json) => {
                if let Err(e) = tokio::fs::write(&path, json).await {
                    tracing::warn!(error = %e, "failed to persist heuristic store");
                }
            }
            Err(e) => tracing::warn!(error = %e, "failed to serialize heuristic store"),
        }
    }

    pub(in crate::agent) async fn persist_pattern_store(&self) {
        let store = self.pattern_store.read().await.clone();
        let path = self.data_dir.join("patterns.json");
        match serde_json::to_string_pretty(&store) {
            Ok(json) => {
                if let Err(e) = tokio::fs::write(&path, json).await {
                    tracing::warn!(error = %e, "failed to persist pattern store");
                }
            }
            Err(e) => tracing::warn!(error = %e, "failed to serialize pattern store"),
        }
    }

    pub(in crate::agent) async fn persist_learning_stores(&self) {
        self.persist_heuristic_store().await;
        self.persist_pattern_store().await;
        let model = self.meta_cognitive_self_model.read().await.clone();
        if let Err(error) = self.persist_meta_cognitive_self_model(&model).await {
            tracing::warn!(%error, "failed to persist meta-cognitive self-model");
        }
    }

    pub(in crate::agent) async fn take_continuity_acknowledgment(
        &self,
        thread_id: &str,
    ) -> Option<String> {
        let stored_id = self
            .history
            .get_consolidation_state("continuity_thread_id")
            .await
            .ok()
            .flatten()?;
        if stored_id.is_empty() || stored_id != thread_id {
            return None;
        }
        let topic = self
            .history
            .get_consolidation_state("continuity_topic")
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "the previous session".to_string());

        self.history
            .set_consolidation_state("continuity_thread_id", "", now_millis())
            .await
            .ok();
        self.history
            .set_consolidation_state("continuity_topic", "", now_millis())
            .await
            .ok();

        Some(format!(
            "Resuming from where we left off \u{2014} last working on {}.",
            topic
        ))
    }

    pub(in crate::agent) async fn record_policy_decision(
        &self,
        scope: &super::orchestrator_policy::PolicyDecisionScope,
        decision: super::orchestrator_policy::PolicyDecision,
        now_epoch_secs: u64,
    ) {
        {
            let mut recent_decisions = self.recent_policy_decisions.write().await;
            super::orchestrator_policy::record_policy_decision(
                &mut recent_decisions,
                scope,
                decision.clone(),
                now_epoch_secs,
            );
        }

        if let Some(retry_guard) = decision.retry_guard.as_deref() {
            self.record_retry_guard(scope, retry_guard, now_epoch_secs)
                .await;
        }
    }

    pub(in crate::agent) async fn latest_policy_decision(
        &self,
        scope: &super::orchestrator_policy::PolicyDecisionScope,
        now_epoch_secs: u64,
    ) -> Option<super::orchestrator_policy::RecentPolicyDecision> {
        let mut recent_decisions = self.recent_policy_decisions.write().await;
        super::orchestrator_policy::latest_policy_decision(
            &mut recent_decisions,
            scope,
            now_epoch_secs,
            super::orchestrator_policy::SHORT_LIVED_POLICY_WINDOW_SECS,
        )
    }

    pub(in crate::agent) async fn record_retry_guard(
        &self,
        scope: &super::orchestrator_policy::PolicyDecisionScope,
        approach_hash: &str,
        now_epoch_secs: u64,
    ) {
        let mut retry_guards = self.retry_guards.write().await;
        super::orchestrator_policy::record_retry_guard(
            &mut retry_guards,
            scope,
            approach_hash,
            now_epoch_secs,
        );
    }

    pub(in crate::agent) async fn is_retry_guard_active(
        &self,
        scope: &super::orchestrator_policy::PolicyDecisionScope,
        approach_hash: &str,
        now_epoch_secs: u64,
    ) -> bool {
        let mut retry_guards = self.retry_guards.write().await;
        super::orchestrator_policy::is_retry_guard_active(
            &mut retry_guards,
            scope,
            approach_hash,
            now_epoch_secs,
            super::orchestrator_policy::SHORT_LIVED_POLICY_WINDOW_SECS,
        )
    }
}

fn persisted_agent_name_for_thread(
    thread: &AgentThread,
    handoff_state: Option<&ThreadHandoffState>,
    has_thread_participants: bool,
) -> String {
    persisted_agent_name_for_thread_surface(thread, handoff_state, has_thread_participants)
}
