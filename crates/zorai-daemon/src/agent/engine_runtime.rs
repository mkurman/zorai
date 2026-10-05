//! Engine runtime — stream cancellation, repo watchers, and memory cache.

use super::*;

fn repo_watcher_contains_thread(entry: &ThreadRepoWatcher, thread_id: &str) -> bool {
    match entry.thread_ids.lock() {
        Ok(thread_ids) => thread_ids.contains(thread_id),
        Err(error) => {
            tracing::warn!(
                repo_root = %entry.repo_root,
                "repo watcher membership lock poisoned while checking thread membership: {error}"
            );
            false
        }
    }
}

fn repo_watcher_insert_thread(entry: &ThreadRepoWatcher, thread_id: &str) -> bool {
    match entry.thread_ids.lock() {
        Ok(mut thread_ids) => {
            thread_ids.insert(thread_id.to_string());
            true
        }
        Err(error) => {
            tracing::warn!(
                repo_root = %entry.repo_root,
                "repo watcher membership lock poisoned while adding thread membership: {error}"
            );
            false
        }
    }
}

fn repo_watcher_remove_thread(entry: &ThreadRepoWatcher, thread_id: &str) -> Option<bool> {
    match entry.thread_ids.lock() {
        Ok(mut thread_ids) => {
            thread_ids.remove(thread_id);
            Some(thread_ids.is_empty())
        }
        Err(error) => {
            tracing::warn!(
                repo_root = %entry.repo_root,
                "repo watcher membership lock poisoned while removing thread membership: {error}"
            );
            None
        }
    }
}

impl AgentEngine {
    pub(super) async fn ensure_subagent_runtime(&self, task: &AgentTask, thread_id: Option<&str>) {
        if !should_track_subagent(task) {
            return;
        }

        let now = now_millis();
        let inserted = {
            let mut runtime = self.subagent_runtime.write().await;
            if runtime.contains_key(&task.id) {
                false
            } else {
                runtime.insert(
                    task.id.clone(),
                    SubagentRuntimeStats {
                        task_id: task.id.clone(),
                        parent_task_id: task.parent_task_id.clone(),
                        thread_id: thread_id
                            .map(ToOwned::to_owned)
                            .or_else(|| task.thread_id.clone()),
                        started_at: task.started_at.unwrap_or(task.created_at),
                        created_at: task.created_at,
                        max_duration_secs: task.max_duration_secs,
                        context_budget_tokens: task.context_budget_tokens,
                        last_tool_call_at: None,
                        last_progress_at: None,
                        tool_calls_total: 0,
                        tool_calls_succeeded: 0,
                        tool_calls_failed: 0,
                        consecutive_errors: 0,
                        recent_tool_names: VecDeque::new(),
                        tokens_consumed: 0,
                        context_utilization_pct: 0,
                        health_state: SubagentHealthState::Healthy,
                        updated_at: now,
                    },
                );
                true
            }
        };
        if inserted {
            self.persist_subagent_runtime_metrics(&task.id).await;
        }
    }

    pub(super) async fn record_subagent_tool_result(
        &self,
        task: &AgentTask,
        thread_id: &str,
        tool_name: &str,
        is_error: bool,
        current_tokens: u32,
    ) {
        if !should_track_subagent(task) {
            return;
        }

        self.ensure_subagent_runtime(task, Some(thread_id)).await;

        let now = now_millis();
        let mut runtime = self.subagent_runtime.write().await;
        let Some(entry) = runtime.get_mut(&task.id) else {
            return;
        };

        entry.thread_id = Some(thread_id.to_string());
        entry.updated_at = now;
        entry.tokens_consumed = current_tokens;
        entry.context_utilization_pct = entry
            .context_budget_tokens
            .map(|budget| {
                if budget == 0 {
                    0
                } else {
                    ((current_tokens as u64 * 100) / budget as u64).min(100) as u32
                }
            })
            .unwrap_or(0);
        entry.last_tool_call_at = Some(now);
        entry.tool_calls_total = entry.tool_calls_total.saturating_add(1);
        entry.recent_tool_names.push_back(tool_name.to_string());
        while entry.recent_tool_names.len() > 8 {
            entry.recent_tool_names.pop_front();
        }

        if is_error {
            entry.tool_calls_failed = entry.tool_calls_failed.saturating_add(1);
            entry.consecutive_errors = entry.consecutive_errors.saturating_add(1);
        } else {
            entry.tool_calls_succeeded = entry.tool_calls_succeeded.saturating_add(1);
            entry.consecutive_errors = 0;
            entry.last_progress_at = Some(now);
        }
    }

