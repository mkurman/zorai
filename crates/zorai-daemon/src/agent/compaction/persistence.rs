use super::*;

impl AgentEngine {
    pub(crate) async fn maybe_persist_compaction_artifact(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
    ) -> Result<bool> {
        self.persist_compaction_artifact_with_mode(
            thread_id,
            task_id,
            config,
            provider_config,
            CompactionCandidateMode::Automatic,
        )
        .await
    }

    pub(crate) async fn force_persist_compaction_artifact(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
    ) -> Result<bool> {
        self.persist_compaction_artifact_with_mode(
            thread_id,
            task_id,
            config,
            provider_config,
            CompactionCandidateMode::Forced,
        )
        .await
    }

    pub(crate) async fn persist_compaction_artifact_with_mode(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
        mode: CompactionCandidateMode,
    ) -> Result<bool> {
        let (window_messages, window_start, message_count) = {
            let threads = self.threads.read().await;
            let Some(thread) = threads.get(thread_id) else {
                return Ok(false);
            };
            let (window_start, _) = active_compaction_window(&thread.messages);
            (
                thread.messages[window_start..].to_vec(),
                window_start,
                thread.messages.len(),
            )
        };
        let Some(candidate) = (match mode {
            CompactionCandidateMode::Automatic => {
                compaction_candidate(&window_messages, config, provider_config)
            }
            CompactionCandidateMode::Forced => {
                forced_compaction_candidate(&window_messages, config, provider_config)
            }
        }) else {
            return Ok(false);
        };
        let pre_compaction_total_tokens = estimate_message_tokens(&window_messages);
        let effective_context_window_tokens =
            effective_compaction_window_tokens(config, provider_config);
        let split_at = window_start + candidate.split_at;
        let source_end = candidate.split_at.min(window_messages.len());
        let source_messages = window_messages[..source_end].to_vec();
        let structural_memory = self.get_thread_structural_memory(thread_id).await;
        let compaction_scope = self.compaction_scope_snapshot(thread_id, task_id).await;

        let artifact_result = self
            .build_compaction_artifact(
                thread_id,
                &source_messages,
                candidate.target_tokens,
                candidate.trigger,
                pre_compaction_total_tokens,
                effective_context_window_tokens,
                config,
                structural_memory.as_ref(),
                compaction_scope.as_ref(),
                mode,
            )
            .await;
        let (artifact, strategy_used, fallback_notice) = artifact_result?;
        let artifact_notice = serde_json::json!({
            "artifact_id": artifact.id,
            "artifact_content": artifact.content,
            "artifact_payload": artifact.compaction_payload,
            "artifact_strategy": strategy_used,
        });
        let compaction_trigger_summary = build_compaction_visible_content(
            pre_compaction_total_tokens,
            effective_context_window_tokens,
            candidate.target_tokens,
            candidate.trigger,
            strategy_used,
        );

        let (current_split_at, total_message_count) = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return Ok(false);
            };
            let Some(_recheck_candidate) = (match mode {
                CompactionCandidateMode::Automatic => {
                    compaction_candidate(&thread.messages, config, provider_config)
                }
                CompactionCandidateMode::Forced => {
                    forced_compaction_candidate(&thread.messages, config, provider_config)
                }
            }) else {
                return Ok(false);
            };
            let current_split_at = thread.messages.len();
            thread.messages.push(artifact);
            thread.updated_at = now_millis();
            thread.total_input_tokens = thread
                .messages
                .iter()
                .map(|message| message.input_tokens)
                .sum();
            thread.total_output_tokens = thread
                .messages
                .iter()
                .map(|message| message.output_tokens)
                .sum();
            (current_split_at, thread.messages.len())
        };
        let _displayed_post_compaction_total_tokens = {
            let threads = self.threads.read().await;
            match threads.get(thread_id) {
                Some(thread) => {
                    let (_, active_messages) = active_compaction_window(&thread.messages);
                    estimate_message_tokens(active_messages) as u64
                }
                None => 0u64,
            }
        };
        let (
            post_compaction_window_start,
            post_compaction_window_end,
            post_compaction_total_tokens,
            total_message_count,
        ) = {
            let threads = self.threads.read().await;
            match threads.get(thread_id) {
                Some(thread) => {
                    let (window_start, active_messages) =
                        active_compaction_window(&thread.messages);
                    (
                        window_start,
                        thread.messages.len(),
                        estimate_message_tokens(active_messages) as u64,
                        thread.messages.len(),
                    )
                }
                None => (
                    current_split_at,
                    total_message_count,
                    0u64,
                    total_message_count,
                ),
            }
        };
        let compaction_notice_details = serde_json::json!({
            "split_at": current_split_at,
            "total_message_count": total_message_count,
            "pre_compaction_total_tokens": pre_compaction_total_tokens,
            "post_compaction_total_tokens": post_compaction_total_tokens,
            "post_compaction_window_start": post_compaction_window_start,
            "post_compaction_window_end": post_compaction_window_end,
            "effective_context_window_tokens": effective_context_window_tokens,
            "target_tokens": candidate.target_tokens,
            "trigger": compaction_trigger_detail_value(candidate.trigger),
            "strategy": strategy_used,
            "artifact_id": artifact_notice.get("artifact_id").and_then(|value| value.as_str()),
            "artifact_content": artifact_notice.get("artifact_content").and_then(|value| value.as_str()),
            "artifact_payload": artifact_notice.get("artifact_payload"),
            "artifact_strategy": artifact_notice.get("artifact_strategy"),
        })
        .to_string();

        self.persist_thread_by_id(thread_id).await;
        self.trim_participant_playground_threads_for_visible_thread(thread_id)
            .await;
        self.record_provenance_event(
            "context_compressed",
            match mode {
                CompactionCandidateMode::Automatic => {
                    "thread context was compacted for an LLM request"
                }
                CompactionCandidateMode::Forced => {
                    "thread context was compacted by operator request"
                }
            },
            serde_json::json!({
                "thread_id": thread_id,
                "split_at": split_at,
                "target_tokens": candidate.target_tokens,
                "trigger": compaction_trigger_detail_value(candidate.trigger),
                "message_count": message_count,
                "strategy": strategy_used,
                "forced": mode == CompactionCandidateMode::Forced,
            }),
            None,
            task_id,
            Some(thread_id),
            None,
            None,
        )
        .await;
        self.persist_context_compression_causal_trace(
            thread_id,
            task_id,
            split_at,
            message_count,
            candidate.target_tokens,
            strategy_used,
        )
        .await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });
        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: thread_id.to_string(),
            kind: match mode {
                CompactionCandidateMode::Automatic => COMPACTION_NOTICE_KIND,
                CompactionCandidateMode::Forced => MANUAL_COMPACTION_NOTICE_KIND,
            }
            .to_string(),
            message: format!(
                "{} compaction applied using {}. {}",
                match mode {
                    CompactionCandidateMode::Automatic => "Auto",
                    CompactionCandidateMode::Forced => "Manual",
                },
                serde_json::to_string(&strategy_used)
                    .unwrap_or_else(|_| "\"heuristic\"".to_string())
                    .trim_matches('"'),
                compaction_trigger_summary,
            ),
            details: Some(compaction_notice_details.clone()),
        });
        if let Some(fallback_notice) = fallback_notice {
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: match mode {
                    CompactionCandidateMode::Automatic => COMPACTION_NOTICE_KIND,
                    CompactionCandidateMode::Forced => MANUAL_COMPACTION_NOTICE_KIND,
                }
                .to_string(),
                message: fallback_notice,
                details: Some(compaction_notice_details),
            });
        }
        tracing::info!(
            thread_id,
            mode = ?mode,
            strategy = ?strategy_used,
            split_at = current_split_at,
            total_messages = total_message_count,
            pre_compaction_total_tokens,
            post_compaction_total_tokens,
            target_tokens = candidate.target_tokens,
            effective_context_window_tokens,
            "compaction broadcast: post-compaction snapshot computed and ContextWindowUpdate emitted"
        );
        let _ = self.event_tx.send(AgentEvent::ContextWindowUpdate {
            thread_id: thread_id.to_string(),
            active_context_window_start: post_compaction_window_start,
            active_context_window_end: post_compaction_window_end,
            active_context_window_tokens: post_compaction_total_tokens,
        });
        Ok(true)
    }

    pub(crate) async fn compaction_scope_snapshot(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
    ) -> Option<CompactionScopeSnapshot> {
        let (task_exists, task_goal_run_id) = if let Some(task_id) = task_id {
            match self.history.agent_task_goal_context(task_id).await {
                Ok(Some(context)) => (true, context.goal_run_id),
                Ok(None) => {
                    let tasks = self.tasks.lock().await;
                    let task = tasks.iter().find(|task| task.id == task_id);
                    (
                        task.is_some(),
                        task.and_then(|task| task.goal_run_id.clone()),
                    )
                }
                Err(error) => {
                    tracing::warn!(
                        task_id,
                        %error,
                        "failed to query task goal context for compaction scope"
                    );
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
                        Some(task) => (true, task.goal_run_id),
                        None => {
                            let tasks = self.tasks.lock().await;
                            let task = tasks.iter().find(|task| task.id == task_id);
                            (
                                task.is_some(),
                                task.and_then(|task| task.goal_run_id.clone()),
                            )
                        }
                    }
                }
            }
        } else {
            (false, None)
        };

        let goal_run = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .find(|goal_run| {
                    task_goal_run_id
                        .as_deref()
                        .is_some_and(|goal_run_id| goal_run.id == goal_run_id)
                        || goal_run_thread_matches(goal_run, thread_id)
                })
                .cloned()
        };

        if let Some(goal_run) = goal_run {
            return Some(compaction_scope_snapshot_from_goal_run(
                thread_id,
                task_id.map(ToOwned::to_owned),
                goal_run,
            ));
        }

        let mut persisted_goal_run = None;
        if persisted_goal_run.is_none() {
            if let Some(goal_run_id) = task_goal_run_id.as_deref() {
                persisted_goal_run = self
                    .history
                    .goal_run_compaction_scope_ref(goal_run_id)
                    .await
                    .ok()
                    .flatten();
            }
        }
        if persisted_goal_run.is_none() {
            persisted_goal_run = match self
                .history
                .latest_goal_run_id_for_thread_ids(&[thread_id.to_string()])
                .await
            {
                Ok(Some(goal_run_id)) => self
                    .history
                    .goal_run_compaction_scope_ref(&goal_run_id)
                    .await
                    .ok()
                    .flatten(),
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(
                        thread_id,
                        "failed to query latest persisted goal run for compaction scope: {error}"
                    );
                    None
                }
            };
        }

        match persisted_goal_run {
            Some(goal_run) => Some(compaction_scope_snapshot_from_goal_run_ref(
                thread_id,
                task_id.map(ToOwned::to_owned),
                goal_run,
            )),
            None if task_exists => Some(CompactionScopeSnapshot {
                thread_id: thread_id.to_string(),
                task_id: task_id.map(ToOwned::to_owned),
                active_task_id: task_id.map(ToOwned::to_owned),
                goal_run_id: task_goal_run_id,
                ..CompactionScopeSnapshot::default()
            }),
            None => None,
        }
    }

    pub async fn force_compact_and_continue(self: &Arc<Self>, thread_id: &str) -> Result<bool> {
        if !self.ensure_thread_messages_loaded(thread_id).await {
            anyhow::bail!("thread not found: {thread_id}");
        }

        if let Some(handled) = self.try_compact_claude_code_cli_session(thread_id).await? {
            return Ok(handled);
        }

        let latest_user_content = self.latest_visible_user_message_content(thread_id).await;
        let latest_user_content = latest_user_content
            .as_deref()
            .filter(|content| !content.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow::anyhow!("no user message available to continue after compaction")
            })?;
        let agent_id = self
            .active_agent_id_for_thread(thread_id)
            .await
            .unwrap_or_else(|| MAIN_AGENT_ID.to_string());
        // Find the task that owns this thread so its `override_provider` /
        // `override_model` flow into the continuation. Without this the
        // task-owned builtin-persona scenario (e.g. a Dazhbog task whose
        // overrides are the only configured provider/model for that
        // persona) fails to compact: the participant-observer resolver
        // sees the persona has no `config.builtin_sub_agents.dazhbog`
        // setup and short-circuits with `builtin_persona_setup_error`.
        let owning_task_id = self.task_id_owning_thread(thread_id).await;
        let continuation = DeferredVisibleThreadContinuation {
            agent_id,
            task_id: owning_task_id,
            preferred_session_hint: None,
            llm_user_content: latest_user_content,
            queued_at_ms: 0,
            force_compaction: true,
            rerun_participant_observers_after_turn: true,
            internal_delegate_sender: None,
            internal_delegate_message: None,
        };

        let was_streaming = {
            let streams = self.stream_cancellations.lock().await;
            streams.contains_key(thread_id)
        };
        tracing::info!(thread_id, was_streaming, "manual compaction requested");
        self.enqueue_visible_thread_continuation(thread_id, continuation)
            .await;

        let starting_message = if was_streaming {
            "Manual compaction requested; waiting for the current stream to stop."
        } else {
            "Manual compaction starting..."
        };
        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: thread_id.to_string(),
            kind: MANUAL_COMPACTION_NOTICE_KIND.to_string(),
            message: starting_message.to_string(),
            details: None,
        });

        if was_streaming {
            let stopped = self.stop_stream(thread_id).await;
            tracing::info!(
                thread_id,
                stopped,
                "manual compaction waiting for the active stream to leave"
            );
            self.wait_for_stream_slot_to_clear(thread_id).await;
        }

        self.flush_deferred_visible_thread_continuations(thread_id)
            .await?;
        Ok(true)
    }

    async fn wait_for_stream_slot_to_clear(&self, thread_id: &str) {
        for _ in 0..120 {
            if !self
                .stream_cancellations
                .lock()
                .await
                .contains_key(thread_id)
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        tracing::warn!(
            thread_id,
            "manual compaction is continuing while the previous stream slot is still registered"
        );
    }

    async fn try_compact_claude_code_cli_session(
        self: &Arc<Self>,
        thread_id: &str,
    ) -> Result<Option<bool>> {
        let (session_id, model) = {
            let threads = self.threads.read().await;
            let Some(thread) = threads.get(thread_id) else {
                return Ok(None);
            };
            if thread.upstream_provider.as_deref()
                != Some(zorai_shared::providers::PROVIDER_ID_CLAUDE_CODE_CLI)
            {
                return Ok(None);
            }
            (
                thread.upstream_thread_id.clone(),
                thread.upstream_model.clone(),
            )
        };

        let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) else {
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: MANUAL_COMPACTION_NOTICE_KIND.to_string(),
                message: "No Claude session yet; send a message before compacting.".to_string(),
                details: None,
            });
            return Ok(Some(false));
        };

        let working_dir = self
            .resolve_thread_repo_root(thread_id)
            .await
            .map(|(repo_root, _, _, _)| repo_root);

        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: thread_id.to_string(),
            kind: MANUAL_COMPACTION_NOTICE_KIND.to_string(),
            message: "Compacting Claude session...".to_string(),
            details: None,
        });

        let outcome = crate::agent::llm_client::compact_claude_code_cli_session(
            &session_id,
            model.as_deref(),
            working_dir.as_deref(),
        )
        .await?;

        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: thread_id.to_string(),
            kind: MANUAL_COMPACTION_NOTICE_KIND.to_string(),
            message: format!(
                "Claude session compacted ({} in / {} out tokens).",
                outcome.input_tokens, outcome.output_tokens
            ),
            details: None,
        });
        Ok(Some(true))
    }
}

