//! State persistence — hydration from disk/SQLite and serialization helpers.

use super::*;

const WELES_RUNTIME_CONTEXT_PREFIX: &str = "weles_runtime_context:";

pub(super) fn weles_runtime_context_key(task_id: &str) -> String {
    format!("{WELES_RUNTIME_CONTEXT_PREFIX}{task_id}")
}

pub(super) fn sanitize_task_for_external_view(task: &mut AgentTask) {
    if task.sub_agent_def_id.as_deref()
        == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    {
        if let Some(prompt) = task.override_system_prompt.as_deref() {
            let sanitized =
                crate::agent::weles_governance::strip_weles_internal_payload_markers(prompt);
            task.override_system_prompt = if sanitized.trim().is_empty() {
                None
            } else {
                Some(sanitized)
            };
        }
    }
}

/// Batched restore of WELES runtime context. The previous per-task form was
/// called per task in hydrate loops, doing one `get_consolidation_state`
/// await per Weles task — at daemon startup with N pending Weles tasks this
/// is N round-trips to the SQLite background thread. The batched version:
///
/// 1. Filters the candidate Weles tasks and computes their consolidation
///    keys in one pass (no awaits).
/// 2. Issues ONE batched `get_consolidation_states_batch` call.
/// 3. Applies the deserialized payloads back per task and acquires the
///    trusted_weles_tasks lock once for the whole batch.
async fn restore_weles_runtime_contexts_batch(engine: &AgentEngine, tasks: &mut [AgentTask]) {
    struct PendingRestore {
        idx: usize,
        key: String,
        prompt: String,
        scope: &'static str,
    }
    let mut pending: Vec<PendingRestore> = Vec::new();
    for (idx, task) in tasks.iter().enumerate() {
        if task.sub_agent_def_id.as_deref()
            != Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
        {
            continue;
        }
        let Some(prompt) = task.override_system_prompt.as_deref() else {
            continue;
        };
        if crate::agent::weles_governance::parse_weles_internal_override_payload(prompt).is_some() {
            continue;
        }
        let scope = if prompt.contains("Your current internal scope is vitality.") {
            crate::agent::agent_identity::WELES_VITALITY_SCOPE
        } else if prompt.contains("Your current internal scope is governance.") {
            crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE
        } else {
            continue;
        };
        pending.push(PendingRestore {
            idx,
            key: weles_runtime_context_key(&task.id),
            prompt: prompt.to_string(),
            scope,
        });
    }
    if pending.is_empty() {
        return;
    }

    let keys: Vec<String> = pending.iter().map(|p| p.key.clone()).collect();
    let states = match engine.history.get_consolidation_states_batch(&keys).await {
        Ok(map) => map,
        Err(error) => {
            tracing::warn!(?error, "failed to batch-load WELES runtime contexts");
            return;
        }
    };

    let mut to_trust: Vec<String> = Vec::new();
    for restore in &pending {
        let Some(raw_context) = states.get(&restore.key) else {
            continue;
        };
        if raw_context.trim().is_empty() {
            continue;
        }
        let Ok(inspection_context) = serde_json::from_str::<serde_json::Value>(raw_context) else {
            tracing::warn!(
                task_id = %tasks[restore.idx].id,
                "failed to parse persisted WELES runtime context"
            );
            continue;
        };
        let Some(internal_payload) =
            crate::agent::weles_governance::build_weles_internal_override_payload(
                restore.scope,
                &inspection_context,
            )
        else {
            tracing::warn!(
                task_id = %tasks[restore.idx].id,
                "failed to rebuild WELES runtime payload from persisted context"
            );
            continue;
        };
        let task = &mut tasks[restore.idx];
        task.override_system_prompt = Some(format!("{}\n\n{internal_payload}", restore.prompt));
        to_trust.push(task.id.clone());
    }
    if !to_trust.is_empty() {
        let mut guard = engine.trusted_weles_tasks.write().await;
        for id in to_trust {
            guard.insert(id);
        }
    }
}

mod save;