    pub(super) async fn update_subagent_health(
        &self,
        task_id: &str,
        health_state: SubagentHealthState,
    ) {
        let mut runtime = self.subagent_runtime.write().await;
        if let Some(entry) = runtime.get_mut(task_id) {
            entry.health_state = health_state;
            entry.updated_at = now_millis();
        }
    }

    pub(super) async fn subagent_snapshot(
        &self,
        task: &AgentTask,
    ) -> Option<crate::agent::subagent::supervisor::SubagentSnapshot> {
        if !should_track_subagent(task) {
            return None;
        }

        self.ensure_subagent_runtime(task, task.thread_id.as_deref())
            .await;
        let runtime = self.subagent_runtime.read().await;
        let stats = runtime.get(&task.id)?;
        Some(crate::agent::subagent::supervisor::SubagentSnapshot {
            task_id: stats.task_id.clone(),
            last_tool_call_at: stats.last_tool_call_at.map(|ts| ts / 1000),
            tool_calls_total: stats.tool_calls_total,
            tool_calls_failed: stats.tool_calls_failed,
            consecutive_errors: stats.consecutive_errors,
            recent_tool_names: stats.recent_tool_names.iter().cloned().collect(),
            context_utilization_pct: stats.context_utilization_pct,
            started_at: stats.started_at / 1000,
            max_duration_secs: stats.max_duration_secs,
        })
    }

    pub(super) async fn persist_subagent_runtime_metrics(&self, task_id: &str) {
        let stats = {
            let runtime = self.subagent_runtime.read().await;
            runtime.get(task_id).cloned()
        };
        let Some(stats) = stats else {
            return;
        };

        let elapsed_secs =
            now_millis().saturating_sub(stats.started_at).max(1_000) as f64 / 1_000.0;
        let progress_rate = stats.tool_calls_succeeded as f64 / elapsed_secs;
        let failure_ratio = if stats.tool_calls_total == 0 {
            0.0
        } else {
            stats.tool_calls_failed as f64 / stats.tool_calls_total as f64
        };
        let stuck_score = ((stats.context_utilization_pct as f64 / 100.0) * 0.5)
            + failure_ratio * 0.3
            + ((stats.consecutive_errors.min(5) as f64) / 5.0) * 0.2;

        if let Err(e) = self
            .history
            .upsert_subagent_metrics(
                &stats.task_id,
                stats.parent_task_id.as_deref(),
                stats.thread_id.as_deref(),
                stats.tool_calls_total as i64,
                stats.tool_calls_succeeded as i64,
                stats.tool_calls_failed as i64,
                stats.tokens_consumed as i64,
                stats.context_budget_tokens.map(|v| v as i64),
                progress_rate,
                stats.last_progress_at,
                stuck_score,
                subagent_health_label(stats.health_state),
                stats.created_at,
                stats.updated_at,
            )
            .await
        {
            tracing::warn!(task_id = %stats.task_id, "failed to persist subagent metrics: {e}");
        }
    }