fn goal_run_thread_matches(goal_run: &GoalRun, thread_id: &str) -> bool {
    goal_run.thread_id.as_deref() == Some(thread_id)
        || goal_run.root_thread_id.as_deref() == Some(thread_id)
        || goal_run.active_thread_id.as_deref() == Some(thread_id)
        || goal_run
            .execution_thread_ids
            .iter()
            .any(|execution_thread_id| execution_thread_id == thread_id)
        || thread_id == format!("goal:{}", goal_run.id)
}

fn compaction_scope_snapshot_from_goal_run(
    thread_id: &str,
    task_id: Option<String>,
    goal_run: GoalRun,
) -> CompactionScopeSnapshot {
    let task_id = task_id.or_else(|| goal_run.active_task_id.clone());
    let step = goal_run.steps.get(goal_run.current_step_index);
    let current_step_title = step
        .map(|step| step.title.clone())
        .or_else(|| goal_run.current_step_title.clone());
    let current_step_status = step.map(|step| format!("{:?}", step.status));
    let current_step_summary = step.and_then(|step| step.summary.clone());
    let recent_events = goal_run
        .events
        .iter()
        .rev()
        .take(3)
        .map(|event| event.message.clone())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();

    CompactionScopeSnapshot {
        thread_id: thread_id.to_string(),
        task_id,
        goal_run_id: Some(goal_run.id),
        active_task_id: goal_run.active_task_id,
        goal_title: Some(goal_run.title),
        goal: Some(goal_run.goal),
        goal_status: Some(format!("{:?}", goal_run.status)),
        root_thread_id: goal_run.root_thread_id,
        active_thread_id: goal_run.active_thread_id,
        execution_thread_ids: goal_run.execution_thread_ids,
        current_step_title,
        current_step_status,
        current_step_summary,
        plan_summary: goal_run.plan_summary,
        latest_error: goal_run.last_error.or(goal_run.failure_cause),
        recent_events,
    }
}

fn compaction_scope_snapshot_from_goal_run_ref(
    thread_id: &str,
    task_id: Option<String>,
    goal_run: crate::history::GoalRunCompactionScopeRef,
) -> CompactionScopeSnapshot {
    CompactionScopeSnapshot {
        thread_id: thread_id.to_string(),
        task_id: task_id.or_else(|| goal_run.active_task_id.clone()),
        goal_run_id: Some(goal_run.id),
        active_task_id: goal_run.active_task_id,
        goal_title: Some(goal_run.title),
        goal: Some(goal_run.goal),
        goal_status: Some(format!("{:?}", goal_run.status)),
        root_thread_id: goal_run.root_thread_id,
        active_thread_id: goal_run.active_thread_id,
        execution_thread_ids: goal_run.execution_thread_ids,
        current_step_title: goal_run.current_step_title,
        current_step_status: goal_run
            .current_step_status
            .map(|status| format!("{:?}", status)),
        current_step_summary: goal_run.current_step_summary,
        plan_summary: goal_run.plan_summary,
        latest_error: goal_run.latest_error,
        recent_events: goal_run.recent_events,
    }
}