async fn canonicalize_aline_startup_repo_root(repo_root: &str) -> Option<String> {
    let show_toplevel = tokio::time::timeout(
        Duration::from_secs(1),
        tokio::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(repo_root)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !show_toplevel.status.success() {
        return None;
    }
    let normalized_root = String::from_utf8_lossy(&show_toplevel.stdout)
        .trim()
        .to_string();
    if normalized_root.is_empty() {
        return None;
    }

    let common_dir_output = tokio::time::timeout(
        Duration::from_secs(1),
        tokio::process::Command::new("git")
            .args(["rev-parse", "--git-common-dir"])
            .current_dir(&normalized_root)
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    if common_dir_output.status.success() {
        let common_dir = String::from_utf8_lossy(&common_dir_output.stdout)
            .trim()
            .to_string();
        if !common_dir.is_empty() {
            let resolved_common_dir = std::path::Path::new(&normalized_root).join(&common_dir);
            if let Ok(canonical_common_dir) = std::fs::canonicalize(&resolved_common_dir) {
                if canonical_common_dir
                    .file_name()
                    .is_some_and(|name| name == std::ffi::OsStr::new(".git"))
                {
                    if let Some(parent) = canonical_common_dir.parent() {
                        return Some(parent.to_string_lossy().to_string());
                    }
                }
                return Some(canonical_common_dir.to_string_lossy().to_string());
            }
        }
    }

    Some(normalized_root)
}

impl AgentEngine {
    /// Load persisted state (threads, tasks, heartbeat, memory, config).
    pub async fn hydrate(self: &Arc<Self>) -> Result<()> {
        self.hydrate_with_participant_observer_restore(true).await
    }

    pub(crate) async fn hydrate_without_participant_observer_restore(
        self: &Arc<Self>,
    ) -> Result<()> {
        self.hydrate_with_participant_observer_restore(false).await
    }

    async fn hydrate_with_participant_observer_restore(
        self: &Arc<Self>,
        schedule_participant_observer_restore: bool,
    ) -> Result<()> {
        match self.history.list_agent_config_items().await {
            Ok(items) if !items.is_empty() => {
                match super::config::load_config_from_items_with_weles_cleanup(items) {
                    Ok((cfg, collisions)) => {
                        self.persist_sanitized_config(cfg, collisions).await;
                    }
                    Err(error) => {
                        tracing::warn!("failed to load agent config from sqlite: {error}")
                    }
                }
            }
            Ok(_) => {}
            Err(error) => tracing::warn!("failed to read agent config items from sqlite: {error}"),
        }

        let most_recent_thread_context_hint = match self.history.latest_thread_context_hint().await
        {
            Ok(hint) => hint,
            Err(error) => {
                tracing::warn!(
                    %error,
                    "failed to select latest thread context hint from sqlite"
                );
                None
            }
        };

        match self.history.list_threads().await {
            Ok(thread_rows) if !thread_rows.is_empty() => {
                let mut threads = HashMap::new();
                let thread_ids = thread_rows
                    .iter()
                    .map(|thread_row| thread_row.id.clone())
                    .collect::<Vec<_>>();
                let thread_structural_memories = match self
                    .history
                    .list_thread_structural_memory_for_threads(&thread_ids)
                    .await
                {
                    Ok(rows) => rows
                        .into_iter()
                        .filter_map(|row| {
                            match serde_json::from_value::<
                                crate::agent::context::structural_memory::ThreadStructuralMemory,
                            >(row.state_json)
                            {
                                Ok(state) => Some((row.thread_id, state)),
                                Err(error) => {
                                    tracing::warn!(
                                        thread_id = %row.thread_id,
                                        %error,
                                        "failed to hydrate thread structural memory"
                                    );
                                    None
                                }
                            }
                        })
                        .collect::<HashMap<_, _>>(),
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            "failed to bulk hydrate thread structural memory"
                        );
                        HashMap::new()
                    }
                };
                let mut thread_message_hydration_pending = HashSet::new();
                let mut handoff_states = HashMap::new();
                let mut thread_participants = HashMap::new();
                let mut thread_participant_suggestions = HashMap::new();
                let mut thread_client_surfaces = HashMap::new();
                let mut thread_execution_profiles = HashMap::new();
                let mut thread_identity_metadata = HashMap::new();
                let mut thread_skill_discovery_states = HashMap::new();
                let mut thread_memory_injection_states = HashMap::new();
                let mut deferred_metadata_compactions: Vec<(String, String)> = Vec::new();
                for thread_row in thread_rows {
                    let thread_id = thread_row.id.clone();
                    let thread_title = thread_row.title.clone();
                    let thread_metadata =
                        parse_thread_metadata(thread_row.metadata_json.as_deref());
                    if let Some(raw_metadata) = thread_row.metadata_json.as_deref() {
                        if let Some(compacted_metadata) =
                            compact_thread_metadata_skill_discovery_query(raw_metadata)
                        {
                            deferred_metadata_compactions
                                .push((thread_id.clone(), compacted_metadata));
                        }
                    }
                    if let Some(client_surface) = thread_metadata.client_surface {
                        thread_client_surfaces.insert(thread_id.clone(), client_surface);
                    }
                    if let Some(execution_profile) = thread_metadata.execution_profile.clone() {
                        thread_execution_profiles.insert(thread_id.clone(), execution_profile);
                    }
                    if let Some(identity) = thread_metadata.identity.clone() {
                        thread_identity_metadata.insert(thread_id.clone(), identity);
                    }
                    if let Some(latest_skill_discovery_state) =
                        thread_metadata.latest_skill_discovery_state.clone()
                    {
                        thread_skill_discovery_states
                            .insert(thread_id.clone(), latest_skill_discovery_state);
                    }
                    if let Some(prompt_memory_injection_state) =
                        thread_metadata.prompt_memory_injection_state.clone()
                    {
                        thread_memory_injection_states
                            .insert(thread_id.clone(), prompt_memory_injection_state);
                    }
                    if !thread_metadata.thread_participants.is_empty() {
                        thread_participants.insert(
                            thread_id.clone(),
                            thread_metadata.thread_participants.clone(),
                        );
                    }
                    if !thread_metadata.thread_participant_suggestions.is_empty() {
                        thread_participant_suggestions.insert(
                            thread_id.clone(),
                            thread_metadata.thread_participant_suggestions.clone(),
                        );
                    }
                    let handoff_state = normalized_thread_handoff_state(
                        &thread_id,
                        thread_row.agent_name.as_deref(),
                        thread_row.created_at as u64,
                        thread_metadata.handoff_state,
                    );
                    let hydrated_agent_name = visible_thread_owner_agent_name_for_handoff_state(
                        &thread_id,
                        &handoff_state,
                        !thread_metadata.thread_participants.is_empty(),
                    )
                    .or_else(|| {
                        thread_row
                            .agent_name
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_string)
                    })
                    .or_else(|| {
                        handoff_state
                            .responder_stack
                            .last()
                            .map(|frame| frame.agent_name.trim())
                            .filter(|value| !value.is_empty())
                            .map(str::to_string)
                    });
                    let total_tokens = thread_row.total_tokens.max(0) as u64;
                    if thread_row.message_count > 0 {
                        thread_message_hydration_pending.insert(thread_id.clone());
                    }

                    threads.insert(
                        thread_id.clone(),
                        AgentThread {
                            id: thread_id.clone(),
                            agent_name: hydrated_agent_name,
                            title: thread_title,
                            messages: Vec::new(),
                            pinned: thread_metadata.pinned,
                            upstream_thread_id: thread_metadata.upstream_thread_id,
                            upstream_transport: thread_metadata.upstream_transport,
                            upstream_provider: thread_metadata.upstream_provider,
                            upstream_model: thread_metadata.upstream_model,
                            upstream_assistant_id: thread_metadata.upstream_assistant_id,
                            created_at: thread_row.created_at as u64,
                            updated_at: thread_row.updated_at as u64,
                            total_input_tokens: total_tokens,
                            total_output_tokens: 0,
                        },
                    );
                    handoff_states.insert(thread_id, handoff_state);
                }
                *self.threads.write().await = threads;
                *self.thread_message_hydration_pending.write().await =
                    thread_message_hydration_pending;
                *self.thread_handoff_states.write().await = handoff_states;
                *self.thread_participants.write().await = thread_participants;
                *self.thread_participant_suggestions.write().await = thread_participant_suggestions;
                *self.thread_client_surfaces.write().await = thread_client_surfaces;
                *self.thread_execution_profiles.write().await = thread_execution_profiles;
                *self.thread_identity_metadata.write().await = thread_identity_metadata;
                *self.thread_skill_discovery_states.write().await = thread_skill_discovery_states;
                *self.thread_memory_injection_state_map().write().await =
                    thread_memory_injection_states;
                *self.thread_structural_memories.write().await = thread_structural_memories;
                let trimmed_playground_threads = self
                    .trim_persisted_participant_playground_threads_on_hydrate()
                    .await;
                if trimmed_playground_threads > 0 {
                    tracing::info!(
                        trimmed_playground_threads,
                        "trimmed persisted participant playground threads during hydrate"
                    );
                }
                if schedule_participant_observer_restore {
                    self.schedule_participant_observer_restore_after_hydrate();
                }

                if !deferred_metadata_compactions.is_empty() {
                    let history = self.history.clone();
                    let pending = deferred_metadata_compactions.len();
                    tokio::spawn(async move {
                        for (thread_id, compacted_metadata) in deferred_metadata_compactions {
                            if let Err(error) = history
                                .update_thread_metadata_json(&thread_id, Some(compacted_metadata))
                                .await
                            {
                                tracing::warn!(
                                    thread_id = %thread_id,
                                    %error,
                                    "failed to compact persisted thread skill discovery metadata"
                                );
                            }
                        }
                        tracing::debug!(
                            pending,
                            "completed deferred thread-metadata compactions after hydrate"
                        );
                    });
                }
            }
            Ok(_) => {
                *self.thread_message_hydration_pending.write().await = HashSet::new();
                *self.thread_execution_profiles.write().await = HashMap::new();
                *self.thread_identity_metadata.write().await = HashMap::new();
            }
            Err(e) => tracing::warn!("failed to load agent threads from sqlite: {e}"),
        }

        match self
            .history
            .list_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: None,
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: true,
                order_by_recent_activity_desc: false,
                limit: None,
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await
        {
            Ok(mut tasks) if !tasks.is_empty() => {
                for task in &mut tasks {
                    sanitize_task_for_external_view(task);
                }
                restore_weles_runtime_contexts_batch(self, &mut tasks).await;
                for task in &mut tasks {
                    if task.status == TaskStatus::InProgress {
                        task.status = TaskStatus::Queued;
                        task.started_at = None;
                        task.lane_id = None;
                        task.logs.push(make_task_log_entry(
                            task.retry_count,
                            TaskLogLevel::Warn,
                            "hydrate",
                            "daemon restarted while task was in progress; task re-queued",
                            None,
                        ));
                    }
                }
                *self.tasks.lock().await = tasks.into_iter().collect();
                self.persist_tasks().await;
            }
            Ok(_) => {
                let has_sqlite_tasks = self
                    .history
                    .count_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
                        id: None,
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
                    .map(|count| count > 0)
                    .unwrap_or(false);
                let tasks_path = self.data_dir.join("tasks.json");
                if !has_sqlite_tasks && tasks_path.exists() {
                    match tokio::fs::read_to_string(&tasks_path).await {
                        Ok(raw) => {
                            if let Ok(mut tasks) = serde_json::from_str::<VecDeque<AgentTask>>(&raw)
                            {
                                for task in tasks.iter_mut() {
                                    sanitize_task_for_external_view(task);
                                }
                                let mut tasks_vec: Vec<AgentTask> = tasks.into_iter().collect();
                                restore_weles_runtime_contexts_batch(self, &mut tasks_vec).await;
                                for task in tasks_vec.iter_mut() {
                                    if task.status == TaskStatus::InProgress {
                                        task.status = TaskStatus::Queued;
                                        task.started_at = None;
                                    }
                                    task.max_retries = task.max_retries.max(1);
                                }
                                *self.tasks.lock().await = tasks_vec.into_iter().collect();
                                self.persist_tasks().await;
                            }
                        }
                        Err(e) => tracing::warn!("failed to migrate legacy agent tasks: {e}"),
                    }
                }
            }
            Err(e) => tracing::warn!("failed to load agent tasks from sqlite: {e}"),
        }

        let mut pause_interrupted_goal_runs_in_memory = false;
        let paused_goal_runs_on_restart = match self
            .history
            .pause_interrupted_goal_runs_on_restart(now_millis())
            .await
        {
            Ok(paused_count) => paused_count,
            Err(error) => {
                tracing::warn!(
                    "failed to pause interrupted goal runs in sqlite on restart: {error}"
                );
                pause_interrupted_goal_runs_in_memory = true;
                0
            }
        };

        match self
            .history
            .list_goal_runs_for_statuses(&[
                GoalRunStatus::Queued,
                GoalRunStatus::Planning,
                GoalRunStatus::Running,
                GoalRunStatus::AwaitingApproval,
                GoalRunStatus::AwaitingReview,
                GoalRunStatus::Paused,
            ])
            .await
        {
            Ok(goal_runs) if !goal_runs.is_empty() => {
                let mut runs: VecDeque<GoalRun> = goal_runs.into_iter().collect();
                let mut paused_count = paused_goal_runs_on_restart;

                if pause_interrupted_goal_runs_in_memory {
                    for goal_run in runs.iter_mut() {
                        if matches!(
                            goal_run.status,
                            GoalRunStatus::Running | GoalRunStatus::Planning
                        ) {
                            goal_run.status = GoalRunStatus::Paused;
                            goal_run.events.push(GoalRunEvent {
                                id: uuid::Uuid::new_v4().to_string(),
                                timestamp: now_millis(),
                                phase: "restart".to_string(),
                                message: "Daemon restarted; goal run paused for operator review."
                                    .to_string(),
                                details: None,
                                step_index: None,
                                todo_snapshot: Vec::new(),
                            });
                            paused_count += 1;
                        }
                    }
                }

                if paused_count > 0 {
                    tracing::info!(
                        paused_count,
                        "paused interrupted goal runs on restart (D-11)"
                    );
                }

                *self.goal_runs.lock().await = runs;
                if pause_interrupted_goal_runs_in_memory {
                    self.persist_goal_runs_in_background();
                }
            }
            Ok(_) => {
                let has_sqlite_goal_runs = self
                    .history
                    .count_goal_runs()
                    .await
                    .map(|count| count > 0)
                    .unwrap_or(false);
                let goal_runs_path = self.data_dir.join("goal-runs.json");
                if !has_sqlite_goal_runs && goal_runs_path.exists() {
                    match tokio::fs::read_to_string(&goal_runs_path).await {
                        Ok(raw) => {
                            if let Ok(goal_runs) = serde_json::from_str::<VecDeque<GoalRun>>(&raw) {
                                *self.goal_runs.lock().await = goal_runs;
                                self.persist_goal_runs_in_background();
                            }
                        }
                        Err(e) => tracing::warn!("failed to migrate legacy goal runs: {e}"),
                    }
                }
            }
            Err(e) => tracing::warn!("failed to load goal runs from sqlite: {e}"),
        }

        match self.history.list_agent_wakeups().await {
            Ok(rows) if !rows.is_empty() => {
                let loaded = rows.len();
                let mut wakeups = self.timer_wakeups.lock().await;
                for row in rows {
                    wakeups.insert(
                        row.id.clone(),
                        crate::agent::agent_wakeup::AgentWakeup {
                            id: row.id,
                            thread_id: row.thread_id,
                            message: row.message,
                            interval_ms: row.interval_ms,
                            next_fire_at: row.next_fire_at,
                            repetitions_remaining: row.repetitions_remaining,
                            wakeup_kind: row.wakeup_kind,
                            goal_run_id: row.goal_run_id,
                            reason: row.reason,
                            created_at: row.created_at,
                        },
                    );
                }
                tracing::info!(loaded_wakeups = loaded, "hydrated persisted agent wakeups");
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("failed to load agent wakeups from sqlite: {e}"),
        }

        let todos_path = self.data_dir.join("todos.json");
        if todos_path.exists() {
            match tokio::fs::read_to_string(&todos_path).await {
                Ok(raw) => {
                    if let Ok(items) = serde_json::from_str::<HashMap<String, Vec<TodoItem>>>(&raw)
                    {
                        *self.thread_todos.write().await = items;
                    }
                }
                Err(e) => tracing::warn!("failed to load thread todos: {e}"),
            }
        }

        let task_approval_rules_path = self.data_dir.join("task-approval-rules.json");
        if task_approval_rules_path.exists() {
            match tokio::fs::read_to_string(&task_approval_rules_path).await {
                Ok(raw) => {
                    match serde_json::from_str::<Vec<zorai_protocol::TaskApprovalRule>>(&raw) {
                        Ok(mut rules) => {
                            rules.sort_by(|left, right| left.command.cmp(&right.command));
                            *self.task_approval_rules.write().await = rules;
                        }
                        Err(error) => tracing::warn!(
                            %error,
                            "failed to parse persisted task approval rules"
                        ),
                    }
                }
                Err(error) => tracing::warn!(
                    %error,
                    "failed to read persisted task approval rules"
                ),
            }
        }

        let work_context_path = self.data_dir.join("work-context.json");
        if work_context_path.exists() {
            match tokio::fs::read_to_string(&work_context_path).await {
                Ok(raw) => {
                    if let Ok(items) =
                        serde_json::from_str::<HashMap<String, ThreadWorkContext>>(&raw)
                    {
                        *self.thread_work_contexts.write().await = items;
                    }
                }
                Err(e) => tracing::warn!("failed to load thread work context: {e}"),
            }
        }

        match self.history.list_gateway_thread_bindings().await {
            Ok(bindings) if !bindings.is_empty() => {
                let mut map: HashMap<String, String> = HashMap::new();
                for (channel_key, thread_id) in bindings {
                    map.entry(
                        crate::agent::gateway_loop::canonical_gateway_channel_key(
                            &channel_key,
                        ),
                    )
                    .or_insert(thread_id);
                }
                *self.gateway_threads.write().await = map;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("failed to load gateway thread map from sqlite: {e}"),
        }
        match self.history.list_gateway_route_modes().await {
            Ok(modes) if !modes.is_empty() => {
                let map: HashMap<String, gateway::GatewayRouteMode> = modes
                    .into_iter()
                    .map(|(channel_key, route_mode)| {
                        (channel_key, gateway::GatewayRouteMode::parse(&route_mode))
                    })
                    .collect();
                *self.gateway_route_modes.write().await = map;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("failed to load gateway route modes from sqlite: {e}"),
        }

        let legacy_gateway_threads_path = self.data_dir.join("gateway-threads.json");
        if legacy_gateway_threads_path.exists() {
            match tokio::fs::read_to_string(&legacy_gateway_threads_path).await {
                Ok(raw) => match serde_json::from_str::<HashMap<String, String>>(&raw) {
                    Ok(items) => {
                        let now = now_millis();
                        let mut imported = 0usize;
                        let mut failed = 0usize;
                        for (channel_key, thread_id) in &items {
                            match self
                                .history
                                .upsert_gateway_thread_binding(channel_key, thread_id, now)
                                .await
                            {
                                Ok(_) => imported += 1,
                                Err(error) => {
                                    failed += 1;
                                    tracing::warn!(
                                        channel_key = %channel_key,
                                        thread_id = %thread_id,
                                        %error,
                                        "failed to migrate legacy gateway thread binding"
                                    );
                                }
                            }
                        }

                        if failed == 0 {
                            if let Err(error) =
                                tokio::fs::remove_file(&legacy_gateway_threads_path).await
                            {
                                tracing::warn!(%error, "failed to remove legacy gateway thread map file after migration");
                            } else {
                                tracing::info!(
                                    imported,
                                    "migrated legacy gateway thread map into sqlite"
                                );
                            }
                        } else {
                            tracing::warn!(
                                imported,
                                failed,
                                "legacy gateway thread map migration partially failed; keeping legacy file"
                            );
                        }

                        if !items.is_empty() {
                            match self.history.list_gateway_thread_bindings().await {
                                Ok(bindings) if !bindings.is_empty() => {
                                    let map: HashMap<String, String> =
                                        bindings.into_iter().collect();
                                    *self.gateway_threads.write().await = map;
                                }
                                Ok(_) => {}
                                Err(error) => tracing::warn!(
                                    %error,
                                    "failed to reload gateway thread bindings from sqlite after migration"
                                ),
                            }
                        }
                    }
                    Err(error) => tracing::warn!(
                        %error,
                        "failed to parse legacy gateway-threads.json for migration"
                    ),
                },
                Err(error) => tracing::warn!(
                    %error,
                    "failed to read legacy gateway-threads.json for migration"
                ),
            }
        }

        match self.history.list_operator_profile_sessions().await {
            Ok(rows) if !rows.is_empty() => {
                let mut sessions = HashMap::new();
                for row in rows {
                    match serde_json::from_str::<OperatorProfileSessionState>(&row.session_json) {
                        Ok(session) => {
                            sessions.insert(row.session_id, session);
                        }
                        Err(error) => tracing::warn!(
                            session_id = %row.session_id,
                            kind = %row.kind,
                            updated_at = row.updated_at,
                            %error,
                            "failed to hydrate operator profile session"
                        ),
                    }
                }
                *self.operator_profile_sessions.write().await = sessions;
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!("failed to load operator profile sessions from sqlite: {error}")
            }
        }

        let heartbeat_path = self.data_dir.join("heartbeat.json");
        if heartbeat_path.exists() {
            match tokio::fs::read_to_string(&heartbeat_path).await {
                Ok(raw) => {
                    if let Ok(items) = serde_json::from_str::<Vec<HeartbeatItem>>(&raw) {
                        *self.heartbeat_items.write().await = items;
                    }
                }
                Err(e) => tracing::warn!("failed to load heartbeat items: {e}"),
            }
        }

        ensure_memory_files(&self.data_dir).await?;
        self.refresh_memory_cache().await;
        self.refresh_operator_model().await?;
        match self.load_meta_cognitive_self_model().await {
            Ok(model) => {
                *self.meta_cognitive_self_model.write().await = model;
            }
            Err(error) => {
                tracing::warn!(%error, "failed to hydrate meta-cognitive self-model");
            }
        }
        match self.history.list_collaboration_sessions().await {
            Ok(rows) if !rows.is_empty() => {
                let mut collaboration = HashMap::new();
                for row in rows {
                    match serde_json::from_str::<super::collaboration::CollaborationSession>(
                        &row.session_json,
                    ) {
                        Ok(session) => {
                            collaboration.insert(row.parent_task_id, session);
                        }
                        Err(error) => tracing::warn!(
                            parent_task_id = %row.parent_task_id,
                            "failed to hydrate collaboration session: {error}"
                        ),
                    }
                }
                *self.collaboration.write().await = collaboration;
            }
            Ok(_) => {}
            Err(error) => tracing::warn!("failed to load collaboration sessions: {error}"),
        }

        self.schedule_builtin_skill_seed_and_catalog_sync();

        let heuristic_path = self.data_dir.join("heuristics.json");
        if heuristic_path.exists() {
            match tokio::fs::read_to_string(&heuristic_path).await {
                Ok(json) => match serde_json::from_str(&json) {
                    Ok(store) => {
                        *self.heuristic_store.write().await = store;
                        tracing::info!(
                            "restored heuristic store from {}",
                            heuristic_path.display()
                        );
                    }
                    Err(e) => tracing::warn!(
                        error = %e,
                        "failed to parse heuristics.json, using defaults"
                    ),
                },
                Err(e) => tracing::warn!(error = %e, "failed to read heuristics.json"),
            }
        }

        let pattern_path = self.data_dir.join("patterns.json");
        if pattern_path.exists() {
            match tokio::fs::read_to_string(&pattern_path).await {
                Ok(json) => match serde_json::from_str(&json) {
                    Ok(store) => {
                        *self.pattern_store.write().await = store;
                        tracing::info!("restored pattern store from {}", pattern_path.display());
                    }
                    Err(e) => tracing::warn!(
                        error = %e,
                        "failed to parse patterns.json, using defaults"
                    ),
                },
                Err(e) => tracing::warn!(error = %e, "failed to read patterns.json"),
            }
        }

        if let Some((ref thread_id, ref last_topic, _updated_at)) = most_recent_thread_context_hint
        {
            match self
                .history
                .list_context_archive_entries(&thread_id, 20)
                .await
            {
                Ok(rows) if !rows.is_empty() => {
                    let entries: Vec<super::context::archive::ArchiveEntry> = rows
                        .into_iter()
                        .map(|row| super::context::archive::ArchiveEntry {
                            id: row.id,
                            thread_id: row.thread_id,
                            original_role: row.original_role,
                            compressed_content: row.compressed_content,
                            summary: row.summary,
                            relevance_score: row.relevance_score,
                            token_count_original: row.token_count_original as u32,
                            token_count_compressed: row.token_count_compressed as u32,
                            metadata: row
                                .metadata_json
                                .and_then(|j| serde_json::from_str(&j).ok()),
                            archived_at: row.archived_at as u64,
                            last_accessed_at: row.last_accessed_at.map(|v| v as u64),
                        })
                        .collect();

                    let request = super::context::restoration::RestorationRequest {
                        thread_id: thread_id.clone(),
                        query: Some(last_topic.clone()),
                        max_items: 10,
                        max_tokens: 2000,
                    };
                    let restored = super::context::restoration::rank_and_select(&entries, &request);
                    if !restored.is_empty() {
                        tracing::info!(
                            thread_id = %thread_id,
                            items = restored.len(),
                            "restored context for most recent thread (D-10)"
                        );

                        self.history
                            .set_consolidation_state(
                                "continuity_thread_id",
                                &thread_id,
                                now_millis(),
                            )
                            .await
                            .ok();
                        self.history
                            .set_consolidation_state("continuity_topic", &last_topic, now_millis())
                            .await
                            .ok();
                    }
                }
                Ok(_) => {
                    tracing::debug!(
                        "no archive entries for most recent thread, skipping context restoration"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "failed to list archive entries for context restoration"
                    );
                }
            }
        }

        let repo_watches = {
            let contexts = self.thread_work_contexts.read().await;
            contexts
                .iter()
                .filter_map(|(thread_id, context)| {
                    context
                        .entries
                        .iter()
                        .find_map(|entry| entry.repo_root.clone())
                        .map(|repo_root| (thread_id.clone(), repo_root))
                })
                .collect::<Vec<_>>()
        };
        self.schedule_repo_watcher_restore(repo_watches);

        let startup_repo_roots = self.collect_aline_startup_repo_roots().await;

        tracing::info!("agent engine hydrated from {:?}", self.data_dir);

        self.schedule_gateway_startup();

        match startup_repo_roots.as_slice() {
            [repo_root] => self.schedule_aline_startup_reconciliation(repo_root.clone()),
            [] => {
                let mut summary = super::aline_startup::AlineStartupSummary::skipped(
                    super::aline_startup::AlineStartupShortCircuitReason::NoRepoRoots,
                );
                summary.aline_available = self.aline_startup_is_available();
                self.record_aline_startup_summary(summary.clone()).await;
                super::log_aline_startup_summary(std::path::Path::new("<none>"), &summary);
                tracing::info!(
                    short_circuit_reason = "no_repo_roots",
                    repo_root_count = 0,
                    "skipping Aline startup reconciliation because no repo root was resolved during hydrate"
                );
                self.schedule_aline_watcher_bootstrap();
            }
            repo_roots => {
                let mut summary = super::aline_startup::AlineStartupSummary::skipped(
                    super::aline_startup::AlineStartupShortCircuitReason::MultipleRepoRoots,
                );
                summary.aline_available = self.aline_startup_is_available();
                self.record_aline_startup_summary(summary.clone()).await;
                super::log_aline_startup_summary(std::path::Path::new("<none>"), &summary);
                tracing::info!(
                    short_circuit_reason = "multiple_repo_roots",
                    repo_root_count = repo_roots.len(),
                    "skipping Aline startup reconciliation because multiple repo roots were resolved during hydrate"
                );
                self.schedule_aline_watcher_bootstrap();
            }
        }

        Ok(())
    }

    async fn collect_aline_startup_repo_roots(&self) -> Vec<String> {
        let mut persisted_raw_repo_roots = std::collections::HashMap::<String, u64>::new();
        let mut live_repo_roots = std::collections::BTreeSet::new();

        {
            let contexts = self.thread_work_contexts.read().await;
            for context in contexts.values() {
                for entry in &context.entries {
                    if let Some(repo_root) = entry
                        .repo_root
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        persisted_raw_repo_roots
                            .entry(repo_root.to_string())
                            .and_modify(|updated_at| {
                                *updated_at = (*updated_at).max(entry.updated_at)
                            })
                            .or_insert(entry.updated_at);
                    }
                }
            }
        }

        for session in self.session_manager.list().await {
            let Some(cwd) = session.cwd else {
                continue;
            };
            if let Some(repo_root) = crate::git::find_git_root(&cwd) {
                if let Some(repo_root) = canonicalize_aline_startup_repo_root(&repo_root).await {
                    live_repo_roots.insert(repo_root);
                }
            }
        }

        if !live_repo_roots.is_empty() {
            return live_repo_roots.into_iter().collect();
        }

        let Some(latest_updated_at) = persisted_raw_repo_roots.values().copied().max() else {
            return Vec::new();
        };

        let mut canonical_persisted_repo_roots = std::collections::HashMap::<String, u64>::new();
        for (raw_repo_root, updated_at) in persisted_raw_repo_roots {
            if updated_at != latest_updated_at {
                continue;
            }
            if let Some(repo_root) = canonicalize_aline_startup_repo_root(&raw_repo_root).await {
                canonical_persisted_repo_roots
                    .entry(repo_root)
                    .and_modify(|existing| *existing = (*existing).max(updated_at))
                    .or_insert(updated_at);
            }
        }

        let Some(latest_updated_at) = canonical_persisted_repo_roots.values().copied().max() else {
            return Vec::new();
        };

        canonical_persisted_repo_roots
            .into_iter()
            .filter_map(|(repo_root, updated_at)| {
                (updated_at == latest_updated_at).then_some(repo_root)
            })
            .collect()
    }

    fn schedule_builtin_skill_seed_and_catalog_sync(self: &Arc<Self>) {
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            seed_builtin_skills(&engine.data_dir);
            seed_builtin_guidelines(&engine.data_dir);
            let skills_root = super::skills_dir(&engine.data_dir);
            if let Err(error) =
                super::skill_recommendation::sync_skill_catalog(&engine.history, &skills_root).await
            {
                tracing::warn!(
                    skills_root = %skills_root.display(),
                    %error,
                    "background built-in skill catalog sync failed after hydrate"
                );
            }
        });
    }

    pub(crate) fn schedule_participant_observer_restore_after_hydrate(self: &Arc<Self>) {
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            engine
                .restore_participant_observer_state_after_hydrate()
                .await;
        });
    }

    fn schedule_aline_startup_reconciliation(self: &Arc<Self>, repo_root: String) {
        if self
            .aline_startup_reconcile_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let engine = Arc::clone(self);
        tokio::spawn(async move {
            #[cfg(test)]
            if engine.aline_startup_test_runner.get().is_some() {
                if let Err(error) = engine
                    .run_aline_startup_reconciliation(PathBuf::from(&repo_root))
                    .await
                {
                    tracing::warn!(repo_root = %repo_root, %error, "Aline startup reconciliation failed");
                }
                engine.notify_aline_startup_reconciliation_finished_for_tests();
                return;
            }

            let completion = match super::aline_startup::run_aline_startup_subprocess(
                super::aline_startup::AsyncAlineStartupRequest {
                    repo_root: repo_root.clone(),
                    data_dir: engine.data_dir.clone(),
                },
            )
            .await
            {
                Ok(completion) => completion,
                Err(error) => {
                    tracing::warn!(repo_root = %repo_root, %error, "Aline startup reconciliation worker failed");
                    engine.notify_aline_startup_reconciliation_finished_for_tests();
                    return;
                }
            };

            if let Some(summary) = completion.summary {
                engine.record_aline_startup_summary(summary.clone()).await;
                super::log_aline_startup_summary(
                    std::path::Path::new(&completion.repo_root),
                    &summary,
                );
            } else if let Some(error) = completion.error {
                tracing::warn!(repo_root = %completion.repo_root, %error, "Aline startup reconciliation worker returned an error");
            }
            engine.notify_aline_startup_reconciliation_finished_for_tests();
        });
    }

    fn schedule_aline_watcher_bootstrap(self: &Arc<Self>) {
        if !self.aline_startup_is_available() {
            return;
        }

        let engine = Arc::clone(self);
        tokio::spawn(async move {
            let runner = engine.aline_startup_command_runner();
            let _ = super::aline_startup::ensure_watcher_running(runner.as_ref()).await;
            engine.notify_aline_startup_reconciliation_finished_for_tests();
        });
    }
}

#[cfg(test)]
#[path = "tests/persistence.rs"]
mod tests;