    pub async fn health_status_snapshot(&self) -> serde_json::Value {
        let health_task_statuses = [
            TaskStatus::Queued,
            TaskStatus::InProgress,
            TaskStatus::Blocked,
            TaskStatus::AwaitingApproval,
            TaskStatus::FailedAnalyzing,
        ]
        .into_iter()
        .filter_map(|status| {
            serde_json::to_value(status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
        })
        .collect::<Vec<_>>();
        let active_tasks = self
            .count_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: health_task_statuses,
                source: None,
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: false,
                order_by_recent_activity_desc: false,
                limit: None,
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await;
        let awaiting_approval_tasks = self
            .count_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: Some("awaiting_approval".to_string()),
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
                limit: None,
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await;
        let active_goal_statuses = [
            GoalRunStatus::Queued,
            GoalRunStatus::Planning,
            GoalRunStatus::Running,
            GoalRunStatus::AwaitingApproval,
            GoalRunStatus::AwaitingReview,
            GoalRunStatus::Paused,
        ];
        let mut active_goal_run_ids = match self
            .history
            .list_goal_run_ids_for_statuses(&active_goal_statuses)
            .await
        {
            Ok(goal_run_ids) => goal_run_ids,
            Err(error) => {
                tracing::warn!(
                    "failed to query persisted active goal run ids for health snapshot: {error}"
                );
                Vec::new()
            }
        };
        let mut seen_goal_ids = active_goal_run_ids
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        {
            let live_goal_runs = self.goal_runs.lock().await;
            for goal_run in live_goal_runs
                .iter()
                .filter(|goal_run| active_goal_statuses.contains(&goal_run.status))
            {
                if seen_goal_ids.insert(goal_run.id.clone()) {
                    active_goal_run_ids.push(goal_run.id.clone());
                }
            }
        }
        let active_goal_runs = active_goal_run_ids.len();

        let latest = self
            .history
            .list_health_log(1)
            .await
            .ok()
            .and_then(|items| items.into_iter().next());

        serde_json::json!({
            "state": latest.as_ref().map(|entry| entry.3.clone()).unwrap_or_else(|| "healthy".to_string()),
            "uptime_secs": now_millis().saturating_sub(self.started_at_ms) / 1000,
            "active_goal_runs": active_goal_runs,
            "active_tasks": active_tasks,
            "awaiting_approval_tasks": awaiting_approval_tasks,
            "latest_health_event_at": latest.as_ref().map(|entry| entry.6),
            "latest_health_entity_type": latest.as_ref().map(|entry| entry.1.clone()),
            "latest_health_entity_id": latest.as_ref().map(|entry| entry.2.clone()),
        })
    }

    pub async fn health_log_entries(&self, limit: u32) -> Result<Vec<serde_json::Value>> {
        let rows = self.history.list_health_log(limit).await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, entity_type, entity_id, health_state, indicators_json, intervention, created_at)| {
                    serde_json::json!({
                        "id": id,
                        "entity_type": entity_type,
                        "entity_id": entity_id,
                        "health_state": health_state,
                        "indicators": indicators_json.and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok()),
                        "intervention": intervention,
                        "created_at": created_at,
                    })
                },
            )
            .collect())
    }

    pub(crate) async fn begin_stream_cancellation(
        &self,
        thread_id: &str,
    ) -> (u64, CancellationToken, Arc<tokio::sync::Notify>) {
        let generation = self.stream_generation.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        let retry_now = Arc::new(tokio::sync::Notify::new());
        let now = now_millis();
        let mut streams = self.stream_cancellations.lock().await;
        self.stream_providers.lock().await.remove(thread_id);
        if let Some(previous) = streams.insert(
            thread_id.to_string(),
            StreamCancellationEntry {
                generation,
                token: token.clone(),
                retry_now: retry_now.clone(),
                started_at: now,
                last_progress_at: now,
                last_progress_kind: StreamProgressKind::Started,
                last_progress_excerpt: String::new(),
            },
        ) {
            previous.token.cancel();
        }
        (generation, token, retry_now)
    }

    pub(super) async fn tag_stream_provider(
        &self,
        thread_id: &str,
        generation: u64,
        provider: &str,
    ) {
        let streams = self.stream_cancellations.lock().await;
        let current = streams
            .get(thread_id)
            .is_some_and(|entry| entry.generation == generation);
        drop(streams);
        if current {
            self.stream_providers
                .lock()
                .await
                .insert(thread_id.to_string(), (generation, provider.to_string()));
        }
    }

    pub(super) async fn abort_streams_for_provider(&self, provider: &str) {
        let tagged = self.stream_providers.lock().await.clone();
        let tokens = {
            let streams = self.stream_cancellations.lock().await;
            tagged
                .into_iter()
                .filter_map(|(thread_id, (generation, tagged_provider))| {
                    if tagged_provider != provider {
                        return None;
                    }
                    streams.get(&thread_id).and_then(|entry| {
                        (entry.generation == generation).then(|| entry.token.clone())
                    })
                })
                .collect::<Vec<_>>()
        };
        for token in tokens {
            token.cancel();
        }
    }

    pub(super) async fn finish_stream_cancellation(&self, thread_id: &str, generation: u64) {
        let mut streams = self.stream_cancellations.lock().await;
        let should_remove = streams
            .get(thread_id)
            .map(|entry| entry.generation == generation)
            .unwrap_or(false);
        if should_remove {
            streams.remove(thread_id);
            drop(streams);
            let mut providers = self.stream_providers.lock().await;
            if providers
                .get(thread_id)
                .is_some_and(|(tagged_generation, _)| *tagged_generation == generation)
            {
                providers.remove(thread_id);
            }
            drop(providers);
            self.wake_prompt_queue(thread_id);
            return;
        }
    }

    pub(super) async fn note_stream_progress(
        &self,
        thread_id: &str,
        generation: u64,
        kind: StreamProgressKind,
        excerpt: &str,
    ) {
        let mut streams = self.stream_cancellations.lock().await;
        let Some(entry) = streams.get_mut(thread_id) else {
            return;
        };
        if entry.generation != generation {
            return;
        }
        entry.last_progress_at = now_millis();
        entry.last_progress_kind = kind;
        entry.last_progress_excerpt = excerpt.chars().take(180).collect();
    }

    pub(super) async fn stream_cancellation_requested(&self, thread_id: &str) -> bool {
        let streams = self.stream_cancellations.lock().await;
        streams
            .get(thread_id)
            .is_some_and(|entry| entry.token.is_cancelled())
    }

    pub async fn stop_stream(&self, thread_id: &str) -> bool {
        self.operator_stopped_streams
            .lock()
            .await
            .insert(thread_id.to_string());
        let token = {
            let streams = self.stream_cancellations.lock().await;
            streams.get(thread_id).map(|entry| entry.token.clone())
        };
        if let Some(token) = token {
            token.cancel();
            let _ = self.event_tx.send(AgentEvent::RetryStatus {
                thread_id: thread_id.to_string(),
                phase: "cleared".to_string(),
                attempt: 0,
                max_retries: 0,
                delay_ms: 0,
                failure_class: String::new(),
                message: String::new(),
            });
            true
        } else {
            false
        }
    }

    pub(crate) async fn clear_operator_stream_stop(&self, thread_id: &str) {
        self.operator_stopped_streams.lock().await.remove(thread_id);
    }

    pub(crate) async fn operator_stream_stop_requested(&self, thread_id: &str) -> bool {
        self.operator_stopped_streams
            .lock()
            .await
            .contains(thread_id)
    }

    pub async fn notify_stream_retry_waiters(&self, thread_id: &str) {
        let retry_now = {
            let streams = self.stream_cancellations.lock().await;
            streams.get(thread_id).map(|entry| entry.retry_now.clone())
        };
        if let Some(retry_now) = retry_now {
            retry_now.notify_waiters();
        }
    }

    pub async fn retry_stream_now(self: &Arc<Self>, thread_id: &str) -> bool {
        let active_retry_now = {
            let streams = self.stream_cancellations.lock().await;
            streams
                .get(thread_id)
                .and_then(|entry| (!entry.token.is_cancelled()).then(|| entry.retry_now.clone()))
        };

        if let Some(retry_now) = active_retry_now {
            tracing::info!(
                thread_id = %thread_id,
                "retry-now requested; notifying active stream waiters"
            );
            let _ = self.event_tx.send(AgentEvent::RetryStatus {
                thread_id: thread_id.to_string(),
                phase: "cleared".to_string(),
                attempt: 0,
                max_retries: 0,
                delay_ms: 0,
                failure_class: String::new(),
                message: String::new(),
            });
            retry_now.notify_one();
            return true;
        }

        let last_user_content = match self.history.latest_user_message_content(thread_id).await {
            Ok(message) => message.filter(|content| !content.trim().is_empty()),
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    "failed to load latest user message for retry-now: {error}"
                );
                None
            }
        };

        if let Some(content) = last_user_content {
            tracing::info!(
                thread_id = %thread_id,
                user_message_chars = content.len(),
                "retry-now requested; starting fresh resend from last user turn"
            );
            let _ = self.event_tx.send(AgentEvent::RetryStatus {
                thread_id: thread_id.to_string(),
                phase: "cleared".to_string(),
                attempt: 0,
                max_retries: 0,
                delay_ms: 0,
                failure_class: String::new(),
                message: String::new(),
            });
            let engine = Arc::clone(self);
            let thread_id = thread_id.to_string();
            tokio::spawn(async move {
                if let Err(error) = engine
                    .resend_existing_user_message(&thread_id, &content)
                    .await
                {
                    tracing::warn!(thread_id = %thread_id, error = %error, "retry-now fresh resend failed");
                }
            });
            true
        } else {
            false
        }
    }

    pub(super) fn schedule_repo_watcher_restore(
        self: &Arc<Self>,
        repo_watches: Vec<(String, String)>,
    ) {
        if repo_watches.is_empty() {
            return;
        }

        let engine = Arc::clone(self);
        tokio::spawn(async move {
            for (thread_id, repo_root) in repo_watches {
                if engine.repo_monitor_enabled_for_repo(&repo_root).await {
                    engine.ensure_repo_watcher(&thread_id, &repo_root).await;
                } else {
                    engine.remove_repo_watcher(&thread_id).await;
                }
            }
        });
    }

    pub(super) async fn ensure_repo_watcher(&self, thread_id: &str, repo_root: &str) {
        let normalized_root = std::fs::canonicalize(repo_root)
            .unwrap_or_else(|_| std::path::PathBuf::from(repo_root))
            .to_string_lossy()
            .to_string();

        let previous_root = {
            let watchers = self.repo_watchers.lock().await;
            watchers.iter().find_map(|(repo_root_key, entry)| {
                if repo_watcher_contains_thread(entry, thread_id) {
                    Some(repo_root_key.clone())
                } else {
                    None
                }
            })
        };

        if previous_root.as_deref() == Some(normalized_root.as_str()) {
            return;
        }

        let removed_previous = {
            let mut watchers = self.repo_watchers.lock().await;

            if let Some(previous_root) = previous_root.as_ref() {
                let should_remove = watchers
                    .get(previous_root)
                    .and_then(|entry| repo_watcher_remove_thread(entry, thread_id))
                    .unwrap_or(false);

                if should_remove {
                    watchers.remove(previous_root)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(entry) = removed_previous {
            tracing::info!(
                thread_id = %thread_id,
                repo_root = %entry.repo_root,
                "filesystem watcher removed"
            );
            drop(entry.watcher);
        }

        {
            let watchers = self.repo_watchers.lock().await;
            if let Some(existing) = watchers.get(&normalized_root) {
                if repo_watcher_insert_thread(existing, thread_id) {
                    return;
                }
            }
        }

        let refresh_tx = self.watcher_refresh_tx.clone();
        let callback_repo_root = normalized_root.clone();
        let callback_thread_ids =
            Arc::new(std::sync::Mutex::new(HashSet::from(
                [thread_id.to_string()],
            )));
        let watcher_thread_ids = callback_thread_ids.clone();
        let mut watcher = match notify::recommended_watcher(move |result: notify::Result<Event>| {
            match result {
                Ok(event) => {
                    if file_watch_event_is_relevant(&event) {
                        let thread_ids = match callback_thread_ids.lock() {
                            Ok(thread_ids) => thread_ids.iter().cloned().collect::<Vec<_>>(),
                            Err(error) => {
                                tracing::warn!(
                                    repo_root = %callback_repo_root,
                                    "filesystem watcher membership lock poisoned in callback: {error}"
                                );
                                Vec::new()
                            }
                        };

                        for thread_id in thread_ids {
                            let _ = refresh_tx.send(thread_id);
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        repo_root = %callback_repo_root,
                        "filesystem watcher error: {error}"
                    );
                }
            }
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    repo_root = %normalized_root,
                    "failed to create filesystem watcher: {error}"
                );
                return;
            }
        };

        if let Err(error) = watcher.watch(
            std::path::Path::new(&normalized_root),
            RecursiveMode::Recursive,
        ) {
            tracing::warn!(
                thread_id = %thread_id,
                repo_root = %normalized_root,
                "failed to watch repo root: {error}"
            );
            return;
        }

        tracing::info!(
            thread_id = %thread_id,
            repo_root = %normalized_root,
            "filesystem watcher attached"
        );

        let replaced = {
            let mut watchers = self.repo_watchers.lock().await;
            if let Some(existing) = watchers.get(&normalized_root) {
                if repo_watcher_insert_thread(existing, thread_id) {
                    true
                } else {
                    false
                }
            } else {
                watchers.insert(
                    normalized_root.clone(),
                    ThreadRepoWatcher {
                        repo_root: normalized_root,
                        thread_ids: watcher_thread_ids,
                        watcher,
                    },
                );
                return;
            }
        };

        if replaced {
            drop(watcher);
        }
    }

    pub(super) async fn remove_repo_watcher(&self, thread_id: &str) {
        let removed = {
            let mut watchers = self.repo_watchers.lock().await;
            let repo_root = watchers.iter().find_map(|(repo_root_key, entry)| {
                if repo_watcher_contains_thread(entry, thread_id) {
                    Some(repo_root_key.clone())
                } else {
                    None
                }
            });

            repo_root.and_then(|repo_root| {
                let should_remove = watchers
                    .get(&repo_root)
                    .and_then(|entry| repo_watcher_remove_thread(entry, thread_id))
                    .unwrap_or(false);

                if should_remove {
                    watchers.remove(&repo_root)
                } else {
                    None
                }
            })
        };

        if let Some(entry) = removed {
            tracing::info!(
                thread_id = %thread_id,
                repo_root = %entry.repo_root,
                "filesystem watcher removed"
            );
            drop(entry.watcher);
        }
    }

    pub(super) async fn refresh_memory_cache(&self) {
        let scope_id = current_agent_scope_id();
        self.refresh_memory_cache_for_scope(&scope_id).await;
    }

    pub(super) async fn refresh_memory_cache_for_scope(&self, scope_id: &str) {
        match load_memory_for_scope(&self.data_dir, scope_id).await {
            Ok(memory) => {
                self.memory
                    .write()
                    .await
                    .insert(scope_id.to_string(), memory);
            }
            Err(e) => {
                tracing::warn!(scope_id, "failed to ensure persistent memory files: {e}");
            }
        }
    }

    pub(super) async fn memory_snapshot_for_scope(&self, scope_id: &str) -> AgentMemory {
        {
            let memory = self.memory.read().await;
            if let Some(snapshot) = memory.get(scope_id) {
                return snapshot.clone();
            }
        }
        self.refresh_memory_cache_for_scope(scope_id).await;
        self.memory
            .read()
            .await
            .get(scope_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) async fn current_memory_snapshot(&self) -> AgentMemory {
        let scope_id = current_agent_scope_id();
        self.memory_snapshot_for_scope(&scope_id).await
    }

    pub(super) async fn onecontext_bootstrap_for_new_thread(
        &self,
        initial_message: &str,
    ) -> Option<String> {
        let trimmed = initial_message.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !aline_available() {
            return None;
        }

        let query = trimmed.to_string();

        let Some(query) = super::tool_executor::prepare_onecontext_search_query(
            &query,
            true,
            ONECONTEXT_BOOTSTRAP_QUERY_MAX_CHARS,
        ) else {
            return None;
        };

        let mut cmd = tokio::process::Command::new("aline");
        cmd.arg("search")
            .arg(&query)
            .arg("-t")
            .arg("session")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null());

        let output = match tokio::time::timeout(
            Duration::from_millis(ONECONTEXT_BOOTSTRAP_TIMEOUT_MS),
            cmd.output(),
        )
        .await
        {
            Ok(Ok(output)) if output.status.success() => output,
            _ => return None,
        };

        let raw = String::from_utf8_lossy(&output.stdout);
        let normalized = raw.trim();
        if normalized.is_empty() {
            return None;
        }

        Some(
            normalized
                .chars()
                .take(ONECONTEXT_BOOTSTRAP_OUTPUT_MAX_CHARS)
                .collect(),
        )
    }
}

fn should_track_subagent(task: &AgentTask) -> bool {
    task.source == "subagent"
        || task.parent_task_id.is_some()
        || task.supervisor_config.is_some()
        || task.sub_agent_def_id.is_some()
}

fn subagent_health_label(state: SubagentHealthState) -> &'static str {
    match state {
        SubagentHealthState::Healthy => "healthy",
        SubagentHealthState::Degraded => "degraded",
        SubagentHealthState::Stuck => "stuck",
        SubagentHealthState::Crashed => "crashed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn health_status_counts_persisted_active_tasks_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let mut task = engine
            .enqueue_task(
                "Persisted health task".to_string(),
                "task should count in health snapshot".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "test",
                None,
                None,
                Some("thread-health-status".to_string()),
                None,
            )
            .await;
        task.status = TaskStatus::AwaitingApproval;
        {
            let mut tasks = engine.tasks.lock().await;
            tasks.clear();
            tasks.push_back(task);
        }
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        let snapshot = engine.health_status_snapshot().await;

        assert_eq!(snapshot["active_tasks"].as_u64(), Some(1));
        assert_eq!(snapshot["awaiting_approval_tasks"].as_u64(), Some(1));
    }

    #[tokio::test]
    async fn health_status_snapshot_counts_persisted_active_goals_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine
            .start_goal_run(
                "Persisted health goal".to_string(),
                Some("Persisted health goal".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await;
        engine.goal_runs.lock().await.clear();

        let snapshot = engine.health_status_snapshot().await;

        assert_eq!(snapshot["active_goal_runs"].as_u64(), Some(1));
    }
}
